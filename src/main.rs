use serde_json::{Value, json, to_string_pretty};
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

const RPC_VERSION: &str = "2.0";
const LANGUAGE_ID: &str = "c";

fn file_uri(file_path: &str) -> String {
    format!("file://{file_path}")
}

fn create_request(method: &str, params: &Value, id: Option<i32>) -> Value {
    let mut request = json!({
        "jsonrpc": RPC_VERSION,
        "method": method,
        "params": params,
    });

    if let Some(id_value) = id {
        request["id"] = json!(id_value);
    }

    request
}

fn generate_rpc_request(request: &Value) -> Vec<u8> {
    let request_json = request.to_string() + "\r\n";
    let content_length = request_json.len();
    format!("Content-Length: {content_length}\r\n\r\n{request_json}")
        .as_bytes()
        .to_vec()
}

fn initialize_request(n: i32) -> Vec<u8> {
    let request = create_request("initialize", &json!({}), Some(n));
    generate_rpc_request(&request)
}

fn did_open_request(file_uri_str: &str, source: &str) -> Vec<u8> {
    let request = create_request(
        "textDocument/didOpen",
        &json!({
            "textDocument": {
                "uri": file_uri_str,
                "languageId": LANGUAGE_ID,
                "version": 1,
                "text": source
            }
        }),
        None,
    );
    generate_rpc_request(&request)
}

fn definition_request(n: i32, file_uri_str: &str, line: usize, character: usize) -> Vec<u8> {
    let request = create_request(
        "textDocument/definition",
        &json!({
            "textDocument": {
                "uri": file_uri_str
            },
            "position": {
                "line": line,
                "character": character
            }
        }),
        Some(n),
    );
    generate_rpc_request(&request)
}

fn document_symbol_request(n: i32, file_uri_str: &str) -> Vec<u8> {
    let request = create_request(
        "textDocument/documentSymbol",
        &json!({
            "textDocument": {
                "uri": file_uri_str
            }
        }),
        Some(n),
    );
    generate_rpc_request(&request)
}

fn did_close_request(file_uri_str: &str) -> Vec<u8> {
    let request = create_request(
        "textDocument/didClose",
        &json!({
            "textDocument": {
                "uri": file_uri_str
            }
        }),
        None,
    );
    generate_rpc_request(&request)
}

fn exit_request() -> Vec<u8> {
    let request = create_request("exit", &Value::Null, None);
    generate_rpc_request(&request)
}

fn process_file(file_path: &PathBuf) -> Result<(String, String), String> {
    let current_file = fs::canonicalize(file_path)
        .map_err(|_| "Error: Unable to canonicalize file path".to_string())?;
    let current_file_str = current_file
        .to_str()
        .expect("Error: Unable to convert path to string");
    let file_uri_str = file_uri(current_file_str);

    let source =
        fs::read_to_string(file_path).map_err(|_| "Error: Unable to read file".to_string())?;

    Ok((file_uri_str, source))
}

fn readline() -> io::Result<String> {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    Ok(buffer.to_string())
}

struct Count(i32);

impl Count {
    fn inc(&mut self) -> i32 {
        self.0 += 1;
        self.0
    }
}

fn start_server_process(command: &str) -> Result<std::process::Child, String> {
    Command::new(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start server: {e}"))
}

fn handle_stdin(
    mut stdin: std::process::ChildStdin,
    count: &Arc<Mutex<Count>>,
    file_uri: &str,
    source: &str,
    commands: &Arc<Mutex<Vec<Value>>>,
) -> Result<(), String> {
    {
        let mut count_guard = count.lock().expect("Failed to lock count");
        stdin
            .write_all(&initialize_request(count_guard.inc()))
            .map_err(|e| format!("Failed to write initialize request: {e}"))?;
    }

    stdin
        .write_all(&did_open_request(file_uri, source))
        .map_err(|e| format!("Failed to write didOpen request: {e}"))?;

    loop {
        let command = readline().map_err(|e| format!("Failed to read command: {e}"))?;

        if command.is_empty() {
            break;
        }

        let mut count_guard = count.lock().expect("Failed to lock count");
        let mut commands_guard = commands.lock().expect("Failed to lock commands");

        match command.trim() {
            "help" => {
                println!("Available commands: def, sym, quit");
                commands_guard.push(json!("help"));
            }
            "def" => {
                let request = definition_request(count_guard.inc(), file_uri, 9, 4);
                stdin
                    .write_all(&request)
                    .map_err(|e| format!("Failed to write definition request: {e}"))?;

                let request_json = String::from_utf8_lossy(&request);
                let json_value: Value = serde_json::from_str(
                    request_json
                        .split("\r\n\r\n")
                        .last()
                        .expect("Failed to split request"),
                )
                .expect("Failed to parse JSON");
                commands_guard.push(json_value);
            }
            "sym" => {
                let request = document_symbol_request(count_guard.inc(), file_uri);
                drop(count_guard);
                stdin
                    .write_all(&request)
                    .map_err(|e| format!("Failed to write documentSymbol request: {e}"))?;

                let request_json = String::from_utf8_lossy(&request);
                let json_value: Value = serde_json::from_str(
                    request_json
                        .split("\r\n\r\n")
                        .last()
                        .expect("Failed to split request"),
                )
                .expect("Failed to parse JSON");
                commands_guard.push(json_value);
            }
            "quit" => {
                commands_guard.push(json!("quit"));
                break;
            }
            _ => {
                eprintln!("Unknown command: {command}");
                commands_guard.push(json!("unknown"));
            }
        }
    }

    stdin
        .write_all(&did_close_request(file_uri))
        .map_err(|e| format!("Failed to write didClose request: {e}"))?;

    stdin
        .write_all(&exit_request())
        .map_err(|e| format!("Failed to write exit request: {e}"))?;

    Ok(())
}

fn consume_json_rpc_message(reader: &mut BufReader<impl Read>) -> Option<Value> {
    let red = "\x1b[31m";
    let normal = "\x1b[0m";
    let yellow = "\x1b[33m";

    let mut line = String::new();
    if reader
        .read_line(&mut line)
        .expect("Failed to read line from stdout")
        == 0
    {
        return None; // EOF
    }

    if line.starts_with("Content-Length: ") {
        let length_str = line.trim_start_matches("Content-Length: ");
        let length: usize = length_str
            .trim()
            .parse()
            .map_err(|e| format!("Failed to parse Content-Length: {e}"))
            .expect("Failed to parse Content-Length");

        // Read the delimiter (\r\n\r\n)
        let mut delimiter = [0; 2];
        reader
            .read_exact(&mut delimiter)
            .map_err(|e| format!("Failed to read delimiter: {e}"))
            .expect("Failed to read delimiter");

        // Read the JSON message
        let mut json_buffer = vec![0; length];
        reader
            .read_exact(&mut json_buffer)
            .map_err(|e| format!("Failed to read JSON message: {e}"))
            .expect("Failed to read JSON message");

        let json_str = String::from_utf8_lossy(&json_buffer);
        let json_str = json_str.trim_end();
        if json_str.is_empty() {
            eprintln!("Received empty JSON message");
            return None;
        }

        if let Ok(json_value) = serde_json::from_str::<Value>(json_str) {
            return Some(json_value);
        }

        println!("{yellow}{json_str}{normal}");
    } else {
        eprintln!("Unexpected line: {red}{line}{normal}");
    }

    None
}

fn display_range(range: &Value) {
    if let Some(end) = range.get("end") {
        if let Some(start) = range.get("start") {
            println!(
                "Start: line {}, character {}, End: line {}, character {}",
                start
                    .get("line")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(-1),
                start
                    .get("character")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(-1),
                end.get("line")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(-1),
                end.get("character")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(-1)
            );
        } else {
            println!("Start range missing.");
        }
    } else {
        println!("Range missing.");
    }
}

fn display_definition(json_value: &Value) -> Result<(), String> {
    if let Some(result) = json_value.get("result") {
        if result.is_null() {
            println!("No definition found.");
        } else if let Some(results) = result.as_array() {
            if results.is_empty() {
                println!("No definition found.");
            } else {
                for item in results {
                    if let Some(uri) = item.get("uri") {
                        println!("Definition found at URI: {uri}");
                        if let Some(range) = item.get("range") {
                            display_range(range);
                        } else {
                            println!("Definition found but range is missing.");
                        }
                    } else {
                        println!("Definition found but URI is missing.");
                    }
                }
            }
        }
        return Ok(());
    }

    Err("No result found in JSON response".to_string())
}

fn display_symbols(json_value: &Value) -> Result<(), String> {
    let symbols = json_value
        .get("result")
        .ok_or("No result found in JSON response")?
        .as_array()
        .ok_or("No symbols found.")?;

    if symbols.is_empty() {
        return Err("No symbols found.".to_string());
    }

    for symbol in symbols {
        let name = symbol
            .get("name")
            .ok_or("Symbol found but name is missing.")?
            .as_str()
            .ok_or("Invalid symbol name")?;

        let location = symbol
            .get("location")
            .ok_or("Symbol found but location is missing.")?;
        let range = location
            .get("range")
            .ok_or("Symbol location found but range is missing.")?;

        let uri = location
            .get("uri")
            .ok_or("Symbol location found but URI is missing.")?
            .as_str()
            .ok_or("Invalid symbol URI")?;

        println!("Symbol name: {name}\nSymbol URI: {uri}\nSymbol location range:");
        display_range(range);
    }

    Ok(())
}

fn display_json_rpc_message(
    json_value: Option<Value>,
    commands: &Arc<Mutex<Vec<Value>>>,
) -> Result<(), String> {
    if let Some(value) = json_value {
        if let Some(id) = value.get("id") {
            let commands_guard = commands.lock().expect("Failed to lock commands");
            for command in commands_guard.iter() {
                if command.get("id") == Some(id) {
                    let method = command
                        .get("method")
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown method");

                    match method {
                        "textDocument/definition" => {
                            display_definition(&value)?;
                        }
                        "textDocument/documentSymbol" => {
                            display_symbols(&value)?;
                        }
                        _ => {
                            let command = to_string_pretty(command)
                                .map_err(|e| format!("Failed to format JSON: {e}"))
                                .unwrap_or_else(|_| "Failed to format JSON".to_string());
                            let response = to_string_pretty(&value)
                                .map_err(|e| format!("Failed to format JSON: {e}"))
                                .unwrap_or_else(|_| "Failed to format JSON".to_string());

                            println!("Command: {command}");
                            println!("Response: {response}",);
                        }
                    }
                    return Ok(());
                }
            }
        }

        let pretty_json =
            to_string_pretty(&value).map_err(|e| format!("Failed to format JSON: {e}"))?;

        let normal = "\x1b[0m";
        let green = "\x1b[32m";

        println!("{green}{pretty_json}{normal}");

        Ok(())
    } else {
        Err("No JSON message received".to_string())
    }
}

fn handle_stdout(stdout: std::process::ChildStdout, commands: &Arc<Mutex<Vec<Value>>>) {
    let mut reader = BufReader::new(stdout);

    loop {
        let json_value = consume_json_rpc_message(&mut reader);
        if let Err(e) = display_json_rpc_message(json_value.clone(), commands) {
            eprintln!("{e}");
            break;
        }
    }
}

fn handle_stderr(stderr: std::process::ChildStderr) -> Result<(), String> {
    let reader = BufReader::new(stderr);
    let red = "\x1b[31m";
    let normal = "\x1b[0m";

    for line in reader.lines() {
        let line = line.map_err(|e| format!("Failed to read line from stderr: {e}"))?;
        eprintln!("{red}stderr: {}{normal}", line.trim_end());
    }

    Ok(())
}

fn flush() {
    io::stdout().flush().expect("Failed to flush stdout");
}

fn run_server(command: &str, print_stderr: bool) {
    let mut child = match start_server_process(command) {
        Ok(child) => child,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let count = Arc::new(Mutex::new(Count(0)));
    let commands = Arc::new(Mutex::new(Vec::new()));

    let stdin = child.stdin.take().expect("Failed to open stdin");

    print!("Enter filename (Default main.c): ");
    flush();
    let mut filename = readline()
        .expect("Failed to read filename")
        .trim()
        .to_string();

    if filename.is_empty() {
        filename = "main.c".to_string();
    }

    let (file_uri, source) = process_file(&PathBuf::from(filename)).expect("Error processing file");

    let commands_clone = commands.clone();
    let stdin_handle = thread::spawn(move || {
        if let Err(e) = handle_stdin(stdin, &count, &file_uri, &source, &commands_clone) {
            eprintln!("{e}");
        }
    });

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let commands_clone = commands;
    let stdout_handle = thread::spawn(move || {
        handle_stdout(stdout, &commands_clone);
    });

    let stderr_handle = if print_stderr {
        let stderr = child.stderr.take().expect("Failed to open stderr");
        Some(thread::spawn(move || {
            if let Err(e) = handle_stderr(stderr) {
                eprintln!("{e}");
            }
        }))
    } else {
        None
    };

    stdin_handle.join().expect("Failed to join stdin thread");
    stdout_handle.join().expect("Failed to join stdout thread");

    if let Some(stderr_handle) = stderr_handle {
        stderr_handle.join().expect("Failed to join stderr thread");
    }

    let status = child.wait().expect("Failed to wait on child process");
    if !status.success() {
        eprintln!("Command exited with status: {status}");
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <command>", args[0]);
        std::process::exit(1);
    }

    run_server(&args[1], false);
}

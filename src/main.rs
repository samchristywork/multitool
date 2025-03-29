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
    last_command: &Arc<Mutex<Value>>,
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
        let mut last_command_guard = last_command.lock().expect("Failed to lock last_command");

        match command.trim() {
            "help" => {
                println!("Available commands: def, sym, quit");
                *last_command_guard = json!("help");
            }
            "def" => {
                let request = definition_request(count_guard.inc(), file_uri, 0, 28);
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
                *last_command_guard = json_value;
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
                *last_command_guard = json_value;
            }
            "quit" => {
                *last_command_guard = json!("quit");
                break;
            }
            _ => {
                eprintln!("Unknown command: {command}");
                *last_command_guard = json!("unknown");
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

fn handle_stdout(
    stdout: std::process::ChildStdout,
    last_command: &Arc<Mutex<Value>>,
) -> Result<(), String> {
    let mut reader = BufReader::new(stdout);

    let normal = "\x1b[0m";
    let green = "\x1b[32m";

    loop {
        let json_value = consume_json_rpc_message(&mut reader);

        // Pretty print last_command
        let last_command_guard = last_command.lock().expect("Failed to lock last_command");
        println!(
            "Last command: {}",
            to_string_pretty(&*last_command_guard).expect("Failed to pretty print last_command")
        );
        drop(last_command_guard);

        // Pretty print JSON message
        if let Some(json_value) = json_value {
            let pretty_json =
                to_string_pretty(&json_value).map_err(|e| format!("Failed to format JSON: {e}"))?;

            println!("{green}{pretty_json}{normal}");
        } else {
            break;
        }
    }

    Ok(())
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
    let last_command = Arc::new(Mutex::new(json!("")));
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

    let last_command_clone = last_command.clone();
    let stdin_handle = thread::spawn(move || {
        if let Err(e) = handle_stdin(stdin, &count, &file_uri, &source, &last_command_clone) {
            eprintln!("{e}");
        }
    });

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let last_command_clone = last_command;
    let stdout_handle = thread::spawn(move || {
        if let Err(e) = handle_stdout(stdout, &last_command_clone) {
            eprintln!("{e}");
        }
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

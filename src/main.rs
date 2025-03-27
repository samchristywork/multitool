use serde_json::{Value, json, to_string_pretty};
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

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
    Ok(buffer.trim_end().to_string())
}

struct Count(i32);

impl Count {
    fn inc(&mut self) -> i32 {
        self.0 += 1;
        self.0
    }
}

fn run_server(command: &str) {
    let mut child = Command::new(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    let mut count = Count(0);
    let mut stdin = child.stdin.take().expect("Failed to open stdin");

    std::thread::spawn(move || {
        stdin
            .write_all(&initialize_request(count.inc()))
            .expect("Failed to write initialize request");

        let filename = readline().expect("Failed to read filename");
        let (file_uri, source) =
            process_file(&PathBuf::from(filename)).expect("Error processing file");

        stdin
            .write_all(&did_open_request(&file_uri, &source))
            .expect("Failed to write didOpen request");

        loop {
            let command = readline().expect("Failed to read command");

            match command.as_str() {
                "help" => {
                    println!("Available commands: def, sym, quit");
                }
                "def" => {
                    stdin
                        .write_all(&definition_request(count.inc(), &file_uri, 0, 28))
                        .expect("Failed to write definition request");
                }
                "sym" => {
                    stdin
                        .write_all(&document_symbol_request(count.inc(), &file_uri))
                        .expect("Failed to write documentSymbol request");
                }
                "quit" => break,
                _ => eprintln!("Unknown command: {command}"),
            }
        }

        stdin
            .write_all(&did_close_request(&file_uri))
            .expect("Failed to write didClose request");

        stdin
            .write_all(&exit_request())
            .expect("Failed to write exit request");
    });

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);

    let red = "\x1b[31m";
    let normal = "\x1b[0m";
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";

    std::thread::spawn(move || {
        for line_result in reader.lines() {
            let line = line_result.expect("Failed to read line");
            if let Ok(json_value) = serde_json::from_str::<Value>(&line) {
                let pretty_json = to_string_pretty(&json_value).expect("Failed to format JSON");
                println!("{green}{pretty_json}{normal}");
            } else {
                println!("{yellow}{line}{normal}");
            }
        }
    });

    let stderr = child.stderr.take().expect("Failed to open stderr");
    let mut stderr_reader = BufReader::new(stderr);

    let mut err_line = String::new();

    while stderr_reader
        .read_line(&mut err_line)
        .expect("Failed to read stderr")
        > 0
    {
        eprintln!("{red}stderr: {}{normal}", err_line.trim_end());
        err_line.clear();
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

    run_server(&args[1]);
}

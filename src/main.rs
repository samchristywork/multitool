use serde_json::{Value, json, to_string_pretty};
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
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
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    let mut line = String::new();
    handle.read_line(&mut line)?;
    Ok(line.trim_end().to_string())
}

fn run_server(command: &str) {
    let mut child = Command::new(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::thread::spawn(move || {
        stdin
            .write_all(&initialize_request(1))
            .expect("Failed to write initialize request");

        let filename = readline().expect("Failed to read filename");
        let (file_uri, source) =
            process_file(&PathBuf::from(filename)).expect("Error processing file");

        stdin
            .write_all(&did_open_request(&file_uri, &source))
            .expect("Failed to write didOpen request");

        stdin
            .write_all(&definition_request(2, &file_uri, 0, 28))
            .expect("Failed to write definition request");

        stdin
            .write_all(&document_symbol_request(3, &file_uri))
            .expect("Failed to write documentSymbol request");

        stdin
            .write_all(&did_close_request(&file_uri))
            .expect("Failed to write didClose request");

        stdin
            .write_all(&exit_request())
            .expect("Failed to write exit request");
    });

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);

    for line_result in reader.lines() {
        let line = line_result.expect("Failed to read line");
        if let Ok(json_value) = serde_json::from_str::<Value>(&line) {
            let pretty_json = to_string_pretty(&json_value).expect("Failed to format JSON");
            println!("{}", pretty_json);
        } else {
            println!("{}", line);
        }
    }

    let stderr = child.stderr.take().expect("Failed to open stderr");
    let mut stderr_reader = BufReader::new(stderr);

    let mut err_line = String::new();

    while stderr_reader.read_line(&mut err_line).unwrap() > 0 {
        eprintln!("stderr: {}", err_line.trim_end());
        err_line.clear();
    }
    let status = child.wait().expect("Failed to wait on child process");
    if !status.success() {
        eprintln!("Command exited with status: {}", status);
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

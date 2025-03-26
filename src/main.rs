use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::io::Write;

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

fn generate_rpc_request(request: &Value) -> String {
    let request_json = request.to_string() + "\r\n";
    let content_length = request_json.len();
    format!("Content-Length: {content_length}\r\n\r\n{request_json}")
}

fn generate_rpc_requests(requests: &[Value]) -> String {
    let mut content = String::new();
    for request in requests {
        content += &generate_rpc_request(request);
    }

    content
}

fn build_requests(file_uri_str: &str, source: &str) -> Vec<Value> {
    vec![
        create_request("initialize", &json!({}), Some(1)),
        create_request(
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
        ),
        create_request(
            "textDocument/definition",
            &json!({
                "textDocument": {
                    "uri": file_uri_str
                },
                "position": {
                    "line": 0,
                    "character": 28
                }
            }),
            Some(2),
        ),
        create_request(
            "textDocument/documentSymbol",
            &json!({
                "textDocument": {
                    "uri": file_uri_str
                }
            }),
            Some(3),
        ),
        create_request(
            "textDocument/didClose",
            &json!({
                "textDocument": {
                    "uri": file_uri_str
                }
            }),
            None,
        ),
        create_request("exit", &Value::Null, None),
    ]
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

fn run_server(command: &str, input: String) {
    let mut child = std::process::Command::new(command)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("Error: Unable to start the server");

    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            stdin.write_all(input.as_bytes()).expect("Error: Unable to write to stdin");
        });
    }

    let output = child.wait_with_output().expect("Error: Unable to read server output");
    println!("{}", String::from_utf8_lossy(&output.stdout));
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <file> <command>", args[0]);
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);
    let command = &args[2];

    let (file_uri_str, source) = match process_file(&file_path) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    };

    let requests = build_requests(&file_uri_str, &source);
    let input = generate_rpc_requests(&requests);

    run_server(command, input);
}

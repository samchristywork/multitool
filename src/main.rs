use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::PathBuf;

const RPC_VERSION: &str = "2.0";
const LANGUAGE_ID: &str = "c";

fn file_uri(file_path: &str) -> String {
    format!("file://{}", file_path)
}

fn create_request(method: &str, params: Value, id: Option<i32>) -> Value {
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

fn print_rpc_request(request: &Value) {
    let request_json = request.to_string() + "\r\n";
    let content_length = request_json.as_bytes().len();
    println!("Content-Length: {}\r\n\r\n{}", content_length, request_json);
}

fn print_rpc_requests(requests: &[Value]) {
    for request in requests {
        print_rpc_request(request);
    }
}

fn build_requests(file_uri_str: &str, source: &str) -> Vec<Value> {
    vec![
        create_request("initialize", json!({}), Some(1)),
        create_request(
            "textDocument/didOpen",
            json!({
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
            json!({
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
            json!({
                "textDocument": {
                    "uri": file_uri_str
                }
            }),
            Some(3),
        ),
        create_request(
            "textDocument/didClose",
            json!({
                "textDocument": {
                    "uri": file_uri_str
                }
            }),
            None,
        ),
        create_request("exit", Value::Null, None),
    ]
}

fn process_file(file_path: &PathBuf) -> Result<(String, String), String> {
    let current_file = fs::canonicalize(file_path)
        .map_err(|_| "Error: Unable to canonicalize file path".to_string())?;
    let current_file_str = current_file.to_str().unwrap();
    let file_uri_str = file_uri(current_file_str);

    let source =
        fs::read_to_string(file_path).map_err(|_| "Error: Unable to read file".to_string())?;

    Ok((file_uri_str, source))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <file>", args[0]);
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);

    let (file_uri_str, source) = match process_file(&file_path) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("{}", err);
            std::process::exit(1);
        }
    };

    let requests = build_requests(&file_uri_str, &source);
    print_rpc_requests(&requests);
}

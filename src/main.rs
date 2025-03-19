use serde_json::{json, Value};
use std::env;
use std::fs;

fn print_rpc_request(request: &Value) {
    let request_json = request.to_string() + "\r\n";
    let content_length = request_json.as_bytes().len();
    println!(
        "Content-Length: {}\r\n\r\n{}",
        content_length, request_json
    );
}

fn print_rpc_requests(requests: &[Value]) {
    for request in requests {
        print_rpc_request(request);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <file>", args[0]);
        std::process::exit(1);
    }

    let current_file = fs::canonicalize(&args[1]).unwrap_or_else(|_| {
        eprintln!("Error: Unable to canonicalize file path");
        std::process::exit(1);
    });
    let current_file = current_file.to_str().unwrap();

    let source = fs::read_to_string(&args[1]).unwrap_or_else(|_| {
        eprintln!("Error: Unable to read file");
        std::process::exit(1);
    });

    let requests = vec![
        json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {},
            "id": 1
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", current_file),
                    "languageId": "c",
                    "version": 1,
                    "text": source
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/definition",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", current_file)
                },
                "position": {
                    "line": 0,
                    "character": 28
                }
            },
            "id": 2
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/documentSymbol",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", current_file)
                }
            },
            "id": 3
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {
                "textDocument": {
                    "uri": format!("file://{}", current_file)
                }
            }
        }),
        json!({
            "jsonrpc": "2.0",
            "method": "exit",
            "params": Value::Null
        }),
        ];

    print_rpc_requests(&requests);
}

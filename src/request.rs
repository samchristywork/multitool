use crate::Count;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::io;

const RPC_VERSION: &str = "2.0";
const LANGUAGE_ID: &str = "c";

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

pub fn initialize_request(n: i32) -> Vec<u8> {
    let request = create_request("initialize", &json!({}), Some(n));
    generate_rpc_request(&request)
}

pub fn did_open_request(file_uri_str: &str, source: &str) -> Vec<u8> {
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

fn reference_request(n: i32, file_uri_str: &str, line: usize, character: usize) -> Vec<u8> {
    let request = create_request(
        "textDocument/references",
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

pub fn did_close_request(file_uri_str: &str) -> Vec<u8> {
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

pub fn exit_request() -> Vec<u8> {
    let request = create_request("exit", &Value::Null, None);
    generate_rpc_request(&request)
}

pub fn handle_command(
    count: &Arc<Mutex<Count>>,
    commands: &std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    file_uri: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer).expect("Failed to read line");
    let command = buffer.to_string();

    if command.is_empty() {
        return Ok(None);
    }

    let mut count_guard = count.lock().expect("Failed to lock count");
    let mut commands_guard = commands.lock().expect("Failed to lock commands");

    let available = "help, def, ref, sym, quit";

    Ok(match command.trim() {
        "help" => {
            println!("Available commands: {available}");
            commands_guard.push(json!("help"));
            None
        }
        "def" => {
            let request = definition_request(count_guard.inc(), file_uri, 9, 4);
            let request_json = String::from_utf8_lossy(&request);
            let json_value: Value = serde_json::from_str(
                request_json
                    .split("\r\n\r\n")
                    .last()
                    .expect("Failed to split request"),
            )
            .expect("Failed to parse JSON");
            commands_guard.push(json_value);

            Some(request)
        }
        "sym" => {
            let request = document_symbol_request(count_guard.inc(), file_uri);
            drop(count_guard);
            let request_json = String::from_utf8_lossy(&request);
            let json_value: Value = serde_json::from_str(
                request_json
                    .split("\r\n\r\n")
                    .last()
                    .expect("Failed to split request"),
            )
            .expect("Failed to parse JSON");
            commands_guard.push(json_value);
            Some(request)
        }
        "quit" => {
            commands_guard.push(json!("quit"));
            None
        }
        _ => {
            eprintln!("Unknown command: {}", command.trim());
            eprintln!("Available commands: {available}");
            commands_guard.push(json!("unknown"));
            None
        }
    })
}

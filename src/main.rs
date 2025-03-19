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
    let requests = vec![
        json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {},
            "id": 1
        }),
        ];

    print_rpc_requests(&requests);
}

use clap::Parser;
use serde_json::{Value, to_string_pretty};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

pub fn format_range(range: &Value) -> Result<String, String> {
    range.get("end").map_or_else(
        || Err("Range end is missing".to_string()),
        |end| {
            range.get("start").map_or_else(
                || Err("Range start is missing".to_string()),
                |start| {
                    Ok(format!(
                        "{}:{}->{}:{}",
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
                    ))
                },
            )
        },
    )
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
                        let uri = uri
                            .as_str()
                            .ok_or("Invalid URI")
                            .map_err(|e| format!("Failed to format URI: {e}"))?;
                        if let Some(range) = item.get("range") {
                            match format_range(range) {
                                Ok(range_str) => {
                                    println!("{uri}\t{range_str}");
                                }
                                Err(e) => {
                                    println!("Failed to format range: {e}");
                                }
                            }
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

fn display_references(json_value: &Value) -> Result<(), String> {
    if let Some(result) = json_value.get("result") {
        if result.is_null() {
            println!("No references found.");
        } else if let Some(results) = result.as_array() {
            if results.is_empty() {
                println!("No references found.");
            } else {
                for item in results {
                    if let Some(uri) = item.get("uri") {
                        let uri = uri
                            .as_str()
                            .ok_or("Invalid URI")
                            .map_err(|e| format!("Failed to format URI: {e}"))?;
                        if let Some(range) = item.get("range") {
                            match format_range(range) {
                                Ok(range_str) => {
                                    println!("{uri}\t{range_str}");
                                }
                                Err(e) => {
                                    println!("Failed to format range: {e}");
                                }
                            }
                        } else {
                            println!("Reference found but range is missing.");
                        }
                    } else {
                        println!("Reference found but URI is missing.");
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

        let range_str = format_range(range)
            .map_err(|e| format!("Failed to format range for symbol '{name}': {e}"))?;
        println!("{uri}\t{range_str}\t{name}");
    }

    Ok(())
}

fn display_message(
    command: &Value,
    value: &Value,
    echo_commands: bool,
    echo_responses: bool,
) -> Result<(), String> {
    let method = command
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or("Unknown method");

    if echo_commands {
        let command = to_string_pretty(command)
            .map_err(|e| format!("Failed to format JSON: {e}"))
            .unwrap_or_else(|_| "Failed to format JSON".to_string());
        println!("Command: {command}");
    }

    if echo_responses {
        let response = to_string_pretty(&value)
            .map_err(|e| format!("Failed to format JSON: {e}"))
            .unwrap_or_else(|_| "Failed to format JSON".to_string());
        println!("Response: {response}",);
    }

    match method {
        "textDocument/definition" => {
            display_definition(value)?;
        }
        "textDocument/references" => {
            display_references(value)?;
        }
        "textDocument/documentSymbol" => {
            display_symbols(value)?;
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

    Ok(())
}

pub fn display_json_rpc_message(
    json_value: Option<Value>,
    commands: &Arc<Mutex<Vec<Value>>>,
    echo_commands: bool,
    echo_responses: bool,
) -> Result<(), String> {
    if let Some(value) = json_value {
        if let Some(id) = value.get("id") {
            let commands_guard = commands.lock().expect("Failed to lock commands");
            for command in commands_guard.iter() {
                if command.get("id") == Some(id) {
                    display_message(command, &value, echo_commands, echo_responses)?;
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

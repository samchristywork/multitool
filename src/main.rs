mod request;
mod display;

use clap::Parser;
use serde_json::{Value, to_string_pretty};
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use request::*;
use display::*;

#[derive(Parser, Debug)]
#[clap(
    author = "Sam Christy",
    version = "1.0",
    about = "A language server client."
)]
struct Args {
    /// The command to execute for the language server
    #[clap(short, long, default_value = "clangd")]
    command: String,

    /// Print stderr from the language server
    #[clap(long)]
    echo_stderr: bool,

    /// Echo the commands sent to the language server
    #[clap(long)]
    echo_commands: bool,

    /// Echo the responses received from the language server
    #[clap(long)]
    echo_responses: bool,

    /// Turn on all echo options
    #[clap(short, long)]
    debug: bool,
}

fn process_file(file_path: &PathBuf) -> Result<(String, String), String> {
    let current_file = fs::canonicalize(file_path)
        .map_err(|_| "Error: Unable to canonicalize file path".to_string())?;
    let current_file_str = current_file
        .to_str()
        .expect("Error: Unable to convert path to string");
    let file_uri_str = format!("file://{current_file_str}");

    let source =
        fs::read_to_string(file_path).map_err(|_| "Error: Unable to read file".to_string())?;

    Ok((file_uri_str, source))
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
        if let Ok(Some(request)) = handle_command(count, commands, file_uri) {
            stdin
                .write_all(&request)
                .map_err(|e| format!("Failed to write reference request: {e}"))?;
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
    commands: &Arc<Mutex<Vec<Value>>>,
    echo_commands: bool,
    echo_responses: bool,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        let json_value = consume_json_rpc_message(&mut reader);
        if let Err(e) =
            display_json_rpc_message(json_value.clone(), commands, echo_commands, echo_responses)
        {
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

fn run_server() {
    let args = Args::parse();

    let mut child = match start_server_process(&args.command) {
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
    io::stdout().flush().expect("Failed to flush stdout");

    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer).expect("Failed to read line");

    let mut filename = buffer
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
        handle_stdout(
            stdout,
            &commands_clone,
            args.echo_commands || args.debug,
            args.echo_responses || args.debug,
        );
    });

    let stderr_handle = if args.echo_stderr || args.debug {
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
    run_server();
}

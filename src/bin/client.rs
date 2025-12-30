use std::io::{self, Write};

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::signal;

const DEFAULT_SERVER_ADDR: &str = "127.0.0.1:7878";

#[derive(Debug)]
enum CliCommand {
    Get { key: String },
    Set { key: String, value: String },
    Del { key: String },
    DbKeysNum,
    DbKeys,
    DbVals,
    DbKeysVals,
    Help,
    Exit,
    Empty,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let server_addr = server_addr();
    println!("Rustdis client connected to http://{server_addr}");
    println!("Type `help` for commands, `exit` to quit.");

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        print!("rustdis> ");
        io::stdout().flush()?;
        line.clear();

        let read_result = tokio::select! {
            res = reader.read_line(&mut line) => res,
            _ = signal::ctrl_c() => {
                println!("");
                break;
            }
        };

        let read = read_result?;
        if read == 0 {
            println!();
            break;
        }

        let command = match parse_command(&line) {
            Ok(cmd) => cmd,
            Err(err) => {
                println!("(error) {err}");
                continue;
            }
        };

        match command {
            CliCommand::Empty => continue,
            CliCommand::Exit => break,
            CliCommand::Help => {
                print_help();
                continue;
            }
            _ => {}
        }

        let (method, target) = build_request_target(&command);
        let response = match send_request(&server_addr, method, &target).await {
            Ok(resp) => resp,
            Err(err) => {
                println!("(error) {err}");
                continue;
            }
        };

        let output = format_response(&command, &response);
        println!("{output}");
    }

    Ok(())
}

fn parse_command(line: &str) -> Result<CliCommand, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(CliCommand::Empty);
    }

    let mut parts = trimmed.split_whitespace();
    let cmd = parts
        .next()
        .ok_or_else(|| "missing command".to_string())?
        .to_ascii_lowercase();

    let command = match cmd.as_str() {
        "get" => {
            let key = parts
                .next()
                .ok_or_else(|| "get requires a key".to_string())?;
            if parts.next().is_some() {
                return Err("get only accepts a single key".to_string());
            }
            CliCommand::Get {
                key: key.to_string(),
            }
        }
        "set" => {
            let key = parts
                .next()
                .ok_or_else(|| "set requires a key".to_string())?;
            let value = parts
                .next()
                .ok_or_else(|| "set requires a value".to_string())?;
            if parts.next().is_some() {
                return Err("set value cannot contain spaces".to_string());
            }
            CliCommand::Set {
                key: key.to_string(),
                value: value.to_string(),
            }
        }
        "del" | "delete" => {
            let key = parts
                .next()
                .ok_or_else(|| "del requires a key".to_string())?;
            if parts.next().is_some() {
                return Err("del only accepts a single key".to_string());
            }
            CliCommand::Del {
                key: key.to_string(),
            }
        }
        "dbkeysnum" | "keysnum" | "count" => CliCommand::DbKeysNum,
        "dbkeys" | "keys" => CliCommand::DbKeys,
        "dbvals" | "vals" | "values" => CliCommand::DbVals,
        "dbkeysvals" | "keysvals" | "all" => CliCommand::DbKeysVals,
        "help" | "?" => CliCommand::Help,
        "exit" | "quit" => CliCommand::Exit,
        _ => return Err(format!("unknown command: {cmd}")),
    };

    Ok(command)
}

fn print_help() {
    println!("Commands:");
    println!("  get <key>");
    println!("  set <key> <value>");
    println!("  del <key>");
    println!("  dbkeysnum   (aliases: keysnum, count)");
    println!("  dbkeys      (aliases: keys)");
    println!("  dbvals      (aliases: vals, values)");
    println!("  dbkeysvals  (aliases: keysvals, all)");
    println!("  exit");
}

fn build_request_target(command: &CliCommand) -> (&'static str, String) {
    match command {
        CliCommand::Get { key } => ("GET", format!("/get?key={}", url_encode(key))),
        CliCommand::Set { key, value } => (
            "POST",
            format!("/set?key={}&value={}", url_encode(key), url_encode(value)),
        ),
        CliCommand::Del { key } => ("DELETE", format!("/del?key={}", url_encode(key))),
        CliCommand::DbKeysNum => ("GET", "/dbkeysnum".to_string()),
        CliCommand::DbKeys => ("GET", "/dbkeys".to_string()),
        CliCommand::DbVals => ("GET", "/dbvals".to_string()),
        CliCommand::DbKeysVals => ("GET", "/dbkeysvals".to_string()),
        CliCommand::Help | CliCommand::Exit | CliCommand::Empty => ("GET", "/".to_string()),
    }
}

async fn send_request(server_addr: &str, method: &str, target: &str) -> Result<Value, String> {
    let mut stream = TcpStream::connect(server_addr)
        .await
        .map_err(|e| format!("failed to connect to {server_addr}: {e}"))?;

    let request =
        format!("{method} {target} HTTP/1.1\r\nHost: {server_addr}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|e| format!("failed to send request: {e}"))?;

    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("failed to read response: {e}"))?;

    let body = extract_body(&buf).ok_or_else(|| "invalid HTTP response".to_string())?;
    serde_json::from_slice(body).map_err(|e| format!("invalid JSON response: {e}"))
}

fn server_addr() -> String {
    std::env::var("SERVER_ADDR").unwrap_or_else(|_| DEFAULT_SERVER_ADDR.to_string())
}

fn extract_body(buf: &[u8]) -> Option<&[u8]> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|pos| &buf[pos + 4..])
}

fn format_response(command: &CliCommand, response: &Value) -> String {
    let ok = response
        .get("ok")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match command {
        CliCommand::Get { .. } => {
            if ok {
                if let Some(value) = response.get("value") {
                    return display_value(value);
                }
            }
            match response.get("error").and_then(|v| v.as_str()) {
                Some("Not found") => "(nil)".to_string(),
                Some(msg) => format!("(error) {msg}"),
                None => "(error) invalid response".to_string(),
            }
        }
        CliCommand::Set { .. } => {
            if ok {
                "OK".to_string()
            } else {
                format_error(response)
            }
        }
        CliCommand::Del { .. } => {
            if ok {
                let removed = response
                    .get("removed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                format!("(integer) {}", if removed { 1 } else { 0 })
            } else {
                format_error(response)
            }
        }
        CliCommand::DbKeysNum => {
            if ok {
                let total = response
                    .get("totalKeys")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                format!("(integer) {total}")
            } else {
                format_error(response)
            }
        }
        CliCommand::DbKeys => {
            if ok {
                let keys = response.get("keys").and_then(|v| v.as_array());
                format_list(keys)
            } else {
                format_error(response)
            }
        }
        CliCommand::DbVals => {
            if ok {
                let values = response.get("values").and_then(|v| v.as_array());
                format_list(values)
            } else {
                format_error(response)
            }
        }
        CliCommand::DbKeysVals => {
            if ok {
                let map = response.get("keysValues").and_then(|v| v.as_object());
                format_map(map)
            } else {
                format_error(response)
            }
        }
        CliCommand::Help | CliCommand::Exit | CliCommand::Empty => {
            "(error) command not supported".to_string()
        }
    }
}

fn format_error(response: &Value) -> String {
    match response.get("error").and_then(|v| v.as_str()) {
        Some(msg) => format!("(error) {msg}"),
        None => "(error) request failed".to_string(),
    }
}

fn format_list(values: Option<&Vec<Value>>) -> String {
    let Some(values) = values else {
        return "(empty list or set)".to_string();
    };
    if values.is_empty() {
        return "(empty list or set)".to_string();
    }

    let mut output = String::new();
    for (idx, value) in values.iter().enumerate() {
        output.push_str(&format!("{}) {}\n", idx + 1, display_value(value)));
    }
    output.trim_end().to_string()
}

fn format_map(map: Option<&serde_json::Map<String, Value>>) -> String {
    let Some(map) = map else {
        return "(empty list or set)".to_string();
    };
    if map.is_empty() {
        return "(empty list or set)".to_string();
    }

    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();

    let mut output = String::new();
    for (idx, key) in keys.iter().enumerate() {
        let value = map.get(*key).unwrap_or(&Value::Null);
        output.push_str(&format!(
            "{}) \"{}\" => {}\n",
            idx + 1,
            key,
            display_value(value)
        ));
    }
    output.trim_end().to_string()
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        _ => value.to_string(),
    }
}

fn url_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_char(b >> 4));
            out.push(hex_char(b & 0x0f));
        }
    }
    out
}

fn is_unreserved(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

fn hex_char(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'A' + (nibble - 10)) as char,
        _ => '0',
    }
}

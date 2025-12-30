//! TCP-IP connection utilities.

use std::sync::Arc;

use serde_json::{Map, Value, json};

use tokio::io::{self, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::server::protocol::command::{Command, ParseError};
use crate::server::protocol::http;
use crate::server::storage::hash_map::HashMap;

/// Generate a minimal JSON HTTP response.
fn build_json_response(status_line: &str, json_body: &str) -> Vec<u8> {
    let length = json_body.as_bytes().len();
    let response = format!(
        "{status_line}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {length}\r\n\
         Connection: close\r\n\
         \r\n\
         {json_body}"
    );
    response.into_bytes()
}

/// Execute command against shared map under a Mutex.
async fn execute_command(map: Arc<Mutex<HashMap<String, String>>>, cmd: Command<'_>) -> Vec<u8> {
    match cmd {
        Command::Get { key } => {
            let key = match http::bytes_to_string(key) {
                Ok(k) => k,
                Err(_) => {
                    return build_json_response(
                        "HTTP/1.1 400 Bad Request",
                        &json!({ "ok": false, "error": "Key must be valid UTF-i" }).to_string(),
                    );
                }
            };

            let guard = map.lock().await;
            if let Some(v) = guard.get(&key) {
                // Minimal JSON escaping omitted for brevity (safe-ish for demo)
                build_json_response(
                    "HTTP/1.1 200 OK",
                    &json!({ "ok": true, "key": key, "value": v }).to_string(),
                )
            } else {
                build_json_response(
                    "HTTP/1.1 200 OK",
                    &json!({ "ok": false, "error": "Not found" }).to_string(),
                )
            }
        }

        Command::Set { key, value } => {
            let key = match http::bytes_to_string(key) {
                Ok(k) => k,
                Err(_) => {
                    return build_json_response(
                        "HTTP/1.1 400 Bad Request",
                        r#"{ "ok": false, "error": "key must be valid UTF-8" }"#,
                    );
                }
            };
            let value = match http::bytes_to_string(value) {
                Ok(v) => v,
                Err(_) => {
                    return build_json_response(
                        "HTTP/1.1 400 Bad Request",
                        r#"{ "ok": false, "error": "value must be valid UTF-8" }"#,
                    );
                }
            };

            let mut guard = map.lock().await;
            guard.insert(key, value);

            build_json_response("HTTP/1.1 200 OK", &json!({ "ok": true }).to_string())
        }

        Command::Del { key } => {
            let key = match http::bytes_to_string(key) {
                Ok(k) => k,
                Err(_) => {
                    return build_json_response(
                        "HTTP/1.1 400 Bad Request",
                        r#"{ "ok": false, "error": "key must be valid UTF-8" }"#,
                    );
                }
            };

            let mut guard = map.lock().await;
            let removed = guard.remove(&key).is_some();

            build_json_response(
                "HTTP/1.1 200 OK",
                &json!({ "ok": true, "removed": removed }).to_string(),
            )
        }

        Command::DbKeysNumber => {
            let guard = map.lock().await;
            let len = guard.len();

            build_json_response(
                "HTTP/1.1 200 OK",
                &json!({ "ok": true, "totalKeys": len }).to_string(),
            )
        }

        Command::DbKeys => {
            let guard = map.lock().await;
            let keys = guard.keys();
            let keys_json = keys.iter().map(|k| json!(k)).collect::<Vec<_>>();

            build_json_response(
                "HTTP/1.1 200 OK",
                &json!({
                    "ok": true,
                    "keys": keys_json,
                })
                .to_string(),
            )
        }

        Command::DbVals => {
            let guard = map.lock().await;
            let values = guard.values();
            let values_json = values.iter().map(|v| json!(v)).collect::<Vec<_>>();

            build_json_response(
                "HTTP/1.1 200 OK",
                &json!({
                    "ok": true,
                    "values": values_json,
                })
                .to_string(),
            )
        }

        Command::DbKeyAndVals => {
            let guard = map.lock().await;
            let mut map = Map::new();
            for (k, v) in guard.iter() {
                map.insert(k.clone(), Value::String(v.clone()));
            }
            std::mem::drop(guard);

            build_json_response(
                "HTTP/1.1 200 OK",
                &json!({ "ok": true, "keysValues": map }).to_string(),
            )
        }
    }
}

/// Connection handler.
///
/// # Parameters
/// - `stream`: TCP stream to read the request from and write the response to.
/// - `datastore`: hash map to insert, read and delete values from based on the HTTP request.
///
/// # Return
/// An I/O result based on if the process was successful.
pub async fn handle_connection(
    mut stream: TcpStream,
    datastore: Arc<Mutex<HashMap<String, String>>>,
) -> io::Result<()> {
    // 1) Read headers
    let (buf, header_end) = match http::read_until_headers(&mut stream).await {
        Ok(v) => v,
        Err(e) => {
            // Could try to respond 400/431; for now respond 400 on parse-ish errors
            let resp = build_json_response(
                "HTTP/1.1 400 Bad Request",
                r#"{ "ok": false, "error": "invalid request" }"#,
            );
            let _ = stream.write_all(&resp).await;
            let _ = stream.shutdown().await;

            return Err(e);
        }
    };

    // 2) Extract request line
    let request_line = match http::request_line_from_headers(&buf, header_end) {
        Ok(l) => l,
        Err(_) => {
            let resp = build_json_response(
                "HTTP/1.1 400 Bad Request",
                r#"{ "ok": false, "error": "bad request line" }"#,
            );
            stream.write_all(&resp).await?;
            stream.shutdown().await?;

            return Ok(());
        }
    };
    println!(
        "Request: {}",
        str::from_utf8(request_line).expect("Could not parse request as UTF-8")
    );

    // 3) Parse command from request line (no allocations here)
    let cmd = match http::parse_command_from_request_line(request_line) {
        Ok(c) => c,
        Err(e) => {
            let (status, body) = match e {
                ParseError::BadRequestLine => (
                    "HTTP/1.1 400 Bad Request",
                    r#"{ "ok": false, "error": "bad request line" }"#,
                ),
                ParseError::UnsupportedMethod => (
                    "HTTP/1.1 405 Method Not Allowed",
                    r#"{ "ok": false, "error": "unsupported method" }"#,
                ),
                ParseError::UnsupportedPath => (
                    "HTTP/1.1 404 Not Found",
                    r#"{ "ok": false, "error": "unsupported path" }"#,
                ),
                ParseError::MissingParam(p) => (
                    "HTTP/1.1 400 Bad Request",
                    // embed p safely (static str)
                    match p {
                        "key" => r#"{ "ok": false, "error": "missing param: key" }"#,
                        "value" => r#"{ "ok": false, "error": "missing param: value" }"#,
                        _ => r#"{ "ok": false, "error": "missing required params" }"#,
                    },
                ),
            };

            let resp = build_json_response(status, body);
            stream.write_all(&resp).await?;
            stream.shutdown().await?;
            return Ok(());
        }
    };

    // 4) Execute against shared map (allocations happen here when converting to String)
    let response_bytes = execute_command(datastore, cmd).await;

    // 5) Respond + close
    stream.write_all(&response_bytes).await?;
    stream.shutdown().await?;
    Ok(())
}

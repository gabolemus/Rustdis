//! Command execution against the datastore.

use std::sync::Arc;

use serde_json::{json, Map, Value};
use tokio::sync::Mutex;

use crate::server::protocol::command::Command;
use crate::server::protocol::http;
use crate::server::storage::hash_map::HashMap;

/// Generate a minimal JSON HTTP response.
pub(crate) fn build_json_response(status_line: &str, json_body: &str) -> Vec<u8> {
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
pub async fn execute_command(
    map: Arc<Mutex<HashMap<String, String>>>,
    cmd: Command<'_>,
) -> Vec<u8> {
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

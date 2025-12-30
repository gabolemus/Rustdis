//! TCP-IP connection utilities.

use std::sync::Arc;

use tokio::io::{self, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::server::handler;
use crate::server::protocol::command::{self, ParseError};
use crate::server::protocol::http;
use crate::server::storage::hash_map::HashMap;

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
            let resp = handler::build_json_response(
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
            let resp = handler::build_json_response(
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
        std::str::from_utf8(request_line).expect("Could not parse request as UTF-8")
    );

    // 3) Parse command from request line (no allocations here)
    let cmd = match command::parse_command_from_request_line(request_line) {
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

            let resp = handler::build_json_response(status, body);
            stream.write_all(&resp).await?;
            stream.shutdown().await?;
            return Ok(());
        }
    };

    // 4) Execute against shared map (allocations happen here when converting to String)
    let response_bytes = handler::execute_command(datastore, cmd).await;

    // 5) Respond + close
    stream.write_all(&response_bytes).await?;
    stream.shutdown().await?;
    Ok(())
}

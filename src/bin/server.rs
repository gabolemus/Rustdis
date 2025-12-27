use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

const MAX_HEADER_BYTES: usize = 32 * 1024; // safety cap
const TMP_BUF_SIZE: usize = 4096;

/// Finds the position where `needle` appears in `haystack` if it exists within.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Find first occurrence of a byte in a slice.
fn find_byte(h: &[u8], b: u8) -> Option<usize> {
    h.iter().position(|&x| x == b)
}

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

#[derive(Debug)]
enum Command<'a> {
    Get { key: &'a [u8] },
    Set { key: &'a [u8], value: &'a [u8] },
    Del { key: &'a [u8] },
}

#[derive(Debug)]
enum ParseError {
    BadRequestLine,
    UnsupportedMethod,
    UnsupportedPath,
    MissingParam(&'static str),
}

/// Parse `a=b&c=d` and return the value slice for `name` if present (no allocations).
fn find_param<'a>(query: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let mut i = 0;
    while i <= query.len() {
        let seg_end = query[i..]
            .iter()
            .position(|&c| c == b'&')
            .map(|p| i + p)
            .unwrap_or(query.len());

        let seg = &query[i..seg_end];
        if let Some(eq) = find_byte(seg, b'=') {
            let k = &seg[..eq];
            if k == name {
                return Some(&seg[eq + 1..]);
            }
        }

        if seg_end == query.len() {
            break;
        }
        i = seg_end + 1;
    }
    None
}

/// Parses only the request line: `METHOD SP TARGET SP HTTP/VERSION`.
/// The returned slices borrow from `request_line`.
fn parse_command_from_request_line<'a>(request_line: &'a [u8]) -> Result<Command<'a>, ParseError> {
    // METHOD
    let sp1 = find_byte(request_line, b' ').ok_or(ParseError::BadRequestLine)?;
    let method = &request_line[..sp1];

    // TARGET
    let rest = &request_line[sp1 + 1..];
    let sp2 = find_byte(rest, b' ').ok_or(ParseError::BadRequestLine)?;
    let target = &rest[..sp2];

    // Split target into path + query
    let (path, query) = match find_byte(target, b'?') {
        Some(q) => (&target[..q], Some(&target[q + 1..])),
        None => (target, None),
    };

    match method {
        b"GET" => {
            if path != b"/get" {
                return Err(ParseError::UnsupportedPath);
            }
            let q = query.ok_or(ParseError::MissingParam("key"))?;
            let key = find_param(q, b"key").ok_or(ParseError::MissingParam("key"))?;
            Ok(Command::Get { key })
        }
        b"DELETE" => {
            if path != b"/del" {
                return Err(ParseError::UnsupportedPath);
            }
            let q = query.ok_or(ParseError::MissingParam("key"))?;
            let key = find_param(q, b"key").ok_or(ParseError::MissingParam("key"))?;
            Ok(Command::Del { key })
        }
        b"POST" => {
            if path != b"/set" {
                return Err(ParseError::UnsupportedPath);
            }
            let q = query.ok_or(ParseError::MissingParam("key/value"))?;
            let key = find_param(q, b"key").ok_or(ParseError::MissingParam("key"))?;
            let value = find_param(q, b"value").ok_or(ParseError::MissingParam("value"))?;
            Ok(Command::Set { key, value })
        }
        _ => Err(ParseError::UnsupportedMethod),
    }
}

/// Convert raw bytes to String (allocates). For now, require valid UTF-8.
/// (If you later want Redis-like binary-safe keys/values, switch to Vec<u8> keys/values.)
fn bytes_to_string(b: &[u8]) -> Result<String, ()> {
    std::str::from_utf8(b).map(|s| s.to_owned()).map_err(|_| ())
}

/// Execute command against shared map under a Mutex.
async fn execute_command(map: Arc<Mutex<HashMap<String, String>>>, cmd: Command<'_>) -> Vec<u8> {
    match cmd {
        Command::Get { key } => {
            let key = match bytes_to_string(key) {
                Ok(k) => k,
                Err(_) => {
                    return build_json_response(
                        "HTTP/1.1 400 Bad Request",
                        r#"{ "ok": false, "error": "key must be valid UTF-8" }"#,
                    );
                }
            };

            let guard = map.lock().await;
            if let Some(v) = guard.get(&key) {
                // Minimal JSON escaping omitted for brevity (safe-ish for demo)
                build_json_response(
                    "HTTP/1.1 200 OK",
                    &format!(r#"{{ "ok": true, "key": "{key}", "value": "{v}" }}"#),
                )
            } else {
                build_json_response(
                    "HTTP/1.1 404 Not Found",
                    r#"{ "ok": false, "error": "not found" }"#,
                )
            }
        }

        Command::Set { key, value } => {
            let key = match bytes_to_string(key) {
                Ok(k) => k,
                Err(_) => {
                    return build_json_response(
                        "HTTP/1.1 400 Bad Request",
                        r#"{ "ok": false, "error": "key must be valid UTF-8" }"#,
                    );
                }
            };
            let value = match bytes_to_string(value) {
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
            build_json_response("HTTP/1.1 200 OK", r#"{ "ok": true }"#)
        }

        Command::Del { key } => {
            let key = match bytes_to_string(key) {
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
                &format!(r#"{{ "ok": true, "removed": {removed} }}"#),
            )
        }
    }
}

/// Reads until the end of headers (`\r\n\r\n`) with safety caps, returning:
/// - Full buffer (includes headers + any extra read).
/// - Index right after header delimiter.
async fn read_until_headers(stream: &mut TcpStream) -> io::Result<(Vec<u8>, usize)> {
    let mut buf: Vec<u8> = Vec::with_capacity(TMP_BUF_SIZE);
    let mut tmp = [0u8; TMP_BUF_SIZE];

    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }

        if buf.len() >= MAX_HEADER_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Headers too large",
            ));
        }

        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Client closed before sending full headers",
            ));
        }

        buf.extend_from_slice(&tmp[..n]);
    };

    Ok((buf, header_end))
}

/// Extract the request line (bytes) from within `buf[..header_end]`.
fn request_line_from_headers<'a>(buf: &'a [u8], header_end: usize) -> io::Result<&'a [u8]> {
    let headers = &buf[..header_end];
    let req_line_end = find_subslice(headers, b"\r\n")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing request line CRLF"))?;
    Ok(&buf[..req_line_end])
}

/// Connection handler.
///
/// # Parameters
/// - `stream`: TCP stream to read the request from and write the response to.
/// - `datastore`: hash map to insert, read and delete values from based on the HTTP request.
///
/// # Return
/// An I/O result based on if the process was successful.
async fn handle_connection(
    mut stream: TcpStream,
    datastore: Arc<Mutex<HashMap<String, String>>>,
) -> io::Result<()> {
    // 1) Read headers
    let (buf, header_end) = match read_until_headers(&mut stream).await {
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
    let request_line = match request_line_from_headers(&buf, header_end) {
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
    let cmd = match parse_command_from_request_line(request_line) {
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

#[tokio::main]
async fn main() -> io::Result<()> {
    let ip = "0.0.0.0:7878";
    let listener = TcpListener::bind(ip).await?;
    println!("Running server on http://{ip}");

    let datastore: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, _addr) = listener.accept().await?;

        let datastore = Arc::clone(&datastore);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, datastore).await {
                eprintln!("Error handling connection: {e}");
            }
        });
    }
}

use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const MAX_HEADER_BYTES: usize = 32 * 1024; // safety cap

/// Finds the position where `needle` appears in `haystack` if it exists within.
///
/// # Parameters
/// - `haystack`: bytes to look within.
/// - `needle`: sequence of bytes to look for.
///
/// # Returns
/// The index where `needle` appears within `haystack`, if it does.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Generates the sequence of bytes that corresponds to the JSON body.
///
/// # Parameters
/// - `status_line`: HTTP status.
/// - `json_body`: JSON object response.
///
/// # Returns
/// The sequence of bytes that represents the JSON response.
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

/// HTTP GET request parser and router
///
/// # Parameters
/// - `request_line`: bytes of the request.
///
/// # Returns
/// The sequence of bytes of the response.
fn route_request_line(request_line: &[u8]) -> Vec<u8> {
    let line = std::str::from_utf8(request_line).unwrap_or("");
    println!("Line: {line}");

    match line {
        "GET /get HTTP/1.1" | "GET /GET HTTP/1.1" => {
            build_json_response("HTTP/1.1 200 OK", r#"{ "message": "GET operation" }"#)
        }
        "POST /set HTTP/1.1" | "POST /SET HTTP/1.1" => {
            build_json_response("HTTP/1.1 200 OK", r#"{ "message": "SET operation" }"#)
        }
        "DELETE /del HTTP/1.1" | "DELETE /DEL HTTP/1.1" => {
            build_json_response("HTTP/1.1 200 OK", r#"{ "message": "DEL operation" }"#)
        }
        _ => build_json_response(
            "HTTP/1.1 404 Not Found",
            r#"{ "message": "Unrecognized command. Available commands are: 'GET', 'SET' and 'DEL'" }"#,
        ),
    }
}

/// Naive TCP connection handler.
///
/// # Parameters
/// - `stream`: TCP stream to read and write from and to.
///
/// # Returns
/// The result of the operation.
async fn handle_connection(mut stream: TcpStream) -> io::Result<()> {
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];

    // 1) Read until we have full headers: "\r\n\r\n"
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4; // Index right after the delimiter
        }

        if buf.len() >= MAX_HEADER_BYTES {
            // Too large / suspicious: drop connection or send 431/400
            return Ok(());
        }

        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            // Client closed before sending full headers
            return Ok(());
        }

        buf.extend_from_slice(&tmp[..n]);
    };

    // 2) Parse request line
    // Request line ends at first "\r\n"
    let req_line_end = find_subslice(&buf[..header_end], b"\r\n")
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing request line CRLF"))?;

    let request_line = &buf[..req_line_end];
    println!(
        "Request line: {:?}",
        std::str::from_utf8(request_line).unwrap_or("<non-utf8>")
    );

    // 3) Route and respond
    let response_bytes = route_request_line(request_line);
    stream.write_all(&response_bytes).await?;
    stream.shutdown().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let ip = "0.0.0.0:7878";
    let listener = TcpListener::bind(ip).await?;
    println!("Running server on http://{ip}");

    loop {
        let (stream, _addr) = listener.accept().await?; // Async accept

        // Spawn a lightweight async task
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Error handling connection: {e}");
            }
        });
    }
}

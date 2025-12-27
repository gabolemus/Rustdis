use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

/// Writes the JSON response to the TCP stream.
///
/// # Params
/// - `writer`: TCP stream to write to.
/// - `status_line`: HTTP response status.
/// - `json_body`: JSON body of the response.
async fn write_json_response(
    mut writer: impl AsyncWriteExt + Unpin,
    status_line: &str,
    json_body: &str,
) -> io::Result<()> {
    let length = json_body.as_bytes().len();

    let response = format!(
        "{status_line}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {length}\r\n\
         Connection: close\r\n\
         \r\n\
         {json_body}"
    );

    writer.write_all(response.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

/// Naïve HTTP get parser.
///
/// # Params
/// - `stream`: TCP stream to read and write to.
async fn handle_connection(stream: TcpStream) -> io::Result<()> {
    // Splitting makes it easy to read and write concurrently
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // 1) Read request line
    let mut request_line = String::new();
    let n = reader.read_line(&mut request_line).await?;
    if n == 0 {
        // Client closed immediately
        return Ok(());
    }

    let request_line = request_line.trim_end_matches(&['\r', '\n'][..]);
    println!("Request line: {request_line}");

    // 2) Drain headers until blank line
    loop {
        let mut header_line = String::new();
        let bytes = reader.read_line(&mut header_line).await?;
        if bytes == 0 {
            // Client closed early
            return Ok(());
        }

        if header_line == "\r\n" || header_line == "\n" {
            break;
        }
    }

    // 3) Naive routing
    if request_line == "GET / HTTP/1.1" {
        write_json_response(
            &mut write_half,
            "HTTP/1.1 200 OK",
            r#"{ "message": "Dummy correct response!" }"#,
        )
        .await?;
    } else {
        write_json_response(
            &mut write_half,
            "HTTP/1.1 404 Not Found",
            r#"{ "message": "Page not found" }"#,
        )
        .await?;
    }

    // 4) Close connection
    write_half.shutdown().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let ip = "0.0.0.0:7878";
    let listener = TcpListener::bind(ip).await?;
    println!("Running server on http://{ip}");

    loop {
        let (stream, _addr) = listener.accept().await?; // async accept

        // Spawn a lightweight async task
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream).await {
                eprintln!("Error handling connection: {e}");
            }
        });
    }
}

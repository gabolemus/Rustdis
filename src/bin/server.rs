use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn write_json_response(
    mut stream: TcpStream,
    status_line: &str,
    json_body: &str,
) -> std::io::Result<()> {
    let length = json_body.as_bytes().len();
    let response = format!(
        "{status_line}\r\n\
        Content-Type: application/json; charset=utf-8\r\n\
        Content-Lenth: {length}\r\n\
        Connection: close\r\n\
        \r\n\
        {json_body}"
    );

    stream.write_all(response.as_bytes())
}

fn handle_connection(stream: TcpStream) -> std::io::Result<()> {
    let mut buf_reader = BufReader::new(&stream);
    let mut request_line = String::new();

    buf_reader
        .read_line(&mut request_line)
        .expect("Failed to read HTTP request line from TCP stream");

    let request_line = request_line.trim_end_matches(&['\r', '\n'][..]);
    println!("Request line: {request_line}");

    // Drain and ignore headers until the blank line
    loop {
        let mut header_line = String::new();
        let bytes = buf_reader
            .read_line(&mut header_line)
            .expect("Failed while reading HTTP request headers");

        if bytes == 0 {
            // Client closed connection early
            break;
        }

        let newlines = vec!["\r\n", "\n"];
        if newlines.contains(&header_line.as_str()) {
            break;
        }
    }

    // Naive routing: only exact "GET / HTTP/1.1"
    if request_line == "GET / HTTP/1.1" {
        write_json_response(
            stream,
            "HTTP/1.1 200 OK",
            r#"{ "message": "Dummy correct response!" }"#,
        )
    } else {
        write_json_response(
            stream,
            "HTTP/1.1 404 Not Found",
            r#"{ "message": "Page not found" }"#,
        )
    }
}

fn main() -> Result<(), std::io::Error> {
    let ip = "127.0.0.1:7878";
    let listener = TcpListener::bind(ip)?;
    println!("Running server on http://{ip}");

    for stream in listener.incoming() {
        let stream = stream.expect("Failed to accept incoming TCP connection");

        thread::spawn(move || {
            if let Err(e) = handle_connection(stream) {
                eprintln!("Error handling connection: {e}");
            }
        });
    }

    Ok(())
}

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

fn handle_connection(mut stream: TcpStream) {
    let buf_reader = BufReader::new(&stream);
    let _http_request: Vec<_> = buf_reader
        .lines()
        .map(|result| result.expect("Could not get HTTP request"))
        .take_while(|line| !line.is_empty())
        .collect();

    let response = "HTTP/1.1 200 OK\r\n\r\n";
    stream
        .write(response.as_bytes())
        .expect("Could not write to TCP stream");
}

fn main() -> Result<(), std::io::Error> {
    let ip = "127.0.0.1:7878";
    let listener = TcpListener::bind(ip)?;
    println!("Running server on http://{ip}");

    for stream in listener.incoming() {
        let stream = stream?;

        handle_connection(stream);
    }

    Ok(())
}

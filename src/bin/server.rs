use std::net::TcpListener;

fn main() -> Result<(), std::io::Error> {
    let ip = "127.0.0.1:7878";
    let listener = TcpListener::bind(ip)?;
    println!("Running server on http://{ip}");

    for stream in listener.incoming() {
        let _stream = stream?;

        println!("Connection established!");
    }

    Ok(())
}

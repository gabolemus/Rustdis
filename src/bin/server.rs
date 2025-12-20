use std::net::TcpListener;

fn main() -> Result<(), std::io::Error> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;

    for stream in listener.incoming() {
        let _stream = stream?;

        println!("Connection established!");
    }

    Ok(())
}

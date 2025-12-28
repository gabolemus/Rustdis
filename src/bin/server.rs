use std::sync::Arc;

use rustdis::server::hash_map::HashMap;
use rustdis::server::runner;
use tokio::io;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ip = "0.0.0.0:7878";
    let listener = TcpListener::bind(ip).await?;
    println!("Running server on http://{ip}");

    let datastore: Arc<Mutex<HashMap>> = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (stream, _addr) = listener.accept().await?;
        let datastore = Arc::clone(&datastore);

        tokio::spawn(async move {
            if let Err(e) = runner::handle_connection(stream, datastore).await {
                eprintln!("Error handling connection: {e}");
            }
        });
    }
}

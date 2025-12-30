use std::sync::Arc;

use rustdis::server::storage::hash_map::HashMap;
use rustdis::server::transport;

use tokio::io;
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::Mutex;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> io::Result<()> {
    let ip = "0.0.0.0:7878";
    let listener = TcpListener::bind(ip).await?;
    println!("Running server on http://{ip}");

    let datastore: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut join_set = JoinSet::new();
    let mut shutdown_requested = false;
    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            accept_result = listener.accept(), if !shutdown_requested => {
                let (stream, _addr) = accept_result?;
                let datastore = Arc::clone(&datastore);

                join_set.spawn(async move {
                    if let Err(e) = transport::handle_connection(stream, datastore).await {
                        eprintln!("Error handling connection: {e}");
                    }
                });
            }
            _ = &mut ctrl_c, if !shutdown_requested => {
                shutdown_requested = true;
                eprintln!();
                eprintln!("Shutdown signal received; will refuse new connections.");
                eprintln!("Press Ctrl+C again to force quit.");
                tokio::spawn(async {
                    if signal::ctrl_c().await.is_ok() {
                        eprintln!();
                        eprintln!("Force quit requested.");
                        std::process::exit(1);
                    }
                });
            }
        }

        if shutdown_requested {
            break;
        }
    }

    while let Some(join_result) = join_set.join_next().await {
        if let Err(e) = join_result {
            eprintln!("Connection task failed: {e}");
        }
    }

    Ok(())
}

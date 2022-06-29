mod model;
mod error;
mod client;
use error::LaboriError;
mod logger;
mod config;
use config::Config;
mod server;
use tokio::sync::mpsc;

const CONFIG_FILENAME: &str = "config.toml";

#[tokio::main]
async fn main() -> Result<(), error::LaboriError> {

    // Load config 
    let config = Config::from_file(CONFIG_FILENAME);

    // Create tokio channel
    let (tx0, rx0) = mpsc::channel(1024);
    let (tx1, rx1) = mpsc::channel(1024);
    let (tx2, rx2) = mpsc::channel(1024);

    // Spawn server, runner, logger
    let server_handle = tokio::spawn(server::serve(config.clone(), tx0, rx1));
    let client_handle = tokio::spawn(client::connect(config.clone(), tx1, tx2, rx0));
    let log_handle = tokio::spawn(logger::log(config, rx2));

    // Join spawned virtual machines
    // let results = tokio::join!(serve_handle, run_handle, log_handle);
    let (r1, r2, r3) = (
        server_handle.await.unwrap(), 
        client_handle.await.unwrap(), 
        log_handle.await.unwrap()
    );
    match (r1, r2, r3) {
        (Ok(_), Ok(_), Ok(_)) => return Ok(()),
        (Err(e), _, _) => return Err(LaboriError::from(e)),
        (_, Err(e), _) => return Err(LaboriError::from(e)),
        (_, _, Err(e)) => return Err(LaboriError::from(e)),
    }

}


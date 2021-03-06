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

    // Spawn server, runner, logger
    let server_handle = tokio::spawn(server::serve(config.clone(), tx0, rx1));
    let client_handle = tokio::spawn(client::connect(config.clone(), tx1, rx0));

    // Join spawned virtual machines
    let (r1, r2) = (
        server_handle.await.unwrap(), 
        client_handle.await.unwrap(), 
    );
    match (r1, r2) {
        (Ok(_), Ok(_)) => return Ok(()),
        (Err(e), _) => return Err(LaboriError::from(e)),
        (_, Err(e)) => return Err(LaboriError::from(e)),
    }

}


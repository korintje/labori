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
    let config_path = std::env::args().nth(1)
        .or_else(|| std::env::var("LABORI_CONFIG").ok())
        .unwrap_or_else(|| CONFIG_FILENAME.to_string());
    let config = Config::from_file(&config_path)?;

    // Create tokio channel
    let (tx0, rx0) = mpsc::channel(1024);
    let (tx1, rx1) = mpsc::channel(1024);

    // Spawn server, runner, logger
    let server_handle = tokio::spawn(server::serve(config.clone(), tx0, rx1));
    let client_handle = tokio::spawn(client::connect(config.clone(), tx1, rx0));

    tokio::select! {
        result = server_handle => {
            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(LaboriError::from(e)),
                Err(e) => Err(LaboriError::APISendError(e.to_string())),
            }
        },
        result = client_handle => {
            match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(LaboriError::from(e)),
                Err(e) => Err(LaboriError::APISendError(e.to_string())),
            }
        },
    }
}


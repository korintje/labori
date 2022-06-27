mod model;
mod runner;
mod error;
use error::LaboriError;
mod logger;
mod config;
use config::Config;
mod server;
use std::path;
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
    let serve_handle = tokio::spawn(server::serve(config.clone(), tx0));
    let run_handle = tokio::spawn(runner::run(config.clone(), tx1, rx0));
    let log_handle = tokio::spawn(logger::log(config, rx1));

    // Join spawned virtual machines
    let results = tokio::join!(serve_handle, run_handle, log_handle);
    match results {
        (Ok(_), Ok(_), Ok(_)) => return Ok(()),
        (Err(e), _, _) => return Err(LaboriError::from(e)),
        (_, Err(e), _) => return Err(LaboriError::from(e)),
        (_, _, Err(e)) => return Err(LaboriError::from(e)),
    }

}


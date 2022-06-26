mod model;
mod load;
mod error;
mod db;
mod config;
mod server;
use std::path;
use tokio::sync::mpsc;


#[tokio::main]
async fn main() -> Result<(), error::LaboriError> {

    let server = server::APIServer::from_config();
    server.listen();

    Ok(())

}


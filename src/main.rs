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

    let dbpath = "test.db";
    if ! path::Path::new(&dbpath).exists() {
        db::create_db(&dbpath).await?;
    }
    let conn = db::connect_db(&dbpath).await?;
    let conn = db::prepare_tables(conn).await?;




    Ok(())

}


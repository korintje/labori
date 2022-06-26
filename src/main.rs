mod model;
mod load;
mod error;
mod db;
mod config;
mod server;
use std::path;
use tokio::{sync::mpsc};
use tokio::net::TcpListener;


#[tokio::main]
async fn main() -> Result<(), error::SQLMDError> {

    let socket_port = config::get_socket_port();
    // let listener = TcpListener::bind(format!("127.0.0.1:{}", socket_port)).await?;

    let (tx1, rx1) = mpsc::channel(1024);
    let listen_handle = tokio::spawn(server::serve(socket_port, tx1));
    /*
    let manipulate_handle = tokio::spawn(server::manipulate(rx1));
    let server_results = tokio::join!(listen_handle, manipulate_handle);
    match server_results {
        (Ok(_), Ok(_)) => println!("##### SUCCESSFULLY SERVED #####"),
        (Err(e), _) => return Err(error::SQLMDError::from(e)),
        (_, Err(e)) => return Err(error::SQLMDError::from(e)),
    }
    */    

    let dbpath = "test.db";

    if ! path::Path::new(&dbpath).exists() {
        db::create_db(&dbpath).await?;
    }
    let conn = db::connect_db(&dbpath).await?;
    let conn = db::prepare_tables(conn).await?;

    // let stdout = io::stdout();
    let (tx0, rx0) = mpsc::channel(1024);
    // let (tx1, rx1) = mpsc::channel(1024);

    let load_handle = tokio::spawn(load::get_data_tcp(tx0));
    let save_handle = tokio::spawn(db::save_db(rx0, conn));
    // let log_handle = tokio::spawn(load::print_log(rx1, stdout));

    let results = tokio::join!(load_handle, save_handle);
    match results {
        (Ok(_), Ok(_)) => println!("##### SUCCESSFULLY HANDLED #####"),
        (Err(e), _) => return Err(error::SQLMDError::from(e)),
        (_, Err(e)) => return Err(error::SQLMDError::from(e)),
        // (_, _, Err(e)) => return Err(error::SQLMDError::from(e)),
    }


    Ok(())

}


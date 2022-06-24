mod model;
mod load;
mod error;
mod db;
use std::path;
use tokio::{sync::mpsc};

#[tokio::main]
async fn main() -> Result<(), error::SQLMDError> {

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


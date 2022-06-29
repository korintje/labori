use std::path::Path;
use sqlx::{migrate::MigrateDatabase, SqliteConnection, Connection, Sqlite, Executor};
use crate::{model, error};
use error::LaboriError;
use tokio::sync::mpsc;
use encoding::{Encoding, DecoderTrap};
use encoding::all::ASCII;

// Create DB file
pub async fn create_db(dbpath: &str) 
-> Result<(), LaboriError> {
  match Sqlite::create_database(&dbpath).await {
    Ok(_) => Ok(()),
    Err(e) => return Err(LaboriError::SQLError(e)),
  }  
}

// Connect to DB and return connection
pub async fn connect_db(dbpath: &str)
-> Result<SqliteConnection, LaboriError> {
  match SqliteConnection::connect(dbpath).await {
    Ok(c) => Ok(c),
    Err(e) => Err(LaboriError::SQLError(e))
  }
}

// Prepare DB tables
pub async fn prepare_tables(mut conn: SqliteConnection) 
-> Result<SqliteConnection, error::LaboriError> {
  let table_count: model::TableCount = sqlx::query_as(
    "SELECT COUNT(*) as count FROM sqlite_master WHERE TYPE='table' AND name=$1"
  )
  .bind("freq")
  .fetch_one(&mut conn)
  .await?;
  if table_count.count == 0 {
    if let Err(e) = conn.execute(sqlx::query(
      "CREATE TABLE IF NOT EXISTS freq (
        step        INTEGER NOT NULL,
        charge      REAL NOT NULL
      )"
    )).await {
      return Err(LaboriError::SQLError(e))
    };
  }
  Ok(conn)
}


pub async fn log(device_name: String, table_name: String, mut rx: mpsc::Receiver<Vec<u8>>) 
-> Result<(), error::LaboriError> {

    let dbpath = format!("{}.db", device_name);
    if ! Path::new(&dbpath).exists() {
        create_db(&dbpath).await?;
    }
    let conn = connect_db(&dbpath).await?;
    let mut conn = prepare_tables(conn).await?;

    // Insert atom parameters into the table
    let mut values = vec![];
    let query_head = format!("INSERT INTO {} VALUES ", &table_name);

    while let Some(buff) = rx.recv().await {

        // Check and remove LF at the end of the buff
        let freqs_u8 :Vec<u8>;
        if buff.last() != Some(&10u8) {
            if buff[0] == 4u8 {
              println!("Stop logging");
              break
            }else{
              println!("Broken stream");
              continue
            }
        } else {
            freqs_u8 = buff[..buff.len()-1].to_vec();
        }

        // Separate by comma, decode to ASCII, parse to f64, and append to vec. 
        freqs_u8.split(|b| *b == 44u8)
            .map(|x| ASCII.decode(x, DecoderTrap::Replace).unwrap())
            .map(|x| x.parse::<f64>().unwrap())
            .for_each(|x| values.push(format!("(0, {:?})", x)));

        println!("{:?}\r", values.len());
        // Insert to sqlite db if values length > 5000.
        if values.len() > 5000 {
            println!("{:?}", &values);
            let query = query_head.clone() + &values.join(", ");
            let _ = &conn.execute(sqlx::query(&query)).await?;
            values = vec![];
        }

    }

    let query = query_head + &values.join(", ");
    let _ = &conn.execute(sqlx::query(&query)).await?;

    Ok(())

}
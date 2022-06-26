use sqlx::{migrate::MigrateDatabase, SqliteConnection, Connection, Sqlite, Executor};
use crate::{model, error};
use error::SQLMDError;
use tokio::sync::mpsc;
use encoding::{Encoding, DecoderTrap};
use encoding::all::ASCII;

// Create DB file
pub async fn create_db(dbpath: &str) 
-> Result<(), SQLMDError> {
  match Sqlite::create_database(&dbpath).await {
    Ok(_) => Ok(()),
    Err(e) => return Err(SQLMDError::SQLError(e)),
  }  
}

// Connect to DB and return connection
pub async fn connect_db(dbpath: &str)
-> Result<SqliteConnection, SQLMDError> {
  match SqliteConnection::connect(dbpath).await {
    Ok(c) => Ok(c),
    Err(e) => Err(SQLMDError::SQLError(e))
  }
}

// Prepare DB tables
pub async fn prepare_tables(mut conn: SqliteConnection) 
-> Result<SqliteConnection, error::SQLMDError> {
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
      return Err(SQLMDError::SQLError(e))
    };
  }
  Ok(conn)
}


pub async fn save_db(mut rx: mpsc::Receiver<Vec<u8>>, mut conn: sqlx::SqliteConnection) 
-> Result<(), error::SQLMDError> {

    // Insert atom parameters into the table
    let mut values = vec![];
    let query_head = "INSERT INTO freq VALUES ".to_string();

    while let Some(buff) = rx.recv().await {

        // println!("{:?}", &ASCII.decode(&buff, DecoderTrap::Replace).unwrap());
        
        // Check and remove LF at the end of the buff
        let freqs_u8 :Vec<u8>;
        if buff.last() != Some(&10u8) {
            println!("Broken stream");
            continue
        } else {
            // println!("THere");
            freqs_u8 = buff[..buff.len()-1].to_vec();
            // println!("{:?}", freqs_u8);
        }
        
        // Separate by comma, decode to ASCII, parse to f64, and append to vec. 
        freqs_u8.split(|b| *b == 44u8)
            .map(|x| ASCII.decode(x, DecoderTrap::Replace).unwrap())
            .map(|x| x.parse::<f64>().unwrap())
            .for_each(|x| values.push(format!("(0, {:?})", x)));
            //.for_each(|x| println!("{:?}", x));
        // println!("{:?}", values.len());
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
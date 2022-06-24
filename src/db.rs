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
    let mut counter = 0;

    while let Some(bytearray) = rx.recv().await {

        let text = ASCII.decode(&bytearray[..], DecoderTrap::Replace).unwrap();
        // println!("text: {}", text);
        if text == "\n" {
          // println!("{}", "continue\r");
          continue;
        }
        // println!("{:?}", text);
        let nums: Vec<f64> = text.split(',').map(|x| x.trim().parse::<f64>().unwrap()).collect();
        println!("{:?}", nums);

        // tx.send(nums.clone()).await.unwrap();
        
        if counter < 5000 {
            for num in &nums {
                let value = format!("(0, {:?})" , num);
                values.push(value);
            }
            counter += nums.len();
        } else {
            let query = query_head.clone() + &values.join(", ");
            let _ = &conn.execute(sqlx::query(&query)).await?;
            values = vec![];
            counter = 0; 
        }
    }

    let query = query_head + &values.join(", ");
    let _ = &conn.execute(sqlx::query(&query)).await?;

    Ok(())

}
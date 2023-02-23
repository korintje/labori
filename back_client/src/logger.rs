use std::path::Path;
use sqlx::{migrate::MigrateDatabase, SqliteConnection, Connection, Sqlite, Executor};
use crate::{model, error};
use error::LaboriError;
use tokio::sync::mpsc;
use encoding::{Encoding, DecoderTrap};
use encoding::all::ASCII;

const REGISTRY_NAME: &str = "registry";

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
pub async fn prepare_tables(mut conn: SqliteConnection, table_name: &str) 
-> Result<SqliteConnection, error::LaboriError> {
  let table_count: model::TableCount = sqlx::query_as(
    "SELECT COUNT(*) as count FROM sqlite_master WHERE TYPE='table' AND name=$1"
  )
  .bind(table_name)
  .fetch_one(&mut conn)
  .await?;
  if table_count.count == 0 {
    if let Err(e) = conn.execute(
      sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS '{}' (
          time    REAL NOT NULL,
          freq    REAL NOT NULL,
          rate    INTEGER NOT NULL
        )", table_name
      ))
    ).await {
      return Err(LaboriError::SQLError(e))
    };
  }
  Ok(conn)
}

// Prepare DB tables and the registry
pub async fn prepare_tables_reg(mut conn: SqliteConnection, table_name: &str) 
-> Result<SqliteConnection, error::LaboriError> {
  
  // Create table registry if not exists
  let reg_count: model::TableCount = sqlx::query_as(
    "SELECT COUNT(*) as count FROM sqlite_master WHERE TYPE='table' AND name=$1"
  )
  .bind(REGISTRY_NAME)
  .fetch_one(&mut conn)
  .await?;
  if reg_count.count == 0 {
    if let Err(e) = conn.execute(
      sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS '{}' (
          table_name          TEXT NOT NULL,
          channel_count       INTEGER NOT NULL,
          switch_delay        REAL NOT NULL,
          channel_interval    REAL NOT NULL,
          interval      REAL NOT NULL
        )", REGISTRY_NAME
      ))
    ).await {
      return Err(LaboriError::SQLError(e))
    };
  }

  // Create data table if not exists
  let table_count: model::TableCount = sqlx::query_as(
    "SELECT COUNT(*) as count FROM sqlite_master WHERE TYPE='table' AND name=$1"
  )
  .bind(table_name)
  .fetch_one(&mut conn)
  .await?;
  if table_count.count == 0 {
    if let Err(e) = conn.execute(
      sqlx::query(&format!(
        "CREATE TABLE IF NOT EXISTS '{}' (
          time    REAL NOT NULL,
          channel INTEGER NOT NULL,
          freq    REAL NOT NULL,
          rate    INTEGER NOT NULL
        )", table_name
      ))
    ).await {
      return Err(LaboriError::SQLError(e))
    };
  }

  Ok(conn)
  
}

pub async fn log(
    device_name: String,
    table_name: String,
    interval: f64,
    mut rx: mpsc::Receiver<Vec<u8>>
) -> Result<(), error::LaboriError> {

    // println!("{:?}", interval);
    
    // Determine batch size for SQL insertion
    let batch_size;
    if interval <= 0.001 {
        batch_size = 100;        
    } else if interval <= 0.01 {
        batch_size = 10;
    } else {
        batch_size = 1;
    }

    let dbpath = format!("{}.db", device_name);
    if ! Path::new(&dbpath).exists() {
        create_db(&dbpath).await?;
    }
    let conn = connect_db(&dbpath).await?;
    let mut conn = prepare_tables(conn, &table_name).await?;

    // Insert atom parameters into the table
    let mut values = vec![];
    let query_head = format!("INSERT INTO '{}' VALUES ", &table_name);

    println!("Start logging");
    let mut current_time = 0.0;

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
            .for_each(|x| {
              current_time += interval;
              values.push(format!("({}, {}, {})", current_time, x, &freqs_u8.len()));
            });

        // println!("{:?}\r", values.len());
        // Insert to sqlite db if values length > 5000.
        if values.len() >= batch_size {
            // println!("{:?}", &values);
            let query = query_head.clone() + &values.join(", ");
            let _ = &conn.execute(sqlx::query(&query)).await?;
            values = vec![];
        }

    }

    if values.len() != 0 {
        let query = query_head + &values.join(", ");
        let _ = &conn.execute(sqlx::query(&query)).await?;
    }

    Ok(())

}


pub async fn log_ext(
  device_name: String,
  table_name: String,
  interval: f64,
  mut rx: mpsc::Receiver<Vec<u8>>
) -> Result<(), error::LaboriError> {

  // println!("{:?}", interval);
  
  // Determine batch size for SQL insertion
  let batch_size;
  if interval <= 0.001 {
      batch_size = 100;        
  } else if interval <= 0.01 {
      batch_size = 10;
  } else if interval <= 0.1 {
      batch_size = 1;
  } else {
      batch_size = 1;
  }

  let dbpath = format!("{}.db", device_name);
  if ! Path::new(&dbpath).exists() {
      create_db(&dbpath).await?;
  }
  let conn = connect_db(&dbpath).await?;
  let mut conn = prepare_tables(conn, &table_name).await?;

  // Insert atom parameters into the table
  let mut values = vec![];
  let query_head = format!("INSERT INTO '{}' VALUES ", &table_name);

  println!("Start logging");
  // let mut current_time = 0.0;

  while let Some(buff) = rx.recv().await {

    // println!("RECEIVED");

      // Check and remove LF at the end of the buff
      let freq_u8: Vec<u8>;
      let meas_time: f64; 
      if buff.last() != Some(&10u8) {
        if buff[0] == 4u8 {
          println!("Stop logging");
          break
        }else{
          println!("Broken stream");
          continue
        }
      } else {
        // println!("Correct stream");
        meas_time = u64::from_ne_bytes(buff[0..8].try_into().unwrap()) as f64 / 1000.0;
        freq_u8 = buff[8..buff.len()-1].to_vec();
      }

      // Decode to ASCII, parse to f64, and append to vec.
      let freq_ascii = ASCII.decode(&freq_u8, DecoderTrap::Replace).unwrap();
      let freq_f64 = freq_ascii.parse::<f64>().unwrap();
      values.push(format!("({}, {}, {})", meas_time, freq_f64, &freq_u8.len()));

      /*
      freqs_u8.split(|b| *b == 44u8)
          .map(|x| ASCII.decode(x, DecoderTrap::Replace).unwrap())
          .map(|x| x.parse::<f64>().unwrap())
          .for_each(|x| {
            // current_time += interval;
            values.push(format!("({}, {}, {})", meas_time, x, &freqs_u8.len()));
          });
      */

      // println!("{:?}\r", values.len());
      // Insert to sqlite db if values length > 5000.
      // println!("VALUES LENGTH: {}", values.len());
      if values.len() >= batch_size {
          // println!("{:?}", &values);
          // println!("Batch!: {:?}", &values);
          let query = query_head.clone() + &values.join(", ");
          let _ = &conn.execute(sqlx::query(&query)).await?;
          values = vec![];
      }

  }

  if values.len() != 0 {
      let query = query_head + &values.join(", ");
      let _ = &conn.execute(sqlx::query(&query)).await?;
  }

  Ok(())

}


pub async fn log_multi(
  device_name: String,
  table_name: String,
  channel_count: u8,
  switch_delay: f64,
  channel_interval: f64,
  interval: f64,
  mut rx: mpsc::Receiver<Vec<u8>>
) -> Result<(), error::LaboriError> {
  
  // Determine batch size for SQL insertion
  let batch_size;
  if interval <= 0.001 {
      batch_size = 100;        
  } else if interval <= 0.01 {
      batch_size = 10;
  } else if interval <= 0.1 {
      batch_size = 1;
  } else {
      batch_size = 1;
  }

  // Connect to DB and prepare tables
  let dbpath = format!("{}.db", device_name);
  if ! Path::new(&dbpath).exists() {
      create_db(&dbpath).await?;
  }
  let conn = connect_db(&dbpath).await?;
  let mut conn = prepare_tables_reg(conn, &table_name).await?;

  // Registry table name
  let reg_query = format!(
    "INSERT INTO '{}' VALUES ({}, {}, {}, {}, {})", 
    REGISTRY_NAME, table_name, channel_count, switch_delay, channel_interval, interval
  );
  let _ = &conn.execute(sqlx::query(&reg_query)).await?;

  // Insert atom parameters into the table
  let query_head = format!("INSERT INTO '{}' VALUES ", &table_name);
  let mut values = vec![];
  
  println!("Start logging");

  while let Some(buff) = rx.recv().await {

      // Check and remove LF at the end of the buff
      let freq_u8s: Vec<u8>;
      let meas_time: f64;
      let channel_id: u8; 
      if buff.last() != Some(&10u8) {
        if buff[0] == 4u8 {
          println!("Stop logging");
          break
        }else{
          println!("Broken stream");
          continue
        }
      } else {
        meas_time = u64::from_ne_bytes(buff[0..8].try_into().unwrap()) as f64 / 1000.0;
        channel_id = u8::from_ne_bytes(buff[8..9].try_into().unwrap());
        freq_u8s = buff[9..buff.len()-1].to_vec();
      }

      // Decode to ASCII, parse to f64, and append to vec.
      let freq_ascii = ASCII.decode(&freq_u8s, DecoderTrap::Replace).unwrap();
      let freq_f64 = freq_ascii.parse::<f64>().unwrap();
      values.push(format!("({}, {}, {}, {})", meas_time, channel_id, freq_f64, &freq_u8s.len()));

      // Insert to sqlite db
      if values.len() >= batch_size {
          let query = query_head.clone() + &values.join(", ");
          let _ = &conn.execute(sqlx::query(&query)).await?;
          values = vec![];
      }

  }

  if values.len() != 0 {
      let query = query_head + &values.join(", ");
      let _ = &conn.execute(sqlx::query(&query)).await?;
  }

  Ok(())

}
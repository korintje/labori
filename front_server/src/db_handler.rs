// use std::process::Command;
use std::sync::Arc;
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;

use crate::model::FreqData;

pub struct DataAccessor {
  pub pool_refs: HashMap<u16, Arc<sqlx::Pool<sqlx::Sqlite>>>,
}

impl DataAccessor { 
  
  pub async fn new(dev_names: &HashMap<u16, String>) -> DataAccessor {
    let mut pool_refs = HashMap::new();
    for( port, dev_name ) in dev_names.iter() {
      let dev_path = format!("~/env0/labori/{}.db", &dev_name);
      let pool = SqlitePoolOptions::new().max_connections(5).connect(&dev_path).await.unwrap();
      pool_refs.insert(*port, Arc::new(pool));
    }
    DataAccessor {pool_refs}
  }

  pub async fn get_freqdata(&self, port: u16, table_name: &str, from: u64) -> Result<Vec<FreqData>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM $1 WHERE rowid>$2")
    .bind(&table_name.to_string())
    .bind(&from.to_string())
    .fetch_all(&*self.pool_refs[&port])
    .await
  }

}
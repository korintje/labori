use sqlx::{FromRow};
use serde::{Deserialize, Serialize};

#[derive(FromRow, Serialize)]
pub struct FreqData {
  pub time: f64,
  pub freq: f64,
  pub rate: i32,
}

#[derive(Deserialize)]
pub struct Filter {
    pub from: u64,
    pub dev_id: u16,
    pub table_name: String,
}

#[derive(Deserialize)]
pub struct FreqFilter {
    pub from: u64,
}
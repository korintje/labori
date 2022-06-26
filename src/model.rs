use serde::{Serialize};
use sqlx::FromRow;

#[derive(Debug)]
pub struct Atom{
  pub step: i64,
  pub atom_id: i64,
  pub element:String,
  pub charge: f64,
  pub x: f64,
  pub y: f64,
  pub z: f64,
  pub vx: f64,
  pub vy: f64,
  pub vz: f64,
}

#[derive(FromRow, Serialize)]
pub struct TableCount {
    pub count: i32,
}

#[derive(FromRow, Serialize)]
pub struct Metadata {
    pub xyzhash: u32,
}
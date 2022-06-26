use serde::{Serialize};
use sqlx::FromRow;

pub enum Command {
  Start,
  Stop,
  Get(State),
  Set,
}

pub struct State {
  device_name: String,
  device_addr: String,
  api_port: u32,
  method: String,
  interval: f32,
  sampling_interval: u32,
  connection: bool,
}

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

pub enum Func {
  FINA,
  FINB,
  FINC,
  FLIN,
  PER,
  DUTY,
  PWID,
  TINT,
  FRAT,
  TOT, 
  VPPA,
  VPPB,
  NONE,
}

impl From<&str> for Func {
    fn from(item: &str) -> Self {
        match item {
            "FINA" => Func::FINA,
            "FINB" => Func::FINB,
            "FINC" => Func::FINC,
            "FLIN" => Func::FLIN,
            "PER" => Func::PER,
            "DUTY" => Func::DUTY,
            "PWID" => Func::PWID,
            "TINT" => Func::TINT,
            "FRAT" => Func::FRAT,
            "TOT" => Func::TOT,
            "VPPA" => Func::VPPA,
            "VPPB" => Func::VPPB,
            _ => Func::NONE, 
        }
    }
}

impl From<Func> for &str {
    fn from(item: Func) -> Self {
        match item {
            Func::FINA => "FINA",
            Func::FINB => "FINB",
            Func::FINC => "FINC",
            Func::FLIN => "FLIN", 
            Func::PER => "PER", 
            Func::DUTY => "DUTY",
            Func::PWID => "PWID",
            Func::TINT => "TINT", 
            Func::FRAT => "FRAT", 
            Func::TOT => "TOT",
            Func::VPPA => "VPPA",
            Func::VPPB => "VPPB", 
            Func::NONE => "", 
        }
    }
}
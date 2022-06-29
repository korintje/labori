use serde::{Serialize, Deserialize};
use sqlx::FromRow;
use crate::error::LaboriError;

#[derive(FromRow, Serialize)]
pub struct TableCount {
    pub count: i32,
}

#[derive(FromRow, Serialize)]
pub struct Metadata {
    pub xyzhash: u32,
}

#[derive(Debug)]
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

#[derive(Debug, Serialize, Deserialize)]
pub enum Signal {
    Start,
    Stop,
}

const FUNC_VALUES: [&str; 12] = [
    "FINA", "FINB", "FINC", "FLIN", "PER", "DUTY",
    "PWID", "TINT", "FRAT", "TOT", "VPPA", "VPPB",
];

const INTERVAL_VALUES: [&str; 7] = [
    "0.00001", "0.0001", "0.001",
    "0.01", "0.1", "1", "10",
];


#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Get { key: String },
    Set { key: String, value: String },
    Trigger { value: String },
}

impl Command {

    pub fn into_cmd(&self) -> Result<String, LaboriError> {

        let mut cmd = "".to_string();      
        match &*self {
            Command::Get{ key: x } => {
                match x.as_ref() {
                    "Func" => cmd += ":FUNC?",
                    "Interval" => cmd += ":GATE:TIME?",
                    _ => return Err(LaboriError::CommandParseError(x.to_string())),
                }
            },
            Command::Set{ key: x, value: y} => {
                match x.as_ref() {
                    "Func" => {
                        cmd += ":FUNC ";
                        if FUNC_VALUES.contains(&y.as_ref()) {
                            cmd += &y;
                        } else {
                            return Err(LaboriError::CommandParseError(y.to_string()))
                        }
                    },
                    "Interval" => {
                        cmd += ":GATE:TIME ";
                        if INTERVAL_VALUES.contains(&y.as_ref()) {
                            cmd += &y;
                        } else {
                            return Err(LaboriError::CommandParseError(y.to_string()))
                        }
                    }
                    _ => return Err(LaboriError::CommandParseError(y.to_string()))
                }
            },
            Command::Trigger{ value: x } => {
                match x.as_ref() {
                    "Start" => cmd += ":LOG:LEN 5e5; :LOG:CLE; :FRUN ON",
                    "Stop" => cmd += ":FRUN OFF",
                    _ => return Err(LaboriError::CommandParseError(x.to_string()))
                }
            },
        }
        Ok(cmd + "\n")
    }

}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Busy,
    Finished,
}
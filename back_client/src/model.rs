use serde::{Serialize, Deserialize};
use sqlx::FromRow;
use crate::error::LaboriError;

#[derive(FromRow, Serialize)]
pub struct TableCount {
    pub count: i32,
}

const FUNC_VALUES: [&str; 12] = [
    "FINA", "FINB", "FINC", "FLIN", "PER", "DUTY",
    "PWID", "TINT", "FRAT", "TOT", "VPPA", "VPPB",
];

const INTERVAL_VALUES: [&str; 14] = [
    "0.00001", "0.0001", "0.001", "0.01", "0.1", "1", "10",
    "10E-6", "0.10E-3", "1.0E-3", "10E-3", "0.10E+0", "1.0E+0", "10.0E+0",
];


#[derive(Debug, Serialize, Deserialize)]
pub enum Command {
    Get { key: String },
    Set { key: String, value: String },
    Run {},
    Stop {},
}

impl Command {

    pub fn into_cmd(&self) -> Result<String, LaboriError> {

        let mut cmd = "".to_string();      
        match &*self {
            Command::Get{ key: x } => {
                match x.as_ref() {
                    "Func" => cmd += ":FUNC?",
                    "Interval" => cmd += ":GATE:TIME?",
                    _ => return Err(LaboriError::CommandParseError(
                        format!("Unregistered key: {}", x.to_string())
                    )),
                }
            },
            Command::Set{ key: x, value: y} => {
                match x.as_ref() {
                    "Func" => {
                        cmd += ":FUNC ";
                        if FUNC_VALUES.contains(&y.as_ref()) {
                            cmd += &y;
                        } else {
                            return Err(LaboriError::CommandParseError(
                                format!("Unregistered value: {}", y.to_string())
                            ))
                        }
                    },
                    "Interval" => {
                        cmd += ":GATE:TIME ";
                        if INTERVAL_VALUES.contains(&y.as_ref()) {
                            cmd += &y;
                        } else {
                            return Err(LaboriError::CommandParseError(
                                format!("Unregistered value: {}", y.to_string())
                            ))
                        }
                    }
                    _ => return Err(LaboriError::CommandParseError(
                        format!("Unregistered key: {}", x.to_string())
                    ))
                }
            },
            Command::Run{} => cmd += ":LOG:LEN 5e5; :LOG:CLE; :FRUN ON",
            Command::Stop{} => cmd += ":FRUN OFF",
        }
        Ok(cmd + "\n")
    }

}

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Success(Success),
    Failure(Failure),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Success {
    Finished(String),
    SaveTable(String),
    GotValue(String),
    SetValue(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Failure {
    Busy(String),
    NotRunning(String),
    ErrorInRunning(String),
    InvalidRequest(String),
    InvalidReturn(String),
    InvalidCommand(String),
    CommandNotSent(String),
    PollerCommandNotSent(String),
    SaveDataFailed(String),
    MachineNotRespond(String),
    SignalFailed(String),
}


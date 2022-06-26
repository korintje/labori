// use thiserror::Error;
// use futures_io;

/*
#[derive(thiserror::Error, Debug)]
pub enum SQLMDError {

    #[error("SQLite connection failed")]
    SQLError(#[from] sqlx::Error),

    #[error("tokio join failed")]
    JointError(#[from] tokio::task::JoinError),

    #[error("io error")]
    IOError(#[from] std::io::Error),

    #[error("Parse int error")]
    ParseIntError(#[from] std::num::ParseIntError),

    // #[error("Not found")]
    // NotFoundError(String),

}
*/

#[derive(thiserror::Error, Debug)]
pub enum LaboriError {

    #[error("Failed to connect Frequency conter")]
    TCPConnectionError(#[from] std::io::Error),

    #[error("Failed to send message to Frequency conter")]
    TCPSendError(String),

    #[error("Failed to receive message from Frequency conter")]
    TCPReceiveError(String),

    #[error("Request rejected because system is in measuring")]
    InMeasuringError(String),

    #[error("Parse int error")]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error("SQLite connection failed")]
    SQLError(#[from] sqlx::Error),

    #[error("tokio join failed")]
    JointError(#[from] tokio::task::JoinError),

}
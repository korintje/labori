#[derive(thiserror::Error, Debug)]
pub enum LaboriError {

    #[error("Failed to connect Frequency conter")]
    TCPConnectionError(#[from] std::io::Error),

    #[error("Failed to send message to Frequency conter")]
    TCPSendError(String),

    #[error("Failed to receive message from Frequency conter")]
    TCPReceiveError(String),

    #[error("Parse int error")]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error("SQLite connection failed")]
    SQLError(#[from] sqlx::Error),

    #[error("Error in logging")]
    LogError(String),

    #[error("tokio join failed")]
    JointError(#[from] tokio::task::JoinError),

    #[error("tokio command parse failed")]
    CommandParseError(String),
    
    #[error("Failed to send API message")]
    APISendError(String),

}
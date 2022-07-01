#[derive(thiserror::Error, Debug)]
pub enum LaboriError {

    #[error("Failed to connect Frequency conter")]
    TCPConnectionError(#[from] std::io::Error),

    #[error("Parse int error")]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error("SQLite connection failed")]
    SQLError(#[from] sqlx::Error),

    #[error("tokio join failed")]
    JointError(#[from] tokio::task::JoinError),

    #[error("tokio command parse failed")]
    CommandParseError(String),
    
    #[error("Failed to send API message")]
    APISendError(String),

}
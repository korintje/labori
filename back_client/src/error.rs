#[derive(thiserror::Error, Debug)]
pub enum LaboriError {

    #[error("Failed to connect Frequency conter")]
    TCPConnectionError(#[from] std::io::Error),

    #[error("SQLite connection failed")]
    SQLError(#[from] sqlx::Error),

    #[error("tokio command parse failed")]
    CommandParseError(String),
    
    #[error("Failed to send API message")]
    APISendError(String),
    
    // #[error("Invalid return from TCP server")]
    // InvalidReturn(String),

}
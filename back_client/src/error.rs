use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum LaboriError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("instrument communication error: {0}")]
    Instrument(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error("measurement is already running")]
    Busy,
    #[error("measurement is not running")]
    NotRunning,
    #[error("storage queue is full; measurement stopped before data could be lost")]
    StorageOverrun,
    #[error("internal channel closed: {0}")]
    ChannelClosed(&'static str),
}

impl IntoResponse for LaboriError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Invalid(_) => StatusCode::BAD_REQUEST,
            Self::Busy => StatusCode::CONFLICT,
            Self::NotRunning => StatusCode::CONFLICT,
            Self::StorageOverrun => StatusCode::INSUFFICIENT_STORAGE,
            Self::Config(_)
            | Self::Instrument(_)
            | Self::Database(_)
            | Self::Io(_)
            | Self::ChannelClosed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

pub type Result<T> = std::result::Result<T, LaboriError>;

// use thiserror::Error;

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

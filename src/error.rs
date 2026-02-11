use std::fmt;
use hyper::{StatusCode, body::Incoming};
use http_body_util::BodyExt;

#[derive(Debug)]
pub enum Error {
    Storage(String),
    Transaction(String),
    Auth(String),
    NotFound(String),
    Conflict(String),
    InvalidRequest(String),
    Compression(String),
    Hash(String),
    Internal(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Storage(msg) => write!(f, "Storage error: {}", msg),
            Error::Transaction(msg) => write!(f, "Transaction error: {}", msg),
            Error::Auth(msg) => write!(f, "Authentication error: {}", msg),
            Error::NotFound(msg) => write!(f, "Not found: {}", msg),
            Error::Conflict(msg) => write!(f, "Conflict: {}", msg),
            Error::InvalidRequest(msg) => write!(f, "Invalid request: {}", msg),
            Error::Compression(msg) => write!(f, "Compression error: {}", msg),
            Error::Hash(msg) => write!(f, "Hash error: {}", msg),
            Error::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    #[inline]
    pub fn status_code(&self) -> StatusCode {
        match self {
            Error::Storage(_) | Error::Transaction(_) | Error::Internal(_) |
            Error::Compression(_) | Error::Hash(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Auth(_) => StatusCode::UNAUTHORIZED,
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Conflict(_) => StatusCode::CONFLICT,
            Error::InvalidRequest(_) => StatusCode::BAD_REQUEST,
        }
    }
}

impl From<sled::Error> for Error {
    fn from(err: sled::Error) -> Self {
        Error::Storage(err.to_string())
    }
}

impl From<bincode::Error> for Error {
    fn from(err: bincode::Error) -> Self {
        Error::Storage(err.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Storage(err.to_string())
    }
}

pub async fn read_body_to_bytes(body: Incoming) -> Result<bytes::Bytes, Error> {
    body.collect()
        .await
        .map_err(|e| Error::InvalidRequest(format!("Failed to read body: {}", e)))
        .map(|c| c.to_bytes())
}

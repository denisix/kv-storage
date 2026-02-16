//! Error types for the KV Storage client

use std::io;
use thiserror::Error;

/// Errors that can occur when interacting with the KV Storage server
#[derive(Error, Debug)]
pub enum Error {
    /// Authentication failed (invalid token)
    #[error("Unauthorized: Invalid token")]
    Unauthorized,

    /// Key was not found
    #[error("Key not found: {0}")]
    NotFound(String),

    /// Invalid request (malformed data, validation error, etc.)
    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    /// Server returned an error
    #[error("Server error (status {status}): {message}")]
    ServerError {
        /// HTTP status code
        status: u16,
        /// Error message from the server
        message: String,
    },

    /// Network or connection error
    #[error("Connection error: {0}")]
    Connection(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// HTTP/2 protocol error
    #[error("HTTP/2 error: {0}")]
    Http2(String),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Request timeout
    #[error("Request timeout after {0}ms")]
    Timeout(u64),

    /// TLS/SSL error
    #[error("TLS error: {0}")]
    Tls(String),

    /// URL parsing error
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    /// Other internal errors
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, Error>;

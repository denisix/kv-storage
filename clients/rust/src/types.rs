//! Data types for the KV Storage client

use serde::{Deserialize, Serialize};
use http::HeaderMap;

/// Response from a PUT request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutResponse {
    /// Hash of the stored content
    pub hash: String,
    /// Hash algorithm used (e.g., "xxhash3-128")
    pub hash_algorithm: String,
    /// Whether the content was deduplicated (same hash already existed)
    pub deduplicated: bool,
}

/// Information about a single key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyInfo {
    /// The key name
    pub key: String,
    /// Size of the value in bytes
    pub size: u64,
    /// Hash of the content
    pub hash: String,
    /// Hash algorithm used
    pub hash_algorithm: String,
    /// Number of references to this content (for deduplication)
    pub refs: u64,
    /// Unix timestamp when the key was created
    pub created_at: u64,
}

/// Response from a list request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse {
    /// Array of key information
    pub keys: Vec<KeyInfo>,
    /// Total number of keys
    pub total: u64,
}

/// A single batch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum BatchOp {
    /// Store a value
    #[serde(rename = "put")]
    Put { key: String, value: String },
    /// Retrieve a value
    #[serde(rename = "get")]
    Get { key: String },
    /// Delete a key
    #[serde(rename = "delete")]
    Delete { key: String },
}

/// Result of a single batch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BatchResult {
    /// Successful PUT operation
    Put { key: String, hash: String, created: bool },
    /// Successful GET operation
    Get {
        key: String,
        value: Option<String>,
        found: bool,
    },
    /// Successful DELETE operation
    Delete { key: String, deleted: bool },
    /// Failed operation
    Error { key: String, error: String },
}

/// Response from a batch request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResponse {
    /// Array of results for each operation
    pub results: Vec<BatchResult>,
}

impl BatchResult {
    /// Returns the key associated with this result
    pub fn key(&self) -> &str {
        match self {
            BatchResult::Put { key, .. } => key,
            BatchResult::Get { key, .. } => key,
            BatchResult::Delete { key, .. } => key,
            BatchResult::Error { key, .. } => key,
        }
    }

    /// Returns true if this result indicates an error
    pub fn is_error(&self) -> bool {
        matches!(self, BatchResult::Error { .. })
    }

    /// Returns the error message if this result is an error
    pub fn error(&self) -> Option<&str> {
        match self {
            BatchResult::Error { error, .. } => Some(error),
            _ => None,
        }
    }
}

/// HEAD request response headers
#[derive(Debug, Clone)]
pub struct HeadInfo {
    /// Content length in bytes
    pub content_length: u64,
    /// Number of references to this content
    pub refs: u64,
    /// Hash of the content
    pub hash: String,
}

impl HeadInfo {
    /// Create HeadInfo from HTTP headers
    pub fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let content_length: u64 = headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v: &str| v.parse().ok())
            .unwrap_or(0);

        let refs: u64 = headers
            .get("x-refs")
            .and_then(|v| v.to_str().ok())
            .and_then(|v: &str| v.parse().ok())
            .unwrap_or(0);

        let hash = headers
            .get("x-hash")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        Some(HeadInfo {
            content_length,
            refs,
            hash,
        })
    }
}

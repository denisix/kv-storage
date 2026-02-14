//! Common utilities and helpers for HTTP handlers
//!
//! This module provides shared functionality to reduce code duplication
//! across different request handlers.

use hyper::{Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;

use crate::error::Error;
use crate::server::Handler;
use crate::storage::KeyMeta;

/// Maximum allowed key length (256KB) to prevent DoS
const MAX_KEY_LENGTH: usize = 256 * 1024;

/// Validates that a key is safe to use.
///
/// Keys must:
/// - Not be empty
/// - Not exceed MAX_KEY_LENGTH
/// - Not contain control characters (except tab)
///
/// # Errors
/// Returns `Error::InvalidRequest` if validation fails.
pub fn validate_key(key: &str) -> Result<(), Error> {
    // Check empty
    if key.is_empty() {
        return Err(Error::InvalidRequest("Key cannot be empty".to_string()));
    }

    // Check length to prevent DoS
    if key.len() > MAX_KEY_LENGTH {
        return Err(Error::InvalidRequest(format!(
            "Key too long (max {} bytes)", MAX_KEY_LENGTH
        )));
    }

    // Check for control characters (security concern)
    // We allow tab (0x09) but not other control characters
    for ch in key.chars() {
        if ch < ' ' && ch != '\t' {
            return Err(Error::InvalidRequest(
                "Key contains invalid control characters".to_string()
            ));
        }
    }

    Ok(())
}

/// Retrieves key metadata from the database.
///
/// # Arguments
/// * `handler` - The handler containing the database reference
/// * `key` - The key to look up
///
/// # Errors
/// Returns `Error::NotFound` if the key does not exist.
pub fn get_key_meta(handler: &Handler, key: &str) -> Result<KeyMeta, Error> {
    let keys_tree = handler.db().keys_tree();
    let meta_bytes = keys_tree
        .get(key.as_bytes())?
        .ok_or_else(|| Error::NotFound(format!("Key '{}' not found", key)))?;

    Ok(bincode::deserialize(&meta_bytes)?)
}

/// Builds a response with hash-related headers.
///
/// # Arguments
/// * `status` - The HTTP status code
/// * `meta` - The key metadata containing hash information
/// * `include_extra_headers` - Whether to include additional headers like X-Refs and X-Created-At
///
/// # Errors
/// Returns `Error::Internal` if response building fails.
#[inline]
pub fn build_hash_response(
    status: StatusCode,
    meta: &KeyMeta,
    include_extra_headers: bool,
) -> Result<Response<Full<Bytes>>, Error> {
    let mut builder = Response::builder()
        .status(status)
        .header("Content-Type", "application/octet-stream")
        .header("X-Hash", meta.hash.to_hex_string())
        .header("X-Hash-Algorithm", "xxhash3");

    if include_extra_headers {
        builder = builder
            .header("X-Created-At", meta.created_at.to_string())
            .header("X-Refs", meta.refs.to_string());
    }

    builder.body(Full::new(Bytes::new()))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

/// Builds a response with hash headers and a body.
///
/// # Arguments
/// * `status` - The HTTP status code
/// * `meta` - The key metadata containing hash information
/// * `body` - The response body bytes
///
/// # Errors
/// Returns `Error::Internal` if response building fails.
#[inline]
pub fn build_hash_response_with_body(
    status: StatusCode,
    meta: &KeyMeta,
    body: Bytes,
) -> Result<Response<Full<Bytes>>, Error> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/octet-stream")
        .header("Content-Length", body.len().to_string())
        .header("X-Hash", meta.hash.to_hex_string())
        .header("X-Hash-Algorithm", "xxhash3")
        .body(Full::new(body))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_key_empty() {
        let result = validate_key("");
        assert!(matches!(result, Err(Error::InvalidRequest(_))));
    }

    #[test]
    fn test_validate_key_valid() {
        let result = validate_key("test-key");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_key_with_spaces() {
        assert!(validate_key("key with spaces").is_ok());
        assert!(validate_key("hello world").is_ok());
        assert!(validate_key(" leading space").is_ok());
        assert!(validate_key("trailing space ").is_ok());
    }

    #[test]
    fn test_validate_key_with_special_characters() {
        assert!(validate_key("key/with/slashes").is_ok());
        assert!(validate_key("key.with.dots").is_ok());
        assert!(validate_key("key:with:colons").is_ok());
        assert!(validate_key("key#with#hash").is_ok());
        assert!(validate_key("key?with?question").is_ok());
        assert!(validate_key("key%with%percent").is_ok());
        assert!(validate_key("key@with@at").is_ok());
        assert!(validate_key("key!exclaim").is_ok());
        assert!(validate_key("key~tilde").is_ok());
        assert!(validate_key("key(parens)").is_ok());
        assert!(validate_key("key[brackets]").is_ok());
        assert!(validate_key("key{braces}").is_ok());
    }

    #[test]
    fn test_validate_key_with_unicode() {
        assert!(validate_key("ключ").is_ok());
        assert!(validate_key("键").is_ok());
        assert!(validate_key("مفتاح").is_ok());
        assert!(validate_key("日本語キー").is_ok());
        assert!(validate_key("emoji🔑key").is_ok());
    }

    #[test]
    fn test_validate_key_rejects_control_characters() {
        assert!(validate_key("key\x00null").is_err());
        assert!(validate_key("key\x01soh").is_err());
        assert!(validate_key("key\nnewline").is_err());
        assert!(validate_key("key\rreturn").is_err());
    }

    #[test]
    fn test_validate_key_allows_tab() {
        assert!(validate_key("key\twith\ttabs").is_ok());
    }
}

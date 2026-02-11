use hyper::{Request, Response, StatusCode, body::Incoming};
use http_body_util::Full;
use hyper::body::Bytes;
use serde::{Deserialize, Serialize};

use crate::error::{Error, read_body_to_bytes};
use crate::server::Handler;
use crate::util::hash::Hash;

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
pub enum BatchOp {
    #[serde(rename = "put")]
    Put { key: String, value: String },
    #[serde(rename = "get")]
    Get { key: String },
    #[serde(rename = "delete")]
    Delete { key: String },
}

#[derive(Debug, Serialize)]
pub enum BatchResult {
    #[serde(rename = "put")]
    Put { key: String, hash: String, created: bool },
    #[serde(rename = "get")]
    Get { key: String, value: Option<String>, found: bool },
    #[serde(rename = "delete")]
    Delete { key: String, deleted: bool },
    #[serde(rename = "error")]
    Error { key: String, error: String },
}

// Helper to ensure consistent JSON serialization
impl BatchResult {
    fn to_json_result(&self) -> serde_json::Value {
        match self {
            BatchResult::Put { key, hash, created } => {
                serde_json::json!({
                    "put": {
                        "key": key,
                        "hash": hash,
                        "created": created
                    }
                })
            }
            BatchResult::Get { key, value, found } => {
                if let Some(v) = value {
                    serde_json::json!({
                        "get": {
                            "key": key,
                            "value": v,
                            "found": found
                        }
                    })
                } else {
                    serde_json::json!({
                        "get": {
                            "key": key,
                            "found": found
                        }
                    })
                }
            }
            BatchResult::Delete { key, deleted } => {
                serde_json::json!({
                    "delete": {
                        "key": key,
                        "deleted": deleted
                    }
                })
            }
            BatchResult::Error { key, error } => {
                serde_json::json!({
                    "error": {
                        "key": key,
                        "error": error
                    }
                })
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    results: Vec<BatchResult>,
}

pub async fn handle_batch(
    handler: &Handler,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Error> {
    // Read body
    let data = read_body_to_bytes(req.into_body()).await?;

    // Parse operations
    let ops: Vec<BatchOp> = serde_json::from_slice(&data)
        .map_err(|e| Error::InvalidRequest(format!("Invalid JSON: {}", e)))?;

    // Pre-allocate results vector with capacity to avoid reallocations
    let mut results = Vec::with_capacity(ops.len());

    for op in ops {
        let result = match op {
            BatchOp::Put { key, value } => {
                let value_bytes = value.into_bytes();

                // Compute hash using xxHash3-128
                let hash = Hash::compute(&value_bytes);
                let hash_str = hash.to_hex_string();

                // Compress
                let compressed = handler.compressor().compress(&value_bytes)?;
                let size = value_bytes.len() as u64;

                // Store
                match handler.db().keys_tree().contains_key(key.as_bytes()) {
                    Ok(true) => {
                        BatchResult::Error { key, error: "Key already exists".to_string() }
                    }
                    Ok(false) => {
                        let tx_manager = crate::storage::TransactionManager::new(handler.db().clone());
                        match tx_manager.put_key_atomic(&key, &compressed, &hash, size) {
                            Ok(is_new) => {
                                handler.metrics().inc_puts();
                                BatchResult::Put { key, hash: hash_str, created: is_new }
                            }
                            Err(e) => BatchResult::Error { key, error: e.to_string() }
                        }
                    }
                    Err(e) => BatchResult::Error { key, error: e.to_string() }
                }
            }
            BatchOp::Get { key } => {
                match handler.db().keys_tree().get(key.as_bytes()) {
                    Ok(Some(meta_bytes)) => {
                        let meta: crate::storage::KeyMeta = bincode::deserialize(&meta_bytes)?;
                        match handler.db().objects_tree().get(meta.hash.as_bytes()) {
                            Ok(Some(compressed)) => {
                                match handler.compressor().decompress(&compressed) {
                                    Ok(data) => {
                                        handler.metrics().inc_gets();
                                        let value = String::from_utf8_lossy(&data).to_string();
                                        BatchResult::Get { key, value: Some(value), found: true }
                                    }
                                    Err(e) => BatchResult::Error { key, error: e.to_string() }
                                }
                            }
                            Ok(None) => BatchResult::Error { key, error: "Object not found".to_string() },
                            Err(e) => BatchResult::Error { key, error: e.to_string() }
                        }
                    }
                    Ok(None) => BatchResult::Get { key, value: None, found: false },
                    Err(e) => BatchResult::Error { key, error: e.to_string() }
                }
            }
            BatchOp::Delete { key } => {
                let tx_manager = crate::storage::TransactionManager::new(handler.db().clone());
                match tx_manager.delete_key_atomic(&key) {
                    Ok(_) => {
                        handler.metrics().inc_deletes();
                        BatchResult::Delete { key, deleted: true }
                    }
                    Err(e) => BatchResult::Error { key, error: e.to_string() }
                }
            }
        };

        results.push(result);
    }

    let response = BatchResponse { results };
    // Use custom serialization for consistent JSON format
    let results_json: Vec<serde_json::Value> = response.results.iter()
        .map(|r| r.to_json_result())
        .collect();

    let json = serde_json::to_string_pretty(&serde_json::json!({ "results": results_json }))
        .map_err(|e| Error::Internal(format!("JSON serialization error: {}", e)))?;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

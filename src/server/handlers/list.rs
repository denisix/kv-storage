use hyper::{Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;
use serde::Serialize;

use crate::error::Error;
use crate::server::Handler;

#[derive(Serialize)]
struct KeyInfo {
    key: String,
    size: u64,
    hash: String,
    hash_algorithm: String,
    refs: u32,
    created_at: u64,
}

#[derive(Serialize)]
struct ListResponse {
    keys: Vec<KeyInfo>,
    total: usize,
}

const DEFAULT_LIMIT: usize = 100;
const MAX_LIMIT: usize = 1000;
const MAX_OFFSET: usize = 1_000_000;

pub fn handle_list(
    handler: &Handler,
    query: Option<&str>,
) -> Result<Response<Full<Bytes>>, Error> {
    // Parse query parameters with proper validation
    let mut offset = 0usize;
    let mut limit = DEFAULT_LIMIT;

    if let Some(q) = query {
        for (k, v) in url::form_urlencoded::parse(q.as_bytes()) {
            match k.as_ref() {
                "offset" => {
                    // Safely parse offset with bounds checking
                    if let Ok(val) = v.parse::<usize>() {
                        offset = val.min(MAX_OFFSET);
                    }
                    // Invalid values default to 0
                }
                "limit" => {
                    // Safely parse limit with bounds checking
                    if let Ok(val) = v.parse::<usize>() {
                        limit = val.clamp(1, MAX_LIMIT);
                    }
                    // Invalid values default to DEFAULT_LIMIT
                }
                _ => {}
            }
        }
    }

    // Get keys
    let key_pairs = handler.db().list_tree_paginated("keys", offset, limit)?;
    let total = handler.db().count_tree("keys")?;

    let keys: Vec<KeyInfo> = key_pairs
        .into_iter()
        .map(|(key, meta_bytes)| {
            let meta: crate::storage::KeyMeta = bincode::deserialize(&meta_bytes)
                .map_err(|e| Error::Internal(format!("Failed to deserialize metadata: {}", e)))?;
            let key_str = String::from_utf8(key)
                .map_err(|e| Error::Internal(format!("Invalid key UTF-8: {}", e)))?;
            Ok::<KeyInfo, Error>(KeyInfo {
                key: key_str,
                size: meta.size,
                hash: meta.hash.to_hex_string(),
                hash_algorithm: "xxhash3".to_string(),
                refs: meta.refs,
                created_at: meta.created_at,
            })
        })
        .collect::<Result<_, Error>>()?;

    let response = ListResponse { keys, total };
    let json = serde_json::to_string_pretty(&response)
        .map_err(|e| Error::Internal(format!("JSON serialization error: {}", e)))?;

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(json)))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

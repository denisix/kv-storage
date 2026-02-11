use hyper::{Request, Response, StatusCode, body::Incoming};
use http_body_util::Full;
use hyper::body::Bytes;

use crate::error::Error;
use crate::server::Handler;
use crate::util::hash::Hash;
use crate::error::read_body_to_bytes;
use crate::server::handlers::common::validate_key;

/// PUT handler with xxHash3-128 for performance and 128-bit collision resistance
/// Compression is done inline for small payloads, blocking task for large ones.
pub async fn handle_put(
    handler: &Handler,
    key: &str,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Error> {
    validate_key(key)?;

    // Read entire body
    let data = read_body_to_bytes(req.into_body()).await?;

    // Compute hash using xxHash3-128 (128-bit for better collision resistance)
    let hash = Hash::compute(&data);
    let size = data.len() as u64;
    let data_len = data.len();

    // Compress data - use blocking task only for larger payloads
    let compressed = if data_len > 64 * 1024 {
        // Large payload (>64KB) - use blocking task to avoid blocking async runtime
        tokio::task::spawn_blocking({
            let compressor = handler.compressor().clone();
            let data = data.to_vec();
            move || compressor.compress(&data)
        })
        .await
        .map_err(|e| Error::Internal(format!("Compression task failed: {}", e)))??
    } else {
        // Small payload - compress inline (faster due to no task spawn overhead)
        handler.compressor().compress(&data)?
    };

    // Get hash bytes for storage lookup
    let hash_bytes = hash.as_bytes();

    // Check for deduplication
    let objects_tree = handler.db().objects_tree();
    let object_exists = objects_tree.contains_key(hash_bytes)?;

    if object_exists {
        // Object already exists - just add key mapping
        let keys_tree = handler.db().keys_tree();
        let refs_tree = handler.db().refs_tree();

        // Check if key already exists
        if keys_tree.contains_key(key.as_bytes())? {
            return Err(Error::Conflict(format!("Key '{}' already exists", key)));
        }

        // Create key metadata pointing to existing object
        let meta = crate::storage::KeyMeta::new(hash.clone(), size);
        let meta_bytes = bincode::serialize(&meta)?;
        keys_tree.insert(key.as_bytes(), meta_bytes)?;

        // Add ref
        let mut ref_key = hash_bytes.to_vec();
        ref_key.extend_from_slice(key.as_bytes());
        refs_tree.insert(&ref_key, b"1")?;

        handler.metrics().inc_dedup_hits();
        handler.metrics().inc_puts();

        return build_dedup_response(StatusCode::OK, &hash, true);
    }

    // Store new object atomically
    let tx_manager = crate::storage::TransactionManager::new(handler.db().clone());
    tx_manager.put_key_atomic(key, &compressed, &hash, size)?;

    handler.metrics().inc_puts();

    build_dedup_response(StatusCode::CREATED, &hash, false)
}

/// Build a PUT response with hash headers
#[inline]
fn build_dedup_response(status: StatusCode, hash: &Hash, deduplicated: bool) -> Result<Response<Full<Bytes>>, Error> {
    Response::builder()
        .status(status)
        .header("X-Hash", hash.to_hex_string())
        .header("X-Hash-Algorithm", "xxhash3")
        .header("X-Deduplicated", if deduplicated { "true" } else { "false" })
        .body(Full::new(Bytes::from(format!("{}\n", hash.to_hex_string()))))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

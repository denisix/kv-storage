use hyper::{Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;

use crate::error::Error;
use crate::server::Handler;
use crate::server::handlers::common::{validate_key, get_key_meta, build_hash_response_with_body};

pub async fn handle_get(
    handler: &Handler,
    key: &str,
) -> Result<Response<Full<Bytes>>, Error> {
    validate_key(key)?;

    // Get key metadata
    let meta = get_key_meta(handler, key)?;

    // Get object data using hash bytes
    let hash_bytes = meta.hash.as_bytes();
    let objects_tree = handler.db().objects_tree();
    let compressed = objects_tree.get(hash_bytes)?
        .ok_or_else(|| Error::NotFound("Object data not found".to_string()))?;

    // Decompress - use blocking task only for larger payloads
    let data = if compressed.len() > 64 * 1024 {
        // Large payload - use blocking task to avoid blocking async runtime
        tokio::task::spawn_blocking({
            let compressor = handler.compressor().clone();
            let compressed = compressed.to_vec();
            move || compressor.decompress(&compressed)
        })
        .await
        .map_err(|e| Error::Internal(format!("Decompression task failed: {}", e)))??
    } else {
        // Small payload - decompress inline (faster due to no task spawn overhead)
        handler.compressor().decompress(&compressed)?
    };

    handler.metrics().inc_gets();

    build_hash_response_with_body(StatusCode::OK, &meta, Bytes::from(data))
}

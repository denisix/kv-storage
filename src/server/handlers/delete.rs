use hyper::{Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;

use crate::error::Error;
use crate::server::Handler;
use crate::server::handlers::common::{validate_key, get_key_meta};

pub async fn handle_delete(
    handler: &Handler,
    key: &str,
) -> Result<Response<Full<Bytes>>, Error> {
    validate_key(key)?;

    // Get metadata first for size tracking
    let meta = get_key_meta(handler, key)?;
    let size = meta.size;

    // Delete atomically
    let tx_manager = crate::storage::TransactionManager::new(handler.db().clone());
    tx_manager.delete_key_atomic(key)?;

    handler.metrics().inc_deletes();
    handler.metrics().sub_bytes(size);

    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Full::new(Bytes::new()))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

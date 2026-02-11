use hyper::{Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;

use crate::error::Error;
use crate::server::Handler;

pub fn handle_metrics(handler: &Handler) -> Result<Response<Full<Bytes>>, Error> {
    // Update current stats
    let keys_count = handler.db().count_tree("keys")?;
    let objects_count = handler.db().count_tree("objects")?;

    handler.metrics().set_keys(keys_count as u64);
    handler.metrics().set_objects(objects_count as u64);

    let metrics_text = handler.metrics().to_prometheus();

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/plain; version=0.0.4")
        .body(Full::new(Bytes::from(metrics_text)))
        .map_err(|e| Error::Internal(format!("Failed to build response: {}", e)))
}

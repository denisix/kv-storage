use hyper::{Response, StatusCode};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::header::HeaderValue;

use crate::error::Error;
use crate::server::Handler;
use crate::server::handlers::common::{validate_key, get_key_meta, build_hash_response};

pub async fn handle_head(
    handler: &Handler,
    key: &str,
) -> Result<Response<Full<Bytes>>, Error> {
    validate_key(key)?;

    // Get key metadata
    let meta = get_key_meta(handler, key)?;

    let mut response = build_hash_response(StatusCode::OK, &meta, true)?;
    // Override content-length to be the actual size, not 0
    let headers = response.headers_mut();
    headers.insert(
        "Content-Length",
        HeaderValue::from_str(&meta.size.to_string())
            .map_err(|e| Error::Internal(format!("Invalid header value: {}", e)))?
    );

    Ok(response)
}

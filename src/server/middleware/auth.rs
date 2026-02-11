use hyper::Request;
use crate::error::Error;

/// Check authentication using constant-time comparison to prevent timing attacks.
pub fn check_auth<B>(req: &Request<B>, expected_token: &str) -> Result<(), Error> {
    let auth_header = req
        .headers()
        .get("Authorization")
        .ok_or_else(|| Error::Auth("Missing Authorization header".to_string()))?;

    let auth_value = auth_header
        .to_str()
        .map_err(|_| Error::Auth("Invalid Authorization header".to_string()))?;

    if !auth_value.starts_with("Bearer ") {
        return Err(Error::Auth("Authorization header must use Bearer scheme".to_string()));
    }

    let token = &auth_value[7..]; // Skip "Bearer "

    // Use constant-time comparison to prevent timing attacks
    if !constant_time_eq(token, expected_token) {
        return Err(Error::Auth("Invalid token".to_string()));
    }

    Ok(())
}

/// Constant-time string comparison to prevent timing attacks.
/// Returns true if the strings are equal, false otherwise.
fn constant_time_eq(a: &str, b: &str) -> bool {
    // Use subtle crate for constant-time comparison
    subtle::ConstantTimeEq::ct_eq(a.as_bytes(), b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Method, Uri};
    use bytes::Bytes;

    fn make_request(auth_header: Option<&str>) -> Request<Bytes> {
        let mut req = Request::builder()
            .method(Method::GET)
            .uri(Uri::from_static("http://localhost/"))
            .body(Bytes::new())
            .unwrap();

        if let Some(header) = auth_header {
            req.headers_mut().insert("Authorization", header.parse().unwrap());
        }

        req
    }

    #[test]
    fn test_valid_auth() {
        let req = make_request(Some("Bearer secret-token"));
        assert!(check_auth(&req, "secret-token").is_ok());
    }

    #[test]
    fn test_missing_auth() {
        let req = make_request(None);
        assert!(check_auth(&req, "secret-token").is_err());
    }

    #[test]
    fn test_invalid_token() {
        let req = make_request(Some("Bearer wrong-token"));
        assert!(check_auth(&req, "secret-token").is_err());
    }

    #[test]
    fn test_invalid_scheme() {
        let req = make_request(Some("Basic secret-token"));
        assert!(check_auth(&req, "secret-token").is_err());
    }
}

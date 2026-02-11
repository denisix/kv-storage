use hyper::{Request, Response, body::Incoming, Version};
use http_body_util::Full;
use hyper::body::Bytes;
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use tracing::{info, debug};
use zeroize::Zeroize;

use crate::error::Error;
use crate::storage::StorageDb;
use crate::server::middleware::auth::check_auth;
use crate::util::{compression::Compressor, metrics::Metrics};
use crate::server::handlers;

/// Secure token wrapper that zeros memory on drop.
/// Uses String internally but implements Drop to zero the memory.
#[derive(Clone)]
struct AuthToken(String);

impl AuthToken {
    fn new(token: String) -> Self {
        Self(token)
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Drop for AuthToken {
    fn drop(&mut self) {
        // Zero out the token memory when dropped
        self.0.zeroize();
    }
}

#[derive(Clone)]
pub struct Handler {
    db: StorageDb,
    auth_token: AuthToken,
    compressor: Arc<Compressor>,
    metrics: Arc<Metrics>,
}

impl Handler {
    pub fn new(
        db: StorageDb,
        auth_token: String,
        compressor: Arc<Compressor>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            db,
            auth_token: AuthToken::new(auth_token),
            compressor,
            metrics,
        }
    }

    pub async fn handle(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Error> {
        // Log request with HTTP version
        let http_version = format_http_version(req.version());
        debug!("{} {} {}", req.method(), req.uri().path(), http_version);

        // Check authentication
        if let Err(e) = check_auth(&req, self.auth_token.as_str()) {
            info!("{} {} {} - Authentication failed (401)", req.method(), req.uri().path(), http_version);
            return Ok(Self::error_response(e));
        }

        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let query = req.uri().query().map(|q| q.to_string());

        // Route request
        let result = match (method.as_str(), path.as_str()) {
            ("PUT", _) if path.len() > 1 => {
                let key = &path[1..]; // Remove leading '/'
                self.handle_put(key, req).await
            }
            ("GET", "/metrics") => self.handle_metrics(),
            ("GET", "/keys") => self.handle_list_keys(query.as_deref()),
            ("GET", _) if path.len() > 1 => {
                let key = &path[1..];
                self.handle_get(key).await
            }
            ("HEAD", _) if path.len() > 1 => {
                let key = &path[1..];
                self.handle_head(key).await
            }
            ("DELETE", _) if path.len() > 1 => {
                let key = &path[1..];
                self.handle_delete(key).await
            }
            ("POST", "/batch") => self.handle_batch(req).await,
            _ => Err(Error::NotFound("Path not found".to_string())),
        };

        result.or_else(|e| Ok(Self::error_response(e)))
    }

    async fn handle_put(&self, key: &str, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Error> {
        handlers::put::handle_put(self, key, req).await
    }

    async fn handle_get(&self, key: &str) -> Result<Response<Full<Bytes>>, Error> {
        handlers::get::handle_get(self, key).await
    }

    async fn handle_head(&self, key: &str) -> Result<Response<Full<Bytes>>, Error> {
        handlers::head::handle_head(self, key).await
    }

    async fn handle_delete(&self, key: &str) -> Result<Response<Full<Bytes>>, Error> {
        handlers::delete::handle_delete(self, key).await
    }

    fn handle_list_keys(&self, query: Option<&str>) -> Result<Response<Full<Bytes>>, Error> {
        handlers::list::handle_list(self, query)
    }

    async fn handle_batch(&self, req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Error> {
        handlers::batch::handle_batch(self, req).await
    }

    fn handle_metrics(&self) -> Result<Response<Full<Bytes>>, Error> {
        handlers::metrics::handle_metrics(self)
    }

    fn error_response(error: Error) -> Response<Full<Bytes>> {
        let status = error.status_code();
        let body = Bytes::from(format!("Error: {}\n", error));
        Response::builder()
            .status(status)
            .header("Content-Type", "text/plain")
            .body(Full::new(body))
            .unwrap()
    }

    // Accessor methods for handlers
    #[inline]
    pub fn db(&self) -> &StorageDb {
        &self.db
    }

    #[inline]
    pub fn compressor(&self) -> &Arc<Compressor> {
        &self.compressor
    }

    #[inline]
    pub fn metrics(&self) -> &Arc<Metrics> {
        &self.metrics
    }
}

/// Format HTTP version for logging
fn format_http_version(version: Version) -> &'static str {
    match version {
        Version::HTTP_09 => "HTTP/0.9",
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_11 => "HTTP/1.1",
        Version::HTTP_2 => "HTTP/2",
        Version::HTTP_3 => "HTTP/3",
        _ => "HTTP/unknown",
    }
}

// Implement Hyper's Service trait for HTTP/2
impl hyper::service::Service<Request<Incoming>> for Handler {
    type Response = Response<Full<Bytes>>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let handler = self.clone();
        Box::pin(async move { handler.handle(req).await })
    }
}

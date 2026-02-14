//! HTTP/2 client implementation for KV Storage

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, StatusCode, Uri};
use hyper::body::{Bytes, Incoming};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client as HttpClient;
use hyper_util::rt::TokioExecutor;
use tracing::debug;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::error::{Error, Result};
use crate::types::*;

/// Characters allowed unencoded in URI path segments per RFC 3986.
/// Everything else (including spaces, `#`, `?`, `%`, non-ASCII) gets percent-encoded.
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'!')
    .remove(b'$')
    .remove(b'&')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b'+')
    .remove(b',')
    .remove(b';')
    .remove(b'=')
    .remove(b':')
    .remove(b'@')
    .remove(b'/');

/// Percent-encode a key for use in a URI path.
fn encode_key(key: &str) -> String {
    utf8_percent_encode(key, PATH_SEGMENT).to_string()
}

/// Configuration options for the KV Storage client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server endpoint URL (default: http://localhost:3000)
    pub endpoint: String,
    /// Authentication token
    pub token: String,
    /// Request timeout in milliseconds (default: 30000)
    pub timeout_ms: u64,
    /// Maximum concurrent streams per session (default: 100)
    pub max_concurrent_streams: u32,
    /// Session timeout in milliseconds (default: 60000)
    pub session_timeout_ms: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:3000".to_string(),
            token: String::new(),
            timeout_ms: 30000,
            max_concurrent_streams: 100,
            session_timeout_ms: 60000,
        }
    }
}

/// HTTP/2 client for KV Storage server
///
/// # Example
/// ```rust,no_run
/// use kv_storage_client::Client;
///
/// #[tokio::main]
/// async fn main() -> Result<(), kv_storage_client::Error> {
///     let client = Client::new("http://localhost:3000", "your-token")?;
///
///     // Store a value
///     let result = client.put("my-key", b"Hello, World!").await?;
///     println!("Stored with hash: {}", result.hash);
///
///     // Retrieve a value
///     let value = client.get("my-key").await?;
///     println!("Retrieved: {:?}", value);
///
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
    http_client: HttpClient<HttpConnector, Full<Bytes>>,
}

impl Client {
    /// Create a new KV Storage client
    ///
    /// # Arguments
    /// * `endpoint` - Server endpoint URL (e.g., "http://localhost:3000")
    /// * `token` - Authentication token
    ///
    /// # Errors
    /// Returns an error if the endpoint URL is invalid
    pub fn new(endpoint: &str, token: &str) -> Result<Self> {
        let config = ClientConfig {
            endpoint: endpoint.to_string(),
            token: token.to_string(),
            ..Default::default()
        };
        Self::with_config(config)
    }

    /// Create a new client with custom configuration
    pub fn with_config(config: ClientConfig) -> Result<Self> {
        // Validate the endpoint URL early
        let _: Uri = config.endpoint.parse()
            .map_err(|e| Error::InvalidUrl(format!("Invalid endpoint URL: {}", e)))?;

        let http_client = HttpClient::builder(TokioExecutor::new())
            .http2_only(true)
            .build(HttpConnector::new());

        Ok(Self {
            config: Arc::new(config),
            http_client,
        })
    }

    /// Get the authentication token
    pub fn token(&self) -> &str {
        &self.config.token
    }

    /// Get the endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.config.endpoint
    }

    /// Internal request method
    async fn request(
        &self,
        path: &str,
        method: &hyper::Method,
        body: Option<Bytes>,
        headers: Option<HashMap<String, String>>,
    ) -> Result<Response<Incoming>> {
        let url = format!("{}{}", self.config.endpoint, path);
        let uri: Uri = url.parse()
            .map_err(|e| Error::InvalidUrl(format!("Invalid request URL: {}", e)))?;

        let mut builder = Request::builder()
            .method(method.clone())
            .uri(uri)
            .header("authorization", format!("Bearer {}", self.config.token));

        if let Some(custom_headers) = headers {
            for (key, value) in custom_headers {
                builder = builder.header(&key, value);
            }
        }

        let req = if let Some(body) = body {
            builder.body(Full::new(body))
        } else {
            builder.body(Full::new(Bytes::new()))
        };

        let req = req.map_err(|e| Error::InvalidRequest(format!("Failed to build request: {}", e)))?;

        debug!("Sending request: {} {}", method, path);

        let timeout = Duration::from_millis(self.config.timeout_ms);
        let response = tokio::time::timeout(timeout, self.http_client.request(req))
            .await
            .map_err(|_| Error::Timeout(self.config.timeout_ms))?
            .map_err(|e| Error::Connection(format!("Request failed: {}", e)))?;

        let status = response.status();

        match status {
            StatusCode::UNAUTHORIZED => Err(Error::Unauthorized),
            StatusCode::NOT_FOUND => Err(Error::NotFound(path.to_string())),
            code if code.is_server_error() => {
                let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
                let message = String::from_utf8_lossy(&body_bytes).to_string();
                Err(Error::ServerError {
                    status: code.as_u16(),
                    message,
                })
            }
            code if code.is_client_error() => {
                let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
                let message = String::from_utf8_lossy(&body_bytes).to_string();
                Err(Error::InvalidRequest(message))
            }
            _ => Ok(response),
        }
    }

    /// Read response body to bytes
    async fn read_body_to_bytes(body: Incoming) -> Result<Vec<u8>> {
        let collected = body.collect()
            .await
            .map_err(|e| Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        Ok(collected.to_bytes().to_vec())
    }

    /// Store a value with a key
    ///
    /// # Arguments
    /// * `key` - The key to store the value under
    /// * `value` - The value bytes to store
    ///
    /// # Returns
    /// Information about the stored value including its hash
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// let result = client.put("user:123", b"{\"name\":\"John\"}").await?;
    /// println!("Hash: {}, Deduplicated: {}", result.hash, result.deduplicated);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put(&self, key: &str, value: &[u8]) -> Result<PutResponse> {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/octet-stream".to_string());

        let path = format!("/{}", encode_key(key));
        let response = self
            .request(&path, &hyper::Method::PUT, Some(Bytes::copy_from_slice(value)), Some(headers))
            .await?;

        let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
        let hash = String::from_utf8_lossy(&body_bytes).trim().to_string();

        let hash_algorithm = "xxhash3-128".to_string();

        Ok(PutResponse {
            hash,
            hash_algorithm,
            deduplicated: false, // TODO: Parse from response headers
        })
    }

    /// Store a string value with a key (convenience method)
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// let result = client.put_str("config", "{\"theme\":\"dark\"}").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put_str(&self, key: &str, value: &str) -> Result<PutResponse> {
        self.put(key, value.as_bytes()).await
    }

    /// Retrieve a value by key
    ///
    /// # Arguments
    /// * `key` - The key to retrieve
    ///
    /// # Returns
    /// The value bytes, or None if the key doesn't exist
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// if let Some(data) = client.get("user:123").await? {
    ///     let text = String::from_utf8_lossy(&data);
    ///     println!("User data: {}", text);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = format!("/{}", encode_key(key));
        match self.request(&path, &hyper::Method::GET, None, None).await {
            Ok(response) => {
                let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
                Ok(Some(body_bytes))
            }
            Err(Error::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Retrieve a string value by key (convenience method)
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// if let Some(text) = client.get_str("config").await? {
    ///     println!("Config: {}", text);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_str(&self, key: &str) -> Result<Option<String>> {
        match self.get(key).await? {
            Some(data) => {
                let text = String::from_utf8(data)
                    .map_err(|e| Error::InvalidRequest(format!("Invalid UTF-8: {}", e)))?;
                Ok(Some(text))
            }
            None => Ok(None),
        }
    }

    /// Delete a key
    ///
    /// # Arguments
    /// * `key` - The key to delete
    ///
    /// # Returns
    /// true if the key was deleted, false if it didn't exist
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// let deleted = client.delete("old-key").await?;
    /// if deleted {
    ///     println!("Key was deleted");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let path = format!("/{}", encode_key(key));
        match self.request(&path, &hyper::Method::DELETE, None, None).await {
            Ok(_) => Ok(true),
            Err(Error::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Get metadata about a key without retrieving the value
    ///
    /// # Arguments
    /// * `key` - The key to get head info for
    ///
    /// # Returns
    /// Head information, or None if the key doesn't exist
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// if let Some(info) = client.head("user:123").await? {
    ///     println!("Size: {} bytes, Refs: {}", info.content_length, info.refs);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn head(&self, key: &str) -> Result<Option<HeadInfo>> {
        let path = format!("/{}", encode_key(key));
        match self.request(&path, &hyper::Method::HEAD, None, None).await {
            Ok(response) => Ok(HeadInfo::from_headers(response.headers())),
            Err(Error::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// List all keys with pagination
    ///
    /// # Arguments
    /// * `offset` - Number of keys to skip (default: 0)
    /// * `limit` - Maximum number of keys to return (default: 100, max: 1000)
    ///
    /// # Returns
    /// List response containing keys and total count
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// let result = client.list_keys(0, 100).await?;
    /// println!("Total keys: {}, showing: {}", result.total, result.keys.len());
    /// for key_info in &result.keys {
    ///     println!("  {} - {} bytes", key_info.key, key_info.size);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_keys(&self, offset: usize, limit: usize) -> Result<ListResponse> {
        let limit = limit.min(1000);
        let path = if offset > 0 {
            format!("/keys?offset={}&limit={}", offset, limit)
        } else {
            format!("/keys?limit={}", limit)
        };

        let mut headers = HashMap::new();
        headers.insert("accept".to_string(), "application/json".to_string());

        let response = self
            .request(&path, &hyper::Method::GET, None, Some(headers))
            .await?;

        let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
        let result: ListResponse = serde_json::from_slice(&body_bytes)?;
        Ok(result)
    }

    /// List all keys (convenience method with default pagination)
    pub async fn list(&self) -> Result<ListResponse> {
        self.list_keys(0, 100).await
    }

    /// Execute multiple operations in a batch
    ///
    /// # Arguments
    /// * `operations` - Array of batch operations to execute
    ///
    /// # Returns
    /// Batch response with results for each operation
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::{Client, BatchOp};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// use kv_storage_client::BatchOp;
    ///
    /// let ops = vec![
    ///     BatchOp::Put { key: "user:1".to_string(), value: r#"{"name":"John"}"#.to_string() },
    ///     BatchOp::Put { key: "user:2".to_string(), value: r#"{"name":"Jane"}"#.to_string() },
    ///     BatchOp::Get { key: "user:1".to_string() },
    ///     BatchOp::Delete { key: "old-key".to_string() },
    /// ];
    ///
    /// let response = client.batch(ops).await?;
    /// for result in response.results {
    ///     if let Some(error) = result.error() {
    ///         eprintln!("Error on {}: {}", result.key(), error);
    ///     } else {
    ///         println!("Operation on {} succeeded", result.key());
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn batch(&self, operations: Vec<BatchOp>) -> Result<BatchResponse> {
        let json = serde_json::to_string(&operations)?;

        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("accept".to_string(), "application/json".to_string());

        let response = self
            .request(
                "/batch",
                &hyper::Method::POST,
                Some(Bytes::from(json)),
                Some(headers),
            )
            .await?;

        let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
        let result: BatchResponse = serde_json::from_slice(&body_bytes)?;
        Ok(result)
    }

    /// Get Prometheus metrics from the server
    ///
    /// # Returns
    /// Raw metrics text
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// let metrics = client.metrics().await?;
    /// println!("Server metrics:\n{}", metrics);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn metrics(&self) -> Result<String> {
        let response = self
            .request("/metrics", &hyper::Method::GET, None, None)
            .await?;

        let body_bytes = Self::read_body_to_bytes(response.into_body()).await?;
        Ok(String::from_utf8_lossy(&body_bytes).to_string())
    }

    /// Check if the server is accessible
    ///
    /// # Returns
    /// true if the server is healthy and accessible
    ///
    /// # Example
    /// ```rust,no_run
    /// # use kv_storage_client::Client;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), kv_storage_client::Error> {
    /// # let client = Client::new("http://localhost:3000", "token")?;
    /// if client.health_check().await? {
    ///     println!("Server is healthy!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn health_check(&self) -> Result<bool> {
        match self.request("/", &hyper::Method::GET, None, None).await {
            Ok(_) => Ok(true),
            Err(Error::NotFound(_)) => Ok(true), // 404 is ok - server is responding
            Err(_) => Ok(false),
        }
    }
}

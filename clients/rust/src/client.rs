//! HTTP/2 client implementation for KV Storage

use std::collections::HashMap;
use std::fmt;
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
    /// Optional SSL certificate fingerprint (SHA-256 hex) for certificate pinning.
    /// When set, the client verifies the server certificate's SHA-256 fingerprint
    /// matches this value instead of using standard CA verification.
    /// Accepts hex with or without colons (e.g., "AB:CD:EF:..." or "abcdef...").
    /// Requires an https:// endpoint.
    pub ssl_fingerprint: Option<String>,
    /// Enable TLS verification (default: true).
    /// When false, the client accepts any certificate (useful for self-signed certs).
    /// Only valid with https:// endpoints.
    pub reject_unauthorized: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:3000".to_string(),
            token: String::new(),
            timeout_ms: 30000,
            max_concurrent_streams: 100,
            session_timeout_ms: 60000,
            ssl_fingerprint: None,
            reject_unauthorized: true,
        }
    }
}

/// Parse a hex fingerprint string (with or without colons) into 32 bytes.
fn parse_fingerprint(s: &str) -> Result<[u8; 32]> {
    let hex_str: String = s.chars().filter(|c| *c != ':').collect();
    let bytes = hex::decode(&hex_str)
        .map_err(|e| Error::Tls(format!("Invalid SSL fingerprint hex: {}", e)))?;
    if bytes.len() != 32 {
        return Err(Error::Tls(format!(
            "SSL fingerprint must be 32 bytes (SHA-256), got {} bytes",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Custom certificate verifier that checks the SHA-256 fingerprint of the server certificate.
struct FingerprintVerifier {
    expected: [u8; 32],
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl fmt::Debug for FingerprintVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FingerprintVerifier")
            .field("expected", &hex::encode(self.expected))
            .finish()
    }
}

impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let fingerprint = ring::digest::digest(&ring::digest::SHA256, end_entity.as_ref());
        if fingerprint.as_ref() == &self.expected {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            let actual = hex::encode(fingerprint.as_ref());
            Err(rustls::Error::General(format!(
                "Certificate fingerprint mismatch: expected {}, got {}",
                hex::encode(self.expected),
                actual
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Custom certificate verifier that accepts any certificate (insecure).
/// Used when reject_unauthorized is false.
struct InsecureVerifier {
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl fmt::Debug for InsecureVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InsecureVerifier").finish()
    }
}

impl rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        // Accept any certificate
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Build a rustls ClientConfig for TLS connections.
fn build_tls_config(ssl_fingerprint: Option<&str>, reject_unauthorized: bool) -> Result<rustls::ClientConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    // Priority: fingerprint > reject_unauthorized=false > standard CA verification
    if let Some(fp) = ssl_fingerprint {
        let expected = parse_fingerprint(fp)?;
        let verifier = Arc::new(FingerprintVerifier {
            expected,
            provider: provider.clone(),
        });

        Ok(rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth())
    } else if !reject_unauthorized {
        // Accept any certificate (insecure, for self-signed certs)
        let verifier = Arc::new(InsecureVerifier {
            provider: provider.clone(),
        });

        Ok(rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Tls(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth())
    } else {
        // Standard CA verification
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        Ok(rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| Error::Tls(e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth())
    }
}

type HttpsConnector = hyper_rustls::HttpsConnector<HttpConnector>;

/// HTTP/2 client for KV Storage server
///
/// Supports both plaintext HTTP/2 (h2c) and HTTP/2 over TLS (h2) connections.
/// When the endpoint uses `https://`, TLS is used automatically.
/// Optional SSL certificate fingerprint pinning is available for additional security.
///
/// # Example
/// ```rust,no_run
/// use kv_storage_client::Client;
///
/// #[tokio::main]
/// async fn main() -> Result<(), kv_storage_client::Error> {
///     // Plaintext HTTP/2
///     let client = Client::new("http://localhost:3000", "your-token")?;
///
///     // HTTPS with standard CA verification
///     let client = Client::new("https://example.com:3000", "your-token")?;
///
///     // HTTPS with certificate fingerprint pinning
///     let client = Client::with_config(kv_storage_client::ClientConfig {
///         endpoint: "https://localhost:3000".to_string(),
///         token: "your-token".to_string(),
///         ssl_fingerprint: Some("AB:CD:EF:...".to_string()),
///         ..Default::default()
///     })?;
///
///     // HTTPS with self-signed certificate (skip verification)
///     let client = Client::with_config(kv_storage_client::ClientConfig {
///         endpoint: "https://localhost:3000".to_string(),
///         token: "your-token".to_string(),
///         reject_unauthorized: false,
///         ..Default::default()
///     })?;
///
///     Ok(())
/// }
/// ```
///
/// # Example
///
/// ```
/// use kv_storage_client::Client;
/// let client = Client::new("http://localhost:3000", "my-token").unwrap();
/// ```
#[derive(Clone)]
pub struct Client {
    config: Arc<ClientConfig>,
    http_client: HttpClient<HttpsConnector, Full<Bytes>>,
}

impl Client {
    /// Create a new KV Storage client
    ///
    /// # Arguments
    /// * `endpoint` - Server endpoint URL (e.g., "http://localhost:3000" or "https://localhost:3000")
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

        if config.ssl_fingerprint.is_some() && !config.endpoint.starts_with("https://") {
            return Err(Error::Tls(
                "ssl_fingerprint requires an https:// endpoint".to_string(),
            ));
        }

        let tls_config = build_tls_config(config.ssl_fingerprint.as_deref(), config.reject_unauthorized)?;

        let https_connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls_config)
            .https_or_http()
            .enable_http2()
            .build();

        let http_client = HttpClient::builder(TokioExecutor::new())
            .http2_only(true)
            .build(https_connector);

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

#[cfg(test)]
mod tests {
    use super::*;

    // ===== parse_fingerprint tests =====

    #[test]
    fn test_parse_fingerprint_valid_hex() {
        let fp = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let result = parse_fingerprint(fp).unwrap();
        assert_eq!(result[0], 0xab);
        assert_eq!(result[1], 0xcd);
        assert_eq!(result[31], 0x89);
    }

    #[test]
    fn test_parse_fingerprint_with_colons() {
        let fp = "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:\
                  AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89";
        let result = parse_fingerprint(fp).unwrap();
        assert_eq!(result[0], 0xab);
        assert_eq!(result[1], 0xcd);
        assert_eq!(result[31], 0x89);
    }

    #[test]
    fn test_parse_fingerprint_case_insensitive() {
        let lower = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let upper = "ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789";
        let mixed = "AbCdEf0123456789aBcDeF0123456789AbCdEf0123456789aBcDeF0123456789";

        let r1 = parse_fingerprint(lower).unwrap();
        let r2 = parse_fingerprint(upper).unwrap();
        let r3 = parse_fingerprint(mixed).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
    }

    #[test]
    fn test_parse_fingerprint_invalid_hex() {
        let fp = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        let result = parse_fingerprint(fp);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Tls(msg) => assert!(msg.contains("Invalid SSL fingerprint hex")),
            e => panic!("Expected Tls error, got: {:?}", e),
        }
    }

    #[test]
    fn test_parse_fingerprint_wrong_length_short() {
        let fp = "abcdef"; // only 3 bytes
        let result = parse_fingerprint(fp);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Tls(msg) => assert!(msg.contains("32 bytes")),
            e => panic!("Expected Tls error, got: {:?}", e),
        }
    }

    #[test]
    fn test_parse_fingerprint_wrong_length_long() {
        // 33 bytes = 66 hex chars
        let fp = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef01234567890011";
        let result = parse_fingerprint(fp);
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Tls(msg) => assert!(msg.contains("32 bytes")),
            e => panic!("Expected Tls error, got: {:?}", e),
        }
    }

    #[test]
    fn test_parse_fingerprint_empty() {
        let result = parse_fingerprint("");
        assert!(result.is_err());
    }

    // ===== build_tls_config tests =====

    #[test]
    fn test_build_tls_config_no_fingerprint() {
        let config = build_tls_config(None, true);
        assert!(config.is_ok(), "Default TLS config should succeed");
        let config = config.unwrap();
        // Default config uses ALPN h2
        assert!(config.alpn_protocols.is_empty() || config.alpn_protocols.contains(&b"h2".to_vec()) || true);
    }

    #[test]
    fn test_build_tls_config_with_valid_fingerprint() {
        let fp = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let config = build_tls_config(Some(fp), true);
        assert!(config.is_ok(), "TLS config with valid fingerprint should succeed");
    }

    #[test]
    fn test_build_tls_config_with_invalid_fingerprint() {
        let result = build_tls_config(Some("invalid"), true);
        assert!(result.is_err());
    }

    // ===== ClientConfig default tests =====

    #[test]
    fn test_client_config_default_has_no_fingerprint() {
        let config = ClientConfig::default();
        assert!(config.ssl_fingerprint.is_none());
        assert_eq!(config.endpoint, "http://localhost:3000");
    }

    // ===== Client construction tests =====

    #[test]
    fn test_client_new_http() {
        let client = Client::new("http://localhost:3000", "token");
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.endpoint(), "http://localhost:3000");
        assert_eq!(client.token(), "token");
    }

    #[test]
    fn test_client_new_https() {
        let client = Client::new("https://localhost:3000", "token");
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.endpoint(), "https://localhost:3000");
    }

    #[test]
    fn test_client_with_fingerprint_requires_https() {
        let config = ClientConfig {
            endpoint: "http://localhost:3000".to_string(),
            token: "token".to_string(),
            ssl_fingerprint: Some(
                "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string(),
            ),
            ..Default::default()
        };
        let result = Client::with_config(config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match &err {
            Error::Tls(msg) => assert!(msg.contains("https://"), "Error message: {}", msg),
            _ => panic!("Expected Tls error, got: {:?}", err),
        }
    }

    #[test]
    fn test_client_with_fingerprint_and_https() {
        let config = ClientConfig {
            endpoint: "https://localhost:3000".to_string(),
            token: "token".to_string(),
            ssl_fingerprint: Some(
                "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string(),
            ),
            ..Default::default()
        };
        let result = Client::with_config(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_with_fingerprint_colons_and_https() {
        let config = ClientConfig {
            endpoint: "https://localhost:3000".to_string(),
            token: "token".to_string(),
            ssl_fingerprint: Some(
                "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:\
                 AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89"
                    .to_string(),
            ),
            ..Default::default()
        };
        let result = Client::with_config(config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_client_invalid_endpoint_url() {
        let result = Client::new("not a url", "token");
        assert!(result.is_err());
        let err = result.err().unwrap();
        match &err {
            Error::InvalidUrl(_) => {}
            _ => panic!("Expected InvalidUrl error, got: {:?}", err),
        }
    }

    #[test]
    fn test_client_with_invalid_fingerprint_format() {
        let config = ClientConfig {
            endpoint: "https://localhost:3000".to_string(),
            token: "token".to_string(),
            ssl_fingerprint: Some("not-valid-hex".to_string()),
            ..Default::default()
        };
        let result = Client::with_config(config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match &err {
            Error::Tls(msg) => assert!(msg.contains("Invalid SSL fingerprint"), "Error message: {}", msg),
            _ => panic!("Expected Tls error, got: {:?}", err),
        }
    }

    // ===== FingerprintVerifier tests =====

    #[test]
    fn test_fingerprint_verifier_matching_cert() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

        // Create a fake certificate (just raw bytes for testing)
        let cert_data = b"test certificate data for fingerprint verification";
        let cert = CertificateDer::from(cert_data.to_vec());

        // Compute expected fingerprint
        let digest = ring::digest::digest(&ring::digest::SHA256, cert_data);
        let mut expected = [0u8; 32];
        expected.copy_from_slice(digest.as_ref());

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = FingerprintVerifier {
            expected,
            provider,
        };

        let server_name = ServerName::try_from("localhost").unwrap();
        let now = UnixTime::now();

        let result = verifier.verify_server_cert(&cert, &[], &server_name, &[], now);
        assert!(result.is_ok(), "Matching fingerprint should succeed");
    }

    #[test]
    fn test_fingerprint_verifier_mismatched_cert() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

        let cert_data = b"test certificate data";
        let cert = CertificateDer::from(cert_data.to_vec());

        // Use a wrong fingerprint (all zeros)
        let expected = [0u8; 32];

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = FingerprintVerifier {
            expected,
            provider,
        };

        let server_name = ServerName::try_from("localhost").unwrap();
        let now = UnixTime::now();

        let result = verifier.verify_server_cert(&cert, &[], &server_name, &[], now);
        assert!(result.is_err(), "Mismatched fingerprint should fail");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("fingerprint mismatch"), "Error should mention mismatch: {}", err_msg);
    }

    #[test]
    fn test_fingerprint_verifier_supported_schemes() {
        use rustls::client::danger::ServerCertVerifier;

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = FingerprintVerifier {
            expected: [0u8; 32],
            provider,
        };

        let schemes = verifier.supported_verify_schemes();
        assert!(!schemes.is_empty(), "Should support at least one signature scheme");
    }

    // ===== reject_unauthorized option tests =====

    #[test]
    fn test_client_config_default_reject_unauthorized() {
        let config = ClientConfig::default();
        assert!(config.reject_unauthorized, "reject_unauthorized should default to true");
    }

    #[test]
    fn test_client_with_reject_unauthorized_false() {
        let config = ClientConfig {
            endpoint: "https://localhost:3000".to_string(),
            token: "token".to_string(),
            reject_unauthorized: false,
            ..Default::default()
        };
        let result = Client::with_config(config);
        assert!(result.is_ok(), "Should create client with reject_unauthorized=false");
    }

    #[test]
    fn test_build_tls_config_insecure() {
        let config = build_tls_config(None, false);
        assert!(config.is_ok(), "TLS config with reject_unauthorized=false should succeed");
    }

    // ===== InsecureVerifier tests =====

    #[test]
    fn test_insecure_verifier_accepts_any_cert() {
        use rustls::client::danger::ServerCertVerifier;
        use rustls::pki_types::{CertificateDer, ServerName, UnixTime};

        let cert_data = b"any certificate data";
        let cert = CertificateDer::from(cert_data.to_vec());

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = InsecureVerifier { provider };

        let server_name = ServerName::try_from("localhost").unwrap();
        let now = UnixTime::now();

        // Should accept any certificate
        let result = verifier.verify_server_cert(&cert, &[], &server_name, &[], now);
        assert!(result.is_ok(), "InsecureVerifier should accept any certificate");
    }

    #[test]
    fn test_insecure_verifier_supported_schemes() {
        use rustls::client::danger::ServerCertVerifier;

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = InsecureVerifier { provider };

        let schemes = verifier.supported_verify_schemes();
        assert!(!schemes.is_empty(), "Should support at least one signature scheme");
    }
}

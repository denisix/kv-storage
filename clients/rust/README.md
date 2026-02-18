# KV Storage Client for Rust

A modern HTTP/2 client library for interacting with the KV Storage server.

## Features

- **HTTP/2 Support** - Full HTTP/2 (h2c and h2) with connection pooling and multiplexing
- **TLS/SSL** - HTTPS with standard CA verification, self-signed certs, or fingerprint pinning
- **Async/Await** - Built on tokio for efficient async operations
- **Type Safe** - Strongly typed API with comprehensive error handling
- **Batch Operations** - Execute multiple operations in a single request
- **Binary & Text** - Support for both binary and string data
- **Authentication** - Bearer token authentication
- **Key Encoding** - Automatic percent-encoding for special characters, unicode, spaces

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
kv-storage-client = "0.1.2"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

```rust
use kv_storage_client::Client;

#[tokio::main]
async fn main() -> Result<(), kv_storage_client::Error> {
    // Create a client
    let client = Client::new("http://localhost:3000", "your-token")?;

    // Store a value
    let result = client.put("my-key", b"Hello, World!").await?;
    println!("Stored with hash: {}", result.hash);

    // Retrieve the value
    if let Some(data) = client.get("my-key").await? {
        println!("Retrieved: {:?}", String::from_utf8_lossy(&data));
    }

    Ok(())
}
```

## Examples

### Basic Operations

```rust
use kv_storage_client::Client;

let client = Client::new("http://localhost:3000", "your-token")?;

// Store binary data
client.put("file:1", binary_data).await?;

// Store string data
client.put_str("config", "{\"theme\":\"dark\"}").await?;

// Retrieve data
if let Some(data) = client.get("file:1").await? {
    // Process the data
}

// Retrieve as string
if let Some(text) = client.get_str("config").await? {
    println!("Config: {}", text);
}

// Delete a key
client.delete("file:1").await?;

// Get metadata without retrieving value
if let Some(info) = client.head("file:1").await? {
    println!("Size: {} bytes", info.content_length);
}
```

### Keys with Special Characters

```rust
use kv_storage_client::Client;

let client = Client::new("http://localhost:3000", "your-token")?;

// Keys are automatically percent-encoded
client.put("my key with spaces", b"data").await?;
client.put("path/to/file.txt", b"data").await?;
client.put("ключ", b"data").await?;  // Unicode
client.put("user:123", b"data").await?;  // Colons
```

### Batch Operations

```rust
use kv_storage_client::{Client, BatchOp};

let client = Client::new("http://localhost:3000", "your-token")?;

let operations = vec![
    BatchOp::Put {
        key: "user:1".to_string(),
        value: r#"{"name":"Alice"}"#.to_string(),
    },
    BatchOp::Put {
        key: "user:2".to_string(),
        value: r#"{"name":"Bob"}"#.to_string(),
    },
    BatchOp::Get {
        key: "user:1".to_string(),
    },
];

let response = client.batch(operations).await?;

for result in response.results {
    if let Some(error) = result.error() {
        eprintln!("Error on {}: {}", result.key(), error);
    }
}
```

### List Keys

```rust
use kv_storage_client::Client;

let client = Client::new("http://localhost:3000", "your-token")?;

// List first 100 keys
let result = client.list_keys(0, 100).await?;
println!("Total keys: {}", result.total);

for key_info in &result.keys {
    println!("  {} - {} bytes", key_info.key, key_info.size);
}
```

### Health Check & Metrics

```rust
use kv_storage_client::Client;

let client = Client::new("http://localhost:3000", "your-token")?;

// Check server health
if client.health_check().await? {
    println!("Server is healthy!");
}

// Get Prometheus metrics
let metrics = client.metrics().await?;
println!("Metrics:\n{}", metrics);
```

## Configuration

```rust
use kv_storage_client::{Client, ClientConfig};

let config = ClientConfig {
    endpoint: "http://localhost:3000".to_string(),
    token: "your-token".to_string(),
    timeout_ms: 60000,           // Request timeout (default: 30000)
    max_concurrent_streams: 200, // HTTP/2 concurrent streams (default: 100)
    session_timeout_ms: 120000,  // Session timeout (default: 60000)
    ssl_fingerprint: None,       // Certificate fingerprint pinning
    reject_unauthorized: true,   // TLS verification (default: true)
};

let client = Client::with_config(config)?;
```

## TLS/SSL

### Standard HTTPS

```rust
use kv_storage_client::Client;

// Uses system CA certificates for verification
// Default server HTTPS port is 3443
let client = Client::new("https://example.com:3443", "your-token")?;
```

### Self-Signed Certificates

For development or testing with self-signed certificates:

```rust
use kv_storage_client::{Client, ClientConfig};

let config = ClientConfig {
    endpoint: "https://localhost:3000".to_string(),
    token: "your-token".to_string(),
    reject_unauthorized: false, // Accept self-signed certificates
    ..Default::default()
};

let client = Client::with_config(config)?;
```

**Warning:** `reject_unauthorized: false` disables all certificate verification. Use only for development.

### Certificate Fingerprint Pinning

For enhanced security with self-signed or custom certificates:

```rust
use kv_storage_client::{Client, ClientConfig};

let config = ClientConfig {
    endpoint: "https://localhost:3443".to_string(),  // Default HTTPS port
    token: "your-token".to_string(),
    ssl_fingerprint: Some(
        "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:"
         "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89:"
         "AB:CD:EF:01:23:45:67:89:AB:CD:EF:01:23:45:67:89".to_string()
    ),
    ..Default::default()
};

let client = Client::with_config(config)?;
```

#### Getting the Certificate Fingerprint

```bash
# Get SHA-256 fingerprint from certificate file
openssl x509 -in cert.pem -noout -fingerprint -sha256

# Convert to lowercase hex without colons
openssl x509 -in cert.pem -noout -fingerprint -sha256 | cut -d= -f2 | tr -d : | tr '[:upper:]' '[:lower:]'
```

The `ssl_fingerprint` field accepts either format:
- With colons: `"AB:CD:EF:01:23:45:..."`
- Without colons: `"abcdef012345..."`

## Error Handling

```rust
use kv_storage_client::{Client, Error};

match client.get("my-key").await {
    Ok(Some(data)) => println!("Got data: {:?}", data),
    Ok(None) => println!("Key not found"),
    Err(Error::Unauthorized) => eprintln!("Invalid token"),
    Err(Error::NotFound(key)) => eprintln!("Key not found: {}", key),
    Err(Error::Timeout(ms)) => eprintln!("Request timed out after {}ms", ms),
    Err(Error::Connection(msg)) => eprintln!("Connection error: {}", msg),
    Err(Error::Tls(msg)) => eprintln!("TLS error: {}", msg),
    Err(e) => eprintln!("Other error: {:?}", e),
}
```

## Running Tests

```bash
# Unit tests (no server required)
cargo test --lib

# Integration tests (requires running server)
TEST_ENDPOINT=http://localhost:3000 TEST_TOKEN=test-token cargo test --tests

# TLS integration tests (requires HTTPS server)
TEST_ENDPOINT=https://localhost:3000 TEST_TOKEN=test-token \
cargo test --test tls_integration_test -- --test-threads=1
```

## API Reference

See the [documentation](https://docs.rs/kv-storage-client) for full API reference.

## License

MIT

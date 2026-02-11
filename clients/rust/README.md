# KV Storage Client for Rust

A modern HTTP/2 client library for interacting with the KV Storage server.

## Features

- **HTTP/2 Support** - Full HTTP/2 support with connection pooling
- **Async/Await** - Built on tokio for efficient async operations
- **Type Safe** - Strongly typed API with comprehensive error handling
- **Batch Operations** - Execute multiple operations atomically
- **Binary & Text** - Support for both binary and string data
- **Authentication** - Bearer token authentication

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
kv-storage-client = "0.1"
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

You can customize the client behavior:

```rust
use kv_storage_client::{Client, ClientConfig};

let config = ClientConfig {
    endpoint: "http://localhost:3000".to_string(),
    token: "your-token".to_string(),
    timeout_ms: 60000,        // 60 second timeout
    max_concurrent_streams: 200,
    session_timeout_ms: 120000, // 2 minute session timeout
};

let client = Client::with_config(config)?;
```

## API Reference

See the [documentation](https://docs.rs/kv-storage-client) for full API reference.

## Running Examples

The examples require a running KV Storage server:

```bash
# Start the server
cd /path/to/kv-storage
cargo run -- --token dev-token

# Run basic usage example
cargo run --example basic_usage

# Run batch operations example
cargo run --example batch_operations
```

## License

MIT

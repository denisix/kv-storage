//! Basic usage example for the KV Storage client
//!
//! Run with: cargo run --example basic_usage -- --token <your-token>

use kv_storage_client::Client;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    // Get token from command line or environment
    let token = std::env::var("KV_TOKEN").unwrap_or_else(|_| "dev-token".to_string());

    // Create client
    let client = Client::new("http://localhost:3000", &token)?;

    // Check server health
    info!("Checking server health...");
    let healthy = client.health_check().await?;
    info!("Server healthy: {}", healthy);

    // Store a value
    info!("Storing key 'example:hello'...");
    let result = client.put("example:hello", b"Hello, KV Storage!").await?;
    info!("Stored! Hash: {}", result.hash);

    // Retrieve the value
    info!("Retrieving key 'example:hello'...");
    if let Some(data) = client.get("example:hello").await? {
        let text = String::from_utf8_lossy(&data);
        info!("Retrieved: {}", text);
    } else {
        info!("Key not found");
    }

    // Store JSON data
    info!("Storing JSON data...");
    let json_data = r#"{"name":"Alice","age":30,"city":"NYC"}"#;
    let result = client.put_str("user:alice", json_data).await?;
    info!("Stored user data! Hash: {}", result.hash);

    // Get head info
    info!("Getting head info for 'example:hello'...");
    if let Some(info) = client.head("example:hello").await? {
        info!("Size: {} bytes", info.content_length);
        info!("Hash: {}", info.hash);
        info!("Refs: {}", info.refs);
    }

    // List keys
    info!("Listing first 10 keys...");
    let list_result = client.list_keys(0, 10).await?;
    info!("Total keys: {}", list_result.total);
    for key_info in &list_result.keys {
        info!("  - {} ({} bytes)", key_info.key, key_info.size);
    }

    // Delete a key
    info!("Deleting key 'example:hello'...");
    let deleted = client.delete("example:hello").await?;
    info!("Deleted: {}", deleted);

    info!("Example completed successfully!");
    Ok(())
}

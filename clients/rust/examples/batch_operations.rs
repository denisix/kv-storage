//! Batch operations example for the KV Storage client
//!
//! Demonstrates atomic batch operations with multiple PUT, GET, and DELETE operations.
//!
//! Run with: cargo run --example batch_operations -- --token <your-token>

use kv_storage_client::{BatchOp, Client};
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

    info!("=== Batch Operations Example ===");

    // Prepare batch operations
    let operations = vec![
        // Store multiple values
        BatchOp::Put {
            key: "batch:user:1".to_string(),
            value: r#"{"name":"Alice","role":"admin"}"#.to_string(),
        },
        BatchOp::Put {
            key: "batch:user:2".to_string(),
            value: r#"{"name":"Bob","role":"user"}"#.to_string(),
        },
        BatchOp::Put {
            key: "batch:user:3".to_string(),
            value: r#"{"name":"Charlie","role":"user"}"#.to_string(),
        },
        // Read back a value
        BatchOp::Get {
            key: "batch:user:1".to_string(),
        },
        // Try to read non-existent key
        BatchOp::Get {
            key: "batch:user:999".to_string(),
        },
    ];

    info!("Executing batch with {} operations...", operations.len());

    // Execute batch
    let response = client.batch(operations).await?;

    info!("Batch results:");
    for (i, result) in response.results.iter().enumerate() {
        match result {
            kv_storage_client::BatchResult::Put { key, hash, created } => {
                info!(
                    "  [{}] PUT {} - hash: {}, created: {}",
                    i + 1,
                    key,
                    hash,
                    created
                );
            }
            kv_storage_client::BatchResult::Get { key, value, found } => {
                if *found {
                    info!(
                        "  [{}] GET {} - value: {}",
                        i + 1,
                        key,
                        value.as_ref().unwrap_or(&"<none>".to_string())
                    );
                } else {
                    info!("  [{}] GET {} - not found", i + 1, key);
                }
            }
            kv_storage_client::BatchResult::Delete { key, deleted } => {
                info!("  [{}] DELETE {} - deleted: {}", i + 1, key, deleted);
            }
            kv_storage_client::BatchResult::Error { key, error } => {
                info!("  [{}] ERROR on {}: {}", i + 1, key, error);
            }
        }
    }

    // Now let's do another batch to delete the keys
    info!("\nCleaning up with DELETE batch...");
    let cleanup_ops = vec![
        BatchOp::Delete {
            key: "batch:user:1".to_string(),
        },
        BatchOp::Delete {
            key: "batch:user:2".to_string(),
        },
        BatchOp::Delete {
            key: "batch:user:3".to_string(),
        },
    ];

    let cleanup_response = client.batch(cleanup_ops).await?;
    for result in &cleanup_response.results {
        if let kv_storage_client::BatchResult::Delete { key, deleted } = result {
            info!("  DELETE {} - success: {}", key, deleted);
        }
    }

    info!("Batch operations example completed!");
    Ok(())
}

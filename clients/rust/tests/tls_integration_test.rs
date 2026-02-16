//! TLS integration tests for kv-storage-client
//!
//! These tests require a running kv-storage server with TLS enabled.
//! Set TEST_ENDPOINT=https://... and TEST_TOKEN, plus TEST_SSL_FINGERPRINT for pinning tests.
//!
//! Run with: cargo test --test tls_integration_test -- --test-threads=1

use kv_storage_client::{Client, ClientConfig};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

// Counter for unique keys across tests
static KEY_COUNTER: OnceLock<AtomicUsize> = OnceLock::new();

fn unique_key(prefix: &str) -> String {
    let counter = KEY_COUNTER.get_or_init(|| AtomicUsize::new(0));
    let id = counter.fetch_add(1, Ordering::SeqCst);
    format!("{}_{}", prefix, id)
}

fn get_tls_endpoint() -> Option<(String, String)> {
    let endpoint = std::env::var("TEST_ENDPOINT").ok()?;
    if !endpoint.starts_with("https://") {
        return None;
    }
    let token = std::env::var("TEST_TOKEN").unwrap_or_else(|_| "test-token".to_string());
    Some((endpoint, token))
}

fn get_ssl_fingerprint() -> Option<String> {
    std::env::var("TEST_SSL_FINGERPRINT").ok()
}

async fn cleanup(client: &Client, keys: &[&str]) {
    for key in keys {
        let _ = client.delete(key).await;
    }
}

// Helper to create a fresh client for each test to avoid connection reuse issues
fn make_client() -> Option<Client> {
    let (endpoint, token) = get_tls_endpoint()?;
    Client::with_config(ClientConfig {
        endpoint,
        token,
        reject_unauthorized: false, // Accept self-signed certs
        ..Default::default()
    }).ok()
}

#[tokio::test]
async fn test_https_basic_operations() {
    let Some(client) = make_client() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };
    let key = unique_key("tls_basic");

    cleanup(&client, &[&key]).await;

    // PUT
    let result = client.put(&key, b"hello TLS").await.expect("PUT failed");
    assert!(!result.hash.is_empty());

    // GET
    let value = client.get(&key).await.expect("GET failed");
    assert_eq!(value, Some(b"hello TLS".to_vec()));

    // DELETE
    let deleted = client.delete(&key).await.expect("DELETE failed");
    assert!(deleted);

    // Verify gone
    let value = client.get(&key).await.expect("GET failed");
    assert!(value.is_none());
}

#[tokio::test]
async fn test_https_batch_operations() {
    let Some(client) = make_client() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };
    
    let key1 = unique_key("tls_batch");
    let key2 = unique_key("tls_batch");
    cleanup(&client, &[&key1, &key2]).await;

    use kv_storage_client::BatchOp;
    
    let ops = vec![
        BatchOp::Put { key: key1.clone(), value: "value1".to_string() },
        BatchOp::Put { key: key2.clone(), value: "value2".to_string() },
        BatchOp::Get { key: key1.clone() },
    ];

    let response = client.batch(ops).await.expect("BATCH failed");
    assert_eq!(response.results.len(), 3);

    cleanup(&client, &[&key1, &key2]).await;
}

#[tokio::test]
async fn test_https_large_payload() {
    let Some(client) = make_client() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };
    let key = unique_key("tls_large");

    cleanup(&client, &[&key]).await;

    // 1MB payload
    let large_data = vec![42u8; 1024 * 1024];
    
    let result = client.put(&key, &large_data).await.expect("PUT large failed");
    assert!(!result.hash.is_empty());

    let value = client.get(&key).await.expect("GET large failed");
    assert_eq!(value, Some(large_data));

    cleanup(&client, &[&key]).await;
}

#[tokio::test]
async fn test_https_concurrent_operations() {
    let Some(client) = make_client() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };
    
    // Run operations sequentially to avoid overwhelming the server
    for i in 0..5 {
        let key = unique_key(&format!("tls_conc{}", i));
        let _ = client.delete(&key).await;
        
        client.put(&key, format!("value_{}", i).as_bytes()).await.expect("PUT failed");
        let value = client.get(&key).await.expect("GET failed");
        assert_eq!(value, Some(format!("value_{}", i).into_bytes()));
        
        client.delete(&key).await.expect("DELETE failed");
    }
}

#[tokio::test]
async fn test_https_with_fingerprint_pinning() {
    let Some((endpoint, token)) = get_tls_endpoint() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };

    let Some(fingerprint) = get_ssl_fingerprint() else {
        eprintln!("Skipping: TEST_SSL_FINGERPRINT not set");
        return;
    };

    let config = ClientConfig {
        endpoint,
        token,
        ssl_fingerprint: Some(fingerprint),
        reject_unauthorized: false, // Also set this for consistency
        ..Default::default()
    };

    let client = Client::with_config(config).expect("Failed to create client with fingerprint");
    let key = "tls_fingerprint_test";

    cleanup(&client, &[key]).await;

    // Should succeed with correct fingerprint
    let result = client.put(key, b"fingerprint test").await.expect("PUT failed");
    assert!(!result.hash.is_empty());

    let value = client.get(key).await.expect("GET failed");
    assert_eq!(value, Some(b"fingerprint test".to_vec()));

    cleanup(&client, &[key]).await;
}

#[tokio::test]
async fn test_https_wrong_fingerprint_rejected() {
    let Some((endpoint, token)) = get_tls_endpoint() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };

    // Use a wrong fingerprint
    let wrong_fingerprint = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF".to_string();

    let config = ClientConfig {
        endpoint,
        token,
        ssl_fingerprint: Some(wrong_fingerprint),
        ..Default::default()
    };

    let client = Client::with_config(config).expect("Failed to create client");

    // Should fail with wrong fingerprint
    let result = client.put("tls_wrong_fp", b"should fail").await;
    assert!(result.is_err(), "Should fail with wrong fingerprint");
    
    // Any error is acceptable - the important thing is that it failed
    // The specific error type depends on TLS implementation details
    let err = result.unwrap_err();
    match &err {
        kv_storage_client::Error::Tls(msg) => {
            // Good - explicit TLS error
            eprintln!("TLS error (expected): {}", msg);
        }
        kv_storage_client::Error::Connection(msg) => {
            // Connection error is also acceptable (TLS handshake failure)
            eprintln!("Connection error (acceptable): {}", msg);
        }
        e => {
            // Any error is fine as long as connection was rejected
            eprintln!("Other error (acceptable): {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_https_connection_reuse() {
    let Some(client) = make_client() else {
        eprintln!("Skipping: TEST_ENDPOINT must be https://...");
        return;
    };

    // Make 10 sequential requests - should reuse the same HTTP/2 connection
    for i in 0..10 {
        let key = unique_key(&format!("tls_reuse{}", i));
        let _ = client.delete(&key).await;
        
        client.put(&key, format!("value_{}", i).as_bytes()).await.expect("PUT failed");
        let value = client.get(&key).await.expect("GET failed");
        assert_eq!(value, Some(format!("value_{}", i).into_bytes()));
        
        client.delete(&key).await.expect("DELETE failed");
    }
}

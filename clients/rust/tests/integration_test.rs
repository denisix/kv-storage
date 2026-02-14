//! Integration tests for kv-storage-client
//!
//! These tests require a running kv-storage server.
//! Set TEST_ENDPOINT and TEST_TOKEN environment variables to configure.
//!
//! Run with: cargo test --tests

use kv_storage_client::{Client, BatchOp};
use std::env;

// ========== Special Character Key Tests ==========

fn get_client() -> Client {
    let endpoint = env::var("TEST_ENDPOINT").unwrap_or_else(|_| "http://localhost:3000".to_string());
    let token = env::var("TEST_TOKEN").unwrap_or_else(|_| "test-token".to_string());
    Client::new(&endpoint, &token).expect("Failed to create client")
}

fn get_test_key(name: &str) -> String {
    format!("rust_test_{}", name)
}

async fn cleanup(client: &Client, keys: &[&str]) {
    for key in keys {
        let _ = client.delete(key).await;
    }
}

#[tokio::test]
async fn test_put_and_get() {
    let client = get_client();
    let key = get_test_key("put_and_get");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    // PUT
    let result = client.put(&key, b"Hello, World!").await.unwrap();
    assert!(!result.hash.is_empty());

    // GET
    let value = client.get(&key).await.unwrap();
    assert_eq!(value, Some(b"Hello, World!".to_vec()));

    // Cleanup
    cleanup(&client, &[&key]).await;
}

#[tokio::test]
async fn test_put_updates_existing_key() {
    let client = get_client();
    let key = get_test_key("update");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    // PUT first value
    let result1 = client.put(&key, b"first value").await.unwrap();
    assert!(!result1.hash.is_empty());

    let value1 = client.get(&key).await.unwrap();
    assert_eq!(value1, Some(b"first value".to_vec()));

    // PUT second value (update)
    let result2 = client.put(&key, b"second value").await.unwrap();
    assert!(!result2.hash.is_empty());

    let value2 = client.get(&key).await.unwrap();
    assert_eq!(value2, Some(b"second value".to_vec()));

    // Cleanup
    cleanup(&client, &[&key]).await;
}

#[tokio::test]
async fn test_get_nonexistent_key() {
    let client = get_client();
    let key = get_test_key("nonexistent");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    let value = client.get(&key).await.unwrap();
    assert!(value.is_none());
}

#[tokio::test]
async fn test_delete_key() {
    let client = get_client();
    let key = get_test_key("delete");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    // Create key
    client.put(&key, b"to be deleted").await.unwrap();

    // Verify it exists
    let value = client.get(&key).await.unwrap();
    assert_eq!(value, Some(b"to be deleted".to_vec()));

    // Delete it
    let deleted = client.delete(&key).await.unwrap();
    assert!(deleted);

    // Verify it's gone
    let value = client.get(&key).await.unwrap();
    assert!(value.is_none());
}

#[tokio::test]
async fn test_delete_nonexistent_key() {
    let client = get_client();
    let key = get_test_key("delete_nonexistent");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    let deleted = client.delete(&key).await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn test_head_request() {
    let client = get_client();
    let key = get_test_key("head");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    // Create key
    client.put(&key, b"head test data").await.unwrap();

    // HEAD request
    let info = client.head(&key).await.unwrap();
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.content_length, 14);

    // Cleanup
    cleanup(&client, &[&key]).await;
}

#[tokio::test]
async fn test_head_nonexistent_key() {
    let client = get_client();
    let key = get_test_key("head_nonexistent");

    // Cleanup before test
    cleanup(&client, &[&key]).await;

    let info = client.head(&key).await.unwrap();
    assert!(info.is_none());
}

#[tokio::test]
async fn test_list_keys() {
    let client = get_client();
    let key1 = get_test_key("list_1");
    let key2 = get_test_key("list_2");

    // Cleanup before test
    cleanup(&client, &[&key1, &key2]).await;

    // Create keys
    client.put(&key1, b"data1").await.unwrap();
    client.put(&key2, b"data2").await.unwrap();

    // List keys
    let result = client.list_keys(0, 10).await.unwrap();
    assert!(result.total >= 2);

    // Cleanup
    cleanup(&client, &[&key1, &key2]).await;
}

#[tokio::test]
async fn test_batch_operations() {
    let client = get_client();
    let key1 = get_test_key("batch_1");
    let key2 = get_test_key("batch_2");

    // Cleanup before test
    cleanup(&client, &[&key1, &key2]).await;

    // Batch operations - values must be JSON strings for batch API
    let ops = vec![
        BatchOp::Put { key: key1.clone(), value: "batch1".to_string() },
        BatchOp::Put { key: key2.clone(), value: "batch2".to_string() },
        BatchOp::Get { key: key1.clone() },
    ];

    let response = client.batch(ops).await.unwrap();
    assert_eq!(response.results.len(), 3);

    // Verify the GET result
    let get_result = &response.results[2];
    match get_result {
        kv_storage_client::BatchResult::Get { found, .. } => assert!(*found),
        _ => panic!("Expected Get result"),
    }

    // Cleanup
    cleanup(&client, &[&key1, &key2]).await;
}

#[tokio::test]
async fn test_keys_with_spaces() {
    let client = get_client();
    let keys = [
        "rust_test key with spaces",
        "rust_test hello world",
        "rust_test path/to/my file.txt",
    ];

    for key in &keys {
        cleanup(&client, &[key]).await;

        let data = format!("data for {}", key);
        let result = client.put(key, data.as_bytes()).await.unwrap();
        assert!(!result.hash.is_empty(), "PUT failed for key: {}", key);

        let value = client.get(key).await.unwrap();
        assert_eq!(value, Some(data.as_bytes().to_vec()), "GET mismatch for key: {}", key);

        let deleted = client.delete(key).await.unwrap();
        assert!(deleted, "DELETE failed for key: {}", key);
    }
}

#[tokio::test]
async fn test_keys_with_special_characters() {
    let client = get_client();
    let keys = [
        "rust_test:colons:here",
        "rust_test.dots.here",
        "rust_test-dashes-here",
        "rust_test_underscores_here",
        "rust_test/slashes/here",
        "rust_test!exclaim",
        "rust_test~tilde",
        "rust_test(parens)",
    ];

    for key in &keys {
        cleanup(&client, &[key]).await;

        let data = format!("data for {}", key);
        let result = client.put(key, data.as_bytes()).await.unwrap();
        assert!(!result.hash.is_empty(), "PUT failed for key: {}", key);

        let value = client.get(key).await.unwrap();
        assert_eq!(value, Some(data.as_bytes().to_vec()), "GET mismatch for key: {}", key);

        let deleted = client.delete(key).await.unwrap();
        assert!(deleted, "DELETE failed for key: {}", key);
    }
}

#[tokio::test]
async fn test_keys_with_unicode() {
    let client = get_client();
    let keys = [
        "rust_test_ключ",
        "rust_test_键",
        "rust_test_مفتاح",
        "rust_test_日本語キー",
    ];

    for key in &keys {
        cleanup(&client, &[key]).await;

        let data = format!("data for {}", key);
        let result = client.put(key, data.as_bytes()).await.unwrap();
        assert!(!result.hash.is_empty(), "PUT failed for key: {}", key);

        let value = client.get(key).await.unwrap();
        assert_eq!(value, Some(data.as_bytes().to_vec()), "GET mismatch for key: {}", key);

        let deleted = client.delete(key).await.unwrap();
        assert!(deleted, "DELETE failed for key: {}", key);
    }
}

#[tokio::test]
async fn test_keys_with_hash_and_question_mark() {
    let client = get_client();
    // These characters are URI-structural (#, ?) and must be encoded by the client
    let keys = [
        "rust_test#hash",
        "rust_test?question",
        "rust_test%percent",
    ];

    for key in &keys {
        cleanup(&client, &[key]).await;

        let data = format!("data for {}", key);
        let result = client.put(key, data.as_bytes()).await.unwrap();
        assert!(!result.hash.is_empty(), "PUT failed for key: {}", key);

        let value = client.get(key).await.unwrap();
        assert_eq!(value, Some(data.as_bytes().to_vec()), "GET mismatch for key: {}", key);

        let deleted = client.delete(key).await.unwrap();
        assert!(deleted, "DELETE failed for key: {}", key);
    }
}

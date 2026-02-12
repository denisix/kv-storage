use std::time::Duration;

// Helper function to get base URL and token
fn get_config() -> (String, String) {
    (
        std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()),
        std::env::var("TEST_TOKEN").unwrap_or_else(|_| "test-token".to_string()),
    )
}

fn make_auth_request(method: &str, path: &str, body: Option<&[u8]>) -> Result<(String, reqwest::StatusCode), Box<dyn std::error::Error>> {
    let (base_url, token) = get_config();
    let url = format!("{}{}", base_url, path);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut request = match method {
        "GET" => client.get(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "HEAD" => client.head(&url),
        "POST" => client.post(&url),
        _ => return Err("Invalid method".into()),
    };

    request = request.header("Authorization", format!("Bearer {}", token));

    let response = if let Some(body_data) = body {
        request.body(body_data.to_vec()).send()?
    } else {
        request.send()?
    };

    let status = response.status();
    let text = response.text()?;
    Ok((text, status))
}

// ========== Basic Operations Tests ==========

#[test]
fn test_put_and_get() {
    let data = b"Hello, World!";
    make_auth_request("PUT", "/test_put_and_get", Some(data)).unwrap();

    let (result, status) = make_auth_request("GET", "/test_put_and_get", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(result, "Hello, World!");

    // Cleanup
    make_auth_request("DELETE", "/test_put_and_get", None).unwrap();
}

#[test]
fn test_put_creates_new_key() {
    let data = b"test data for creation";
    let (result, status) = make_auth_request("PUT", "/test_put_creates", Some(data)).unwrap();
    assert_eq!(status, reqwest::StatusCode::CREATED);
    assert!(!result.is_empty()); // Should return hash

    // Cleanup
    make_auth_request("DELETE", "/test_put_creates", None).unwrap();
}

#[test]
fn test_put_existing_key_updates() {
    let data = b"first put";
    make_auth_request("PUT", "/test_put_conflict", Some(data)).unwrap();

    // Second put should update and return 200 OK
    let (result, status) = make_auth_request("PUT", "/test_put_conflict", Some(b"second put")).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(!result.is_empty()); // Should return hash

    // Verify the value was updated
    let (result, status) = make_auth_request("GET", "/test_put_conflict", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(result, "second put");

    // Cleanup
    make_auth_request("DELETE", "/test_put_conflict", None).unwrap();
}

#[test]
fn test_get_nonexistent_key() {
    let (_, status) = make_auth_request("GET", "/nonexistent_key_12345", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn test_delete_key() {
    // First create a key
    make_auth_request("PUT", "/test_delete", Some(b"data to delete")).unwrap();

    // Verify it exists
    let (_, status) = make_auth_request("GET", "/test_delete", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);

    // Delete it
    let (_, status) = make_auth_request("DELETE", "/test_delete", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::NO_CONTENT);

    // Verify it's gone
    let (_, status) = make_auth_request("GET", "/test_delete", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn test_delete_nonexistent_key() {
    let (_, status) = make_auth_request("DELETE", "/nonexistent_delete_key", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

// ========== Large File Tests ==========

#[test]
fn test_large_file() {
    let large_data = vec![42u8; 10 * 1024 * 1024]; // 10MB
    make_auth_request("PUT", "/large_file_test", Some(&large_data)).unwrap();

    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}/large_file_test", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let bytes = response.bytes().unwrap();
    assert_eq!(bytes.len(), 10 * 1024 * 1024);
    assert!(bytes.iter().all(|&b| b == 42));

    // Cleanup
    make_auth_request("DELETE", "/large_file_test", None).unwrap();
}

#[test]
fn test_empty_file() {
    let empty_data = b"";
    make_auth_request("PUT", "/empty_file_test", Some(empty_data)).unwrap();

    let (result, status) = make_auth_request("GET", "/empty_file_test", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(result, "");

    // Cleanup
    make_auth_request("DELETE", "/empty_file_test", None).unwrap();
}

// ========== Deduplication Tests ==========

#[test]
fn test_deduplication_same_data() {
    let data = b"dedup test data that should be stored once";

    // First put - creates new object
    let (result1, status1) = make_auth_request("PUT", "/dedup_key1", Some(data)).unwrap();
    assert_eq!(status1, reqwest::StatusCode::CREATED);

    // Second put with different key but same data
    // Since it's a new key, should return 201 CREATED (even though content is deduplicated)
    let (result2, status2) = make_auth_request("PUT", "/dedup_key2", Some(data)).unwrap();
    assert_eq!(status2, reqwest::StatusCode::CREATED);

    // Both should return the same hash
    assert_eq!(result1.trim(), result2.trim());

    // Verify both keys work
    let (val1, _) = make_auth_request("GET", "/dedup_key1", None).unwrap();
    let (val2, _) = make_auth_request("GET", "/dedup_key2", None).unwrap();
    assert_eq!(val1, val2);
    assert_eq!(val1, "dedup test data that should be stored once");

    // Cleanup
    make_auth_request("DELETE", "/dedup_key1", None).unwrap();
    // Second key should still work after deleting first
    let (val3, _) = make_auth_request("GET", "/dedup_key2", None).unwrap();
    assert_eq!(val3, "dedup test data that should be stored once");

    make_auth_request("DELETE", "/dedup_key2", None).unwrap();
}

#[test]
fn test_deduplication_gc() {
    let data = b"data for gc test";

    // Create two keys with same data
    make_auth_request("PUT", "/gc_key1", Some(data)).unwrap();
    make_auth_request("PUT", "/gc_key2", Some(data)).unwrap();

    // Delete first key - object should still exist
    make_auth_request("DELETE", "/gc_key1", None).unwrap();
    let (_, status) = make_auth_request("GET", "/gc_key2", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);

    // Delete second key - object should be GC'd
    make_auth_request("DELETE", "/gc_key2", None).unwrap();
}

// ========== HEAD Request Tests ==========

#[test]
fn test_head_request() {
    let data = b"head test data";
    make_auth_request("PUT", "/head_test_key", Some(data)).unwrap();

    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .head(format!("{}/head_test_key", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.headers().contains_key("x-hash"));
    assert!(response.headers().contains_key("content-length"));
    assert!(response.headers().contains_key("x-created-at"));
    assert!(response.headers().contains_key("x-refs"));

    let content_length = response.headers().get("content-length").unwrap().to_str().unwrap();
    assert_eq!(content_length, data.len().to_string());

    // Cleanup
    make_auth_request("DELETE", "/head_test_key", None).unwrap();
}

#[test]
fn test_head_nonexistent() {
    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .head(format!("{}/nonexistent_head", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}

// ========== Key Listing Tests ==========

#[test]
fn test_list_keys_empty() {
    let (_, status) = make_auth_request("GET", "/keys", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
}

#[test]
fn test_list_keys_pagination() {
    // Clean up first
    for i in 0..5 {
        let _ = make_auth_request("DELETE", &format!("/list_page_test_{}", i), None);
    }

    // Create test keys
    for i in 0..5 {
        make_auth_request("PUT", &format!("/list_page_test_{}", i), Some(format!("value{}", i).as_bytes())).unwrap();
    }

    // Test limit
    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();

    // Get first 2 keys
    let response = client
        .get(format!("{}/keys?limit=2", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().unwrap();
    assert_eq!(json["keys"].as_array().unwrap().len(), 2);
    assert_eq!(json["total"].as_u64().unwrap(), 5);

    // Get with offset
    let response = client
        .get(format!("{}/keys?offset=2&limit=2", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().unwrap();
    assert_eq!(json["keys"].as_array().unwrap().len(), 2);

    // Cleanup
    for i in 0..5 {
        make_auth_request("DELETE", &format!("/list_page_test_{}", i), None).unwrap();
    }
}

// ========== Batch Operations Tests ==========

#[test]
fn test_batch_put_operations() {
    let batch_ops = r#"[
        {"op": "put", "key": "batch_test_1", "value": "value1"},
        {"op": "put", "key": "batch_test_2", "value": "value2"},
        {"op": "put", "key": "batch_test_3", "value": "value3"}
    ]"#;

    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("{}/batch", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .body(batch_ops)
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().unwrap();
    assert_eq!(json["results"].as_array().unwrap().len(), 3);

    // Verify all keys were created
    assert_eq!(make_auth_request("GET", "/batch_test_1", None).unwrap().0, "value1");
    assert_eq!(make_auth_request("GET", "/batch_test_2", None).unwrap().0, "value2");
    assert_eq!(make_auth_request("GET", "/batch_test_3", None).unwrap().0, "value3");

    // Cleanup
    make_auth_request("DELETE", "/batch_test_1", None).unwrap();
    make_auth_request("DELETE", "/batch_test_2", None).unwrap();
    make_auth_request("DELETE", "/batch_test_3", None).unwrap();
}

#[test]
fn test_batch_mixed_operations() {
    // Setup: create a key first
    make_auth_request("PUT", "/batch_mixed_get", Some(b"existing")).unwrap();

    let batch_ops = r#"[
        {"op": "put", "key": "batch_mixed_put", "value": "new_value"},
        {"op": "get", "key": "batch_mixed_get"},
        {"op": "delete", "key": "batch_mixed_get"}
    ]"#;

    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("{}/batch", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .body(batch_ops)
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().unwrap();
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 3);

    // Cleanup
    make_auth_request("DELETE", "/batch_mixed_put", None).unwrap();
}

#[test]
fn test_batch_update_key() {
    // Test that batch PUT can update existing keys
    make_auth_request("PUT", "/batch_update_key", Some(b"first")).unwrap();

    let batch_ops = r#"[
        {"op": "put", "key": "batch_update_key", "value": "second"},
        {"op": "put", "key": "batch_update_new", "value": "new"}
    ]"#;

    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .post(format!("{}/batch", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .body(batch_ops)
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let json: serde_json::Value = response.json().unwrap();
    let results = json["results"].as_array().unwrap();

    // First should update (200 OK), second should create (201 CREATED)
    // Both should succeed with hash
    assert!(results[0]["hash"].is_string());
    assert!(results[1]["hash"].is_string());

    // Verify the value was updated
    let (result, _) = make_auth_request("GET", "/batch_update_key", None).unwrap();
    assert_eq!(result, "second");

    // Cleanup
    make_auth_request("DELETE", "/batch_update_key", None).unwrap();
    make_auth_request("DELETE", "/batch_update_new", None).unwrap();
}

// ========== Authentication Tests ==========

#[test]
fn test_auth_missing_token() {
    let (base_url, _) = get_config();
    let client = reqwest::blocking::Client::new();

    let response = client.get(format!("{}/test_key", base_url)).send().unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[test]
fn test_auth_invalid_token() {
    let (base_url, _) = get_config();
    let client = reqwest::blocking::Client::new();

    let response = client
        .get(format!("{}/test_key", base_url))
        .header("Authorization", "Bearer invalid-token")
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[test]
fn test_auth_wrong_scheme() {
    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();

    let response = client
        .get(format!("{}/test_key", base_url))
        .header("Authorization", format!("Basic {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}

// ========== Metrics Tests ==========

#[test]
fn test_metrics_endpoint() {
    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();

    let response = client
        .get(format!("{}/metrics", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let metrics = response.text().unwrap();
    assert!(metrics.contains("kv_storage_keys_total"));
    assert!(metrics.contains("kv_storage_objects_total"));
    assert!(metrics.contains("kv_storage_bytes_total"));
    assert!(metrics.contains("kv_storage_ops_total"));
    assert!(metrics.contains("kv_storage_dedup_hits_total"));
}

#[test]
fn test_metrics_count_operations() {
    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();

    // Get initial metrics
    let response = client
        .get(format!("{}/metrics", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();
    let initial_metrics = response.text().unwrap();

    // Perform operations
    make_auth_request("PUT", "/metrics_test_key", Some(b"test")).unwrap();
    make_auth_request("GET", "/metrics_test_key", None).unwrap();
    make_auth_request("DELETE", "/metrics_test_key", None).unwrap();

    // Get updated metrics
    let response = client
        .get(format!("{}/metrics", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();
    let updated_metrics = response.text().unwrap();

    // Metrics should have changed
    assert_ne!(initial_metrics, updated_metrics);
}

// ========== Edge Case Tests ==========

#[test]
fn test_special_characters_in_key() {
    let special_keys = [
        "test/key/with/slashes",
        "test-key-with-dashes",
        "test_key_with_underscores",
        "test.key.with.dots",
        "test:key:with:colons",
    ];

    for key in &special_keys {
        let data = format!("data for {}", key);
        make_auth_request("PUT", key, Some(data.as_bytes())).unwrap();
        let (result, _) = make_auth_request("GET", key, None).unwrap();
        assert_eq!(result, data);
        make_auth_request("DELETE", key, None).unwrap();
    }
}

#[test]
fn test_unicode_in_key() {
    let unicode_keys = ["test-ключ", "test-键", "test-مفتاح"];

    for key in &unicode_keys {
        let data = format!("data for {}", key);
        make_auth_request("PUT", key, Some(data.as_bytes())).unwrap();
        let (result, _) = make_auth_request("GET", key, None).unwrap();
        assert_eq!(result, data);
        make_auth_request("DELETE", key, None).unwrap();
    }
}

#[test]
fn test_long_key() {
    let long_key = "a".repeat(1000);
    let data = b"data for long key";
    make_auth_request("PUT", &long_key, Some(data)).unwrap();
    let (result, _) = make_auth_request("GET", &long_key, None).unwrap();
    assert_eq!(result, "data for long key");
    make_auth_request("DELETE", &long_key, None).unwrap();
}

#[test]
fn test_binary_data() {
    let binary_data: Vec<u8> = (0..256).map(|i| i as u8).collect();
    make_auth_request("PUT", "/binary_test", Some(&binary_data)).unwrap();

    let (base_url, token) = get_config();
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("{}/binary_test", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .unwrap();

    let retrieved = response.bytes().unwrap();
    assert_eq!(retrieved.as_ref(), binary_data.as_slice());

    // Cleanup
    make_auth_request("DELETE", "/binary_test", None).unwrap();
}

#[test]
fn test_root_path_returns_404() {
    let (_, status) = make_auth_request("GET", "/", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn test_empty_key_returns_error() {
    // This tests PUT with empty path (just "/")
    let (_, status) = make_auth_request("PUT", "/", Some(b"data")).unwrap();
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

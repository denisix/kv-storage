//! TLS integration tests for kv-storage server
//!
//! These tests generate a self-signed certificate using rcgen, start the server
//! with TLS enabled, and verify that clients can connect over HTTPS.
//!
//! Run with: cargo test --test tls_integration_test

use std::io::BufReader;
use std::process::{Command, Child, Stdio};
use std::time::Duration;
use std::net::TcpStream;

/// Generate a self-signed certificate and key using rcgen.
/// Writes PEM files and returns (cert_path, key_path, sha256_fingerprint_hex).
fn generate_self_signed_cert(dir: &std::path::Path) -> (String, String, String) {
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");

    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    params.distinguished_name.push(rcgen::DnType::CommonName, rcgen::DnValue::Utf8String("localhost".into()));

    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    std::fs::write(&cert_path, &cert_pem).expect("Failed to write cert");
    std::fs::write(&key_path, &key_pem).expect("Failed to write key");

    // Compute SHA-256 fingerprint of the DER-encoded certificate
    let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(cert_pem.as_bytes()))
        .collect::<Result<Vec<_>, _>>()
        .expect("Failed to parse cert PEM");
    assert!(!certs.is_empty(), "No certificates found in PEM");

    let digest = ring::digest::digest(&ring::digest::SHA256, certs[0].as_ref());
    let fingerprint = hex::encode(digest.as_ref());

    (
        cert_path.to_str().unwrap().to_string(),
        key_path.to_str().unwrap().to_string(),
        fingerprint,
    )
}

/// Start the kv-storage server with TLS configuration
fn start_tls_server(cert_path: &str, key_path: &str, port: u16, db_path: &str) -> Child {
    let binary = env!("CARGO_BIN_EXE_kv-storage");

    Command::new(binary)
        .env("TOKEN", "test-token")
        .env("SSL_CERT", cert_path)
        .env("SSL_KEY", key_path)
        .env("BIND_ADDR", format!("127.0.0.1:{}", port))
        .env("DB_PATH", db_path)
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start kv-storage server")
}

/// Wait for the server to accept TCP connections.
/// If the child process exits early (error), print its stderr and return false.
fn wait_for_server(child: &mut Child, port: u16, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        // Check if the server process has exited (crashed)
        if let Ok(Some(status)) = child.try_wait() {
            eprintln!("Server process exited early with status: {}", status);
            if let Some(ref mut stderr) = child.stderr {
                use std::io::Read;
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                eprintln!("Server stderr: {}", buf);
            }
            return false;
        }
        if TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Make an HTTPS request using reqwest with TLS (skip verification for self-signed)
fn make_tls_request(
    method: &str,
    url: &str,
    token: &str,
    body: Option<&[u8]>,
) -> Result<(String, reqwest::StatusCode), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .use_rustls_tls()
        .build()?;

    let mut request = match method {
        "GET" => client.get(url),
        "PUT" => client.put(url),
        "DELETE" => client.delete(url),
        "HEAD" => client.head(url),
        "POST" => client.post(url),
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

struct TestServer {
    child: Child,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn test_tls_server_accepts_https_connections() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, _fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13443;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    // PUT a value over HTTPS
    let url = format!("https://127.0.0.1:{}/tls_test_key", port);
    let (_, status) = make_tls_request("PUT", &url, "test-token", Some(b"hello TLS"))
        .expect("PUT request failed");
    assert!(
        status == reqwest::StatusCode::CREATED || status == reqwest::StatusCode::OK,
        "Expected 201 or 200, got {}",
        status
    );

    // GET the value back
    let (body, status) = make_tls_request("GET", &url, "test-token", None)
        .expect("GET request failed");
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(body, "hello TLS");

    // DELETE
    let (_, status) = make_tls_request("DELETE", &url, "test-token", None)
        .expect("DELETE request failed");
    assert_eq!(status, reqwest::StatusCode::NO_CONTENT);

    // Verify deleted
    let (_, status) = make_tls_request("GET", &url, "test-token", None)
        .expect("GET after delete failed");
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
}

#[test]
fn test_tls_server_auth_required() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, _fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13444;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    // Request with wrong token
    let url = format!("https://127.0.0.1:{}/auth_test", port);
    let (_, status) = make_tls_request("GET", &url, "wrong-token", None)
        .expect("Request failed");
    assert_eq!(status, reqwest::StatusCode::UNAUTHORIZED);
}

#[test]
fn test_tls_server_certificate_fingerprint() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13445;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    // Verify we got a valid 64-char hex fingerprint
    assert_eq!(fingerprint.len(), 64, "SHA-256 fingerprint should be 64 hex chars");
    assert!(
        fingerprint.chars().all(|c| c.is_ascii_hexdigit()),
        "Fingerprint should contain only hex chars: {}",
        fingerprint
    );

    // Connect with TLS and verify the server certificate fingerprint
    let tls_config = {
        let cert_pem = std::fs::read(&cert_path).expect("Failed to read cert");
        let certs: Vec<_> = rustls_pemfile::certs(&mut BufReader::new(cert_pem.as_slice()))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        let mut roots = rustls::RootCertStore::empty();
        roots.add(certs[0].clone()).unwrap();

        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        std::sync::Arc::new(
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    };

    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut conn = rustls::ClientConnection::new(tls_config, server_name).unwrap();
    let mut sock = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    sock.set_write_timeout(Some(Duration::from_secs(5))).unwrap();

    // Complete TLS handshake by writing HTTP/2 preface
    let mut stream = rustls::Stream::new(&mut conn, &mut sock);
    use std::io::Write;
    let _ = stream.write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n");
    let _ = stream.flush();

    // Get peer certificates and verify fingerprint
    let peer_certs = conn.peer_certificates();
    assert!(peer_certs.is_some(), "Should have peer certificates");
    let certs = peer_certs.unwrap();
    assert!(!certs.is_empty(), "Should have at least one certificate");

    let actual_digest = ring::digest::digest(&ring::digest::SHA256, certs[0].as_ref());
    let actual_fingerprint = hex::encode(actual_digest.as_ref());
    assert_eq!(
        actual_fingerprint, fingerprint,
        "Server certificate fingerprint should match"
    );
}

#[test]
fn test_tls_server_batch_operations() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, _fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13446;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    // Batch operations over TLS
    let url = format!("https://127.0.0.1:{}/batch", port);
    let batch_body = serde_json::json!([
        {"op": "put", "key": "tls_batch_1", "value": "value1"},
        {"op": "put", "key": "tls_batch_2", "value": "value2"},
        {"op": "get", "key": "tls_batch_1"}
    ]);

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .use_rustls_tls()
        .build()
        .unwrap();

    let response = client
        .post(&url)
        .header("Authorization", "Bearer test-token")
        .header("Content-Type", "application/json")
        .body(batch_body.to_string())
        .send()
        .expect("Batch request failed");

    assert_eq!(response.status(), reqwest::StatusCode::OK);

    let body: serde_json::Value = response.json().unwrap();
    let results = body["results"].as_array().unwrap();
    assert_eq!(results.len(), 3, "Should have 3 batch results");

    // Cleanup
    let url1 = format!("https://127.0.0.1:{}/tls_batch_1", port);
    let url2 = format!("https://127.0.0.1:{}/tls_batch_2", port);
    let _ = make_tls_request("DELETE", &url1, "test-token", None);
    let _ = make_tls_request("DELETE", &url2, "test-token", None);
}

#[test]
fn test_tls_server_metrics_endpoint() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, _fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13447;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    let url = format!("https://127.0.0.1:{}/metrics", port);
    let (body, status) = make_tls_request("GET", &url, "test-token", None)
        .expect("Metrics request failed");

    assert_eq!(status, reqwest::StatusCode::OK);
    assert!(body.contains("kv_storage_"), "Metrics should contain kv_storage_ prefix");
}

#[test]
fn test_tls_server_head_request() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, _fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13448;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    // Create a key
    let url = format!("https://127.0.0.1:{}/tls_head_test", port);
    make_tls_request("PUT", &url, "test-token", Some(b"head test data"))
        .expect("PUT request failed");

    // HEAD request over HTTPS
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .danger_accept_invalid_certs(true)
        .use_rustls_tls()
        .build()
        .unwrap();

    let response = client
        .head(&url)
        .header("Authorization", "Bearer test-token")
        .send()
        .expect("HEAD request failed");

    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let content_length = response.headers().get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    assert_eq!(content_length, 14, "Content-Length should be 14");

    // Cleanup
    let _ = make_tls_request("DELETE", &url, "test-token", None);
}

#[test]
fn test_tls_server_list_keys() {
    let tmp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_dir = tmp_dir.path().join("db");
    std::fs::create_dir_all(&db_dir).unwrap();

    let (cert_path, key_path, _fingerprint) = generate_self_signed_cert(tmp_dir.path());
    let port = 13449;

    let child = start_tls_server(&cert_path, &key_path, port, db_dir.to_str().unwrap());
    let mut _server = TestServer { child };

    assert!(wait_for_server(&mut _server.child, port, Duration::from_secs(10)), "Server failed to start");

    // Create some keys
    let url1 = format!("https://127.0.0.1:{}/tls_list_1", port);
    let url2 = format!("https://127.0.0.1:{}/tls_list_2", port);
    make_tls_request("PUT", &url1, "test-token", Some(b"data1")).unwrap();
    make_tls_request("PUT", &url2, "test-token", Some(b"data2")).unwrap();

    // List keys
    let keys_url = format!("https://127.0.0.1:{}/keys?limit=100", port);
    let (body, status) = make_tls_request("GET", &keys_url, "test-token", None).unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);

    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    let total = parsed["total"].as_u64().unwrap();
    assert!(total >= 2, "Should have at least 2 keys, got {}", total);

    // Cleanup
    let _ = make_tls_request("DELETE", &url1, "test-token", None);
    let _ = make_tls_request("DELETE", &url2, "test-token", None);
}

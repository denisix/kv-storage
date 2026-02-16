use tokio::net::TcpListener;
use std::sync::Arc;
use hyper::server::conn::http2;
use hyper_util::rt::{TokioIo, TokioExecutor};
use tracing::{info, error};

use kv_storage::Config;
use kv_storage::storage::{DbWrapper, StorageDb};
use kv_storage::server::Handler;
use kv_storage::util::{compression::Compressor, metrics::Metrics};

fn build_http2_builder() -> http2::Builder<TokioExecutor> {
    let mut builder = http2::Builder::new(TokioExecutor::new());
    builder
        .max_frame_size(256 * 1024)
        .max_concurrent_streams(500)
        .initial_stream_window_size(1024 * 1024)
        .max_send_buf_size(2 * 1024 * 1024);
    builder
}

fn load_tls_config(cert_path: &str, key_path: &str) -> Result<Arc<tokio_rustls::rustls::ServerConfig>, Box<dyn std::error::Error>> {
    use std::fs::File;
    use std::io::BufReader;
    use tokio_rustls::rustls;

    // Ensure the ring crypto provider is installed
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok(); // Ignore error if already installed

    let cert_file = File::open(cert_path)
        .map_err(|e| format!("Failed to open SSL_CERT '{}': {}", cert_path, e))?;
    let key_file = File::open(key_path)
        .map_err(|e| format!("Failed to open SSL_KEY '{}': {}", key_path, e))?;

    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut BufReader::new(cert_file))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to parse SSL_CERT: {}", e))?;

    if certs.is_empty() {
        return Err("SSL_CERT file contains no certificates".into());
    }

    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))
        .map_err(|e| format!("Failed to parse SSL_KEY: {}", e))?
        .ok_or("SSL_KEY file contains no private key")?;

    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS configuration error: {}", e))?;

    config.alpn_protocols = vec![b"h2".to_vec()];

    Ok(Arc::new(config))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // Load configuration
    let config = Config::from_env()
        .map_err(|e| format!("Configuration error: {}", e))?;

    info!("Starting KV Storage Server");
    info!("Database path: {}", config.db_path);
    info!("Binding to: {}", config.bind_addr);
    info!("Compression level: {}", config.compression_level);
    if let Some(cache) = config.cache_capacity_bytes {
        info!("Cache capacity: {} bytes", cache);
    }
    if let Some(flush) = config.flush_interval_ms {
        info!("Flush interval: {} ms", flush);
    }

    // Load TLS config if SSL_CERT and SSL_KEY are set
    let tls_acceptor = match (&config.ssl_cert, &config.ssl_key) {
        (Some(cert), Some(key)) => {
            let tls_config = load_tls_config(cert, key)?;
            info!("TLS enabled (HTTP/2 over SSL)");
            Some(tokio_rustls::TlsAcceptor::from(tls_config))
        }
        _ => {
            info!("TLS disabled (HTTP/2 cleartext / h2c)");
            None
        }
    };

    // Open database with config
    let db: StorageDb = Arc::new(DbWrapper::open_with_config(
        &config.db_path,
        config.cache_capacity_bytes,
        config.flush_interval_ms,
    )?);
    info!("Database opened successfully");

    // Initialize compressor and metrics
    let compressor = Arc::new(Compressor::new(config.compression_level));
    let metrics = Arc::new(Metrics::new());

    // Create handler
    let handler = Handler::new(
        db.clone(),
        config.auth_token.clone(),
        compressor,
        metrics.clone(),
    );

    // Create TCP listener
    let listener = TcpListener::bind(&config.bind_addr).await?;
    info!("Server listening on {}", config.bind_addr);

    // Set up graceful shutdown
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    // Spawn signal handler
    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to setup SIGTERM handler");
        let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .expect("Failed to setup SIGINT handler");

        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, initiating graceful shutdown");
            }
        }

        let _ = shutdown_tx.send(true);
    });

    // Accept connections
    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        info!("New connection from {}", addr);

                        let handler = handler.clone();
                        let tls_acceptor = tls_acceptor.clone();

                        tokio::spawn(async move {
                            let builder = build_http2_builder();

                            if let Some(acceptor) = tls_acceptor {
                                // TLS mode: perform TLS handshake then serve HTTP/2
                                match acceptor.accept(stream).await {
                                    Ok(tls_stream) => {
                                        let io = TokioIo::new(tls_stream);
                                        match builder.serve_connection(io, handler).await {
                                            Ok(_) => info!("TLS connection from {} closed", addr),
                                            Err(e) => error!("TLS connection from {} error: {}", addr, e),
                                        }
                                    }
                                    Err(e) => {
                                        error!("TLS handshake failed for {}: {}", addr, e);
                                    }
                                }
                            } else {
                                // Plaintext h2c mode
                                let io = TokioIo::new(stream);
                                match builder.serve_connection(io, handler).await {
                                    Ok(_) => info!("Connection from {} closed", addr),
                                    Err(e) => error!("Connection from {} error: {}", addr, e),
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                info!("Shutting down...");
                // Run flush in blocking task to avoid blocking async runtime
                let db_clone = db.clone();
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = db_clone.flush() {
                        error!("Database flush error: {}", e);
                    }
                }).await?;
                info!("Database flushed, shutdown complete");
                break;
            }
        }
    }

    Ok(())
}

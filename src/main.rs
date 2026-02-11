use tokio::net::TcpListener;
use std::sync::Arc;
use hyper::server::conn::http2;
use hyper_util::rt::{TokioIo, TokioExecutor};
use tracing::{info, error};

use kv_storage::Config;
use kv_storage::storage::{DbWrapper, StorageDb};
use kv_storage::server::Handler;
use kv_storage::util::{compression::Compressor, metrics::Metrics};

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
                        tokio::spawn(async move {
                            let io = TokioIo::new(stream);

                            // Configure HTTP/2 with optimized settings for high throughput
                            let mut builder = http2::Builder::new(TokioExecutor::new());
                            builder
                                .max_frame_size(256 * 1024)        // 256KB frames (h2 max is 2^24-1, but practical is lower)
                                .max_concurrent_streams(500)       // Increased concurrent streams
                                .initial_stream_window_size(1024 * 1024) // 1MB flow control window
                                .max_send_buf_size(2 * 1024 * 1024);  // 2MB send buffer

                            match builder.serve_connection(io, handler).await {
                                Ok(_) => info!("Connection from {} closed", addr),
                                Err(e) => error!("Connection from {} error: {}", addr, e),
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

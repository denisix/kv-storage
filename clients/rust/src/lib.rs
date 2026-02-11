//! A modern HTTP/2 client for the KV Storage server
//!
//! This library provides a high-level async client for interacting with the KV Storage server
//! using HTTP/2 with connection pooling and session management.
//!
//! # Features
//! - HTTP/2 with connection pooling
//! - Bearer token authentication
//! - Async/await API using tokio
//! - Binary and text data support
//! - Comprehensive error handling
//! - Batch operations support
//! - Built-in timeout support
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use kv_storage_client::Client;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), kv_storage_client::Error> {
//!     let client = Client::new("http://localhost:3000", "your-token")?;
//!
//!     // Store a value
//!     let result = client.put("my-key", b"Hello, World!").await?;
//!     println!("Stored with hash: {}", result.hash);
//!
//!     // Retrieve a value
//!     let value = client.get("my-key").await?;
//!     println!("Retrieved: {:?}", value);
//!
//!     Ok(())
//! }
//! ```

#![warn(missing_docs, rust_2018_idioms)]

pub mod client;
pub mod error;
pub mod types;

pub use client::{Client, ClientConfig};
pub use error::{Error, Result};
pub use types::*;

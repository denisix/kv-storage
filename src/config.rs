use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: String,
    pub auth_token: String,
    pub port: u16,           // HTTP/2 cleartext port (h2c)
    pub ssl_port: Option<u16>, // HTTPS port (h2) - only when SSL_CERT/SSL_KEY set
    pub bind_addr: String,   // Host to bind to (e.g., "0.0.0.0")
    pub compression_level: i32,
    pub cache_capacity_bytes: Option<usize>,
    pub flush_interval_ms: Option<u64>,
    pub ssl_cert: Option<String>,
    pub ssl_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        // Use static strings for defaults to avoid allocations
        let db_path = env::var("DB_PATH").unwrap_or_else(|_| "./kv_db".to_string());
        let auth_token = env::var("TOKEN")
            .map_err(|_| "TOKEN environment variable must be set".to_string())?;

        // Parse bind address (host only, e.g., "0.0.0.0")
        // Support both BIND_ADDR (full addr:port) and HOST (just host)
        let bind_addr = if let Ok(full) = env::var("BIND_ADDR") {
            // Extract host from "host:port" format
            full.split(':').next().unwrap_or("0.0.0.0").to_string()
        } else {
            env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string())
        };

        // Parse HTTP port (h2c cleartext)
        // Priority: PORT > BIND_ADDR port > default 3000
        let port = env::var("PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| {
                env::var("BIND_ADDR")
                    .ok()
                    .and_then(|addr| addr.split(':').nth(1)?.parse().ok())
                    .unwrap_or(3000)
            });

        // SSL certificate and key paths (both must be set to enable TLS)
        let ssl_cert = env::var("SSL_CERT").ok();
        let ssl_key = env::var("SSL_KEY").ok();

        if ssl_cert.is_some() != ssl_key.is_some() {
            return Err("Both SSL_CERT and SSL_KEY must be set to enable TLS".to_string());
        }

        // Parse SSL port (HTTPS) - only used when SSL is configured
        let ssl_port = if ssl_cert.is_some() {
            Some(env::var("SSL_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3443))
        } else {
            None
        };

        let compression_level = env::var("COMPRESSION_LEVEL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        // Parse cache capacity (supports: 256M, 1G, 512000000, etc.)
        let cache_capacity_bytes = env::var("KV_CACHE_CAPACITY")
            .ok()
            .and_then(|s| parse_size(&s));

        // Parse flush interval (in milliseconds, default: 1000)
        let flush_interval_ms = Some(
            env::var("KV_FLUSH_INTERVAL_MS")
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(1000)
        );

        Ok(Config {
            db_path,
            auth_token,
            port,
            ssl_port,
            bind_addr,
            compression_level,
            cache_capacity_bytes,
            flush_interval_ms,
            ssl_cert,
            ssl_key,
        })
    }
}

/// Parse size string to bytes (supports: 256M, 1G, 512000000, etc.)
pub fn parse_size(s: &str) -> Option<usize> {
    let s = s.trim().to_uppercase();
    let (num_str, suffix): (&str, usize) = if s.ends_with('K') {
        (&s[..s.len()-1], 1024)
    } else if s.ends_with('M') {
        (&s[..s.len()-1], 1024 * 1024)
    } else if s.ends_with('G') {
        (&s[..s.len()-1], 1024 * 1024 * 1024)
    } else {
        (s.as_str(), 1)
    };

    num_str.parse::<usize>().ok().map(|n| n.saturating_mul(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_parse_size() {
        // Test basic sizes
        assert_eq!(parse_size("256"), Some(256));
        assert_eq!(parse_size("1024"), Some(1024));

        // Test suffixes
        assert_eq!(parse_size("1K"), Some(1024));
        assert_eq!(parse_size("1M"), Some(1024 * 1024));
        assert_eq!(parse_size("1G"), Some(1024 * 1024 * 1024));

        // Test uppercase
        assert_eq!(parse_size("1k"), Some(1024));
        assert_eq!(parse_size("1m"), Some(1024 * 1024));
        assert_eq!(parse_size("1g"), Some(1024 * 1024 * 1024));

        // Test with spaces
        assert_eq!(parse_size(" 512M "), Some(512 * 1024 * 1024));

        // Test invalid inputs
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("invalid"), None);
        assert_eq!(parse_size("1.5M"), None);
    }

    #[test]
    #[serial]
    fn test_config_from_env_default() {
        // Clean up all env vars first
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("PORT");
        env::remove_var("BIND_ADDR");
        env::remove_var("HOST");
        // Set required env vars only
        env::set_var("TOKEN", "test-token");

        let config = Config::from_env().unwrap();

        assert_eq!(config.db_path, "./kv_db");
        assert_eq!(config.auth_token, "test-token");
        assert_eq!(config.bind_addr, "0.0.0.0");
        assert_eq!(config.port, 3000);
        assert!(config.ssl_port.is_none());
        assert_eq!(config.compression_level, 1);
        assert!(config.cache_capacity_bytes.is_none());
        assert_eq!(config.flush_interval_ms, Some(1000));
    }

    #[test]
    #[serial]
    fn test_config_cache_capacity() {
        // Clean up first
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");

        env::set_var("KV_CACHE_CAPACITY", "256M");
        let config = Config::from_env().unwrap();
        assert_eq!(config.cache_capacity_bytes, Some(256 * 1024 * 1024));

        env::set_var("KV_CACHE_CAPACITY", "1G");
        let config = Config::from_env().unwrap();
        assert_eq!(config.cache_capacity_bytes, Some(1024 * 1024 * 1024));

        env::set_var("KV_CACHE_CAPACITY", "512000000");
        let config = Config::from_env().unwrap();
        assert_eq!(config.cache_capacity_bytes, Some(512000000));

        // Clean up
        env::remove_var("KV_CACHE_CAPACITY");
    }

    #[test]
    #[serial]
    fn test_config_flush_interval() {
        // Clean up first
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::set_var("TOKEN", "test-token");

        env::set_var("KV_FLUSH_INTERVAL_MS", "5000");
        let config = Config::from_env().unwrap();
        assert_eq!(config.flush_interval_ms, Some(5000));

        env::set_var("KV_FLUSH_INTERVAL_MS", "100");
        let config = Config::from_env().unwrap();
        assert_eq!(config.flush_interval_ms, Some(100));

        // Clean up
        env::remove_var("KV_FLUSH_INTERVAL_MS");
    }

    #[test]
    #[serial]
    fn test_config_compression_level() {
        // Clean up first
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");

        env::set_var("COMPRESSION_LEVEL", "3");
        let config = Config::from_env().unwrap();
        assert_eq!(config.compression_level, 3);

        env::set_var("COMPRESSION_LEVEL", "10");
        let config = Config::from_env().unwrap();
        assert_eq!(config.compression_level, 10);

        // Clean up
        env::remove_var("COMPRESSION_LEVEL");
    }

    #[test]
    #[serial]
    fn test_config_missing_token() {
        // Clean up all optional vars first
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("TOKEN");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("TOKEN"));
    }

    #[test]
    #[serial]
    fn test_config_invalid_compression() {
        // Clean up first
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");

        env::set_var("COMPRESSION_LEVEL", "invalid");

        let config = Config::from_env().unwrap();
        // Should default to 1 for invalid compression level
        assert_eq!(config.compression_level, 1);

        // Clean up
        env::remove_var("COMPRESSION_LEVEL");
    }

    #[test]
    #[serial]
    fn test_config_ssl_neither_set() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("SSL_CERT");
        env::remove_var("SSL_KEY");
        env::set_var("TOKEN", "test-token");

        let config = Config::from_env().unwrap();
        assert!(config.ssl_cert.is_none());
        assert!(config.ssl_key.is_none());
        assert!(config.ssl_port.is_none());
    }

    #[test]
    #[serial]
    fn test_config_ssl_both_set() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("SSL_PORT");
        env::set_var("TOKEN", "test-token");
        env::set_var("SSL_CERT", "/path/to/cert.pem");
        env::set_var("SSL_KEY", "/path/to/key.pem");

        let config = Config::from_env().unwrap();
        assert_eq!(config.ssl_cert, Some("/path/to/cert.pem".to_string()));
        assert_eq!(config.ssl_key, Some("/path/to/key.pem".to_string()));
        assert_eq!(config.ssl_port, Some(3443)); // Default SSL port

        // Clean up
        env::remove_var("SSL_CERT");
        env::remove_var("SSL_KEY");
    }

    #[test]
    #[serial]
    fn test_config_ssl_port_custom() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");
        env::set_var("SSL_CERT", "/path/to/cert.pem");
        env::set_var("SSL_KEY", "/path/to/key.pem");
        env::set_var("SSL_PORT", "8443");

        let config = Config::from_env().unwrap();
        assert_eq!(config.ssl_port, Some(8443));

        // Clean up
        env::remove_var("SSL_CERT");
        env::remove_var("SSL_KEY");
        env::remove_var("SSL_PORT");
    }

    #[test]
    #[serial]
    fn test_config_ssl_only_cert_set() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("SSL_KEY");
        env::set_var("TOKEN", "test-token");
        env::set_var("SSL_CERT", "/path/to/cert.pem");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SSL_CERT"));

        // Clean up
        env::remove_var("SSL_CERT");
    }

    #[test]
    #[serial]
    fn test_config_ssl_only_key_set() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("SSL_CERT");
        env::set_var("TOKEN", "test-token");
        env::set_var("SSL_KEY", "/path/to/key.pem");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("SSL_CERT"));

        // Clean up
        env::remove_var("SSL_KEY");
    }

    // ===== PORT and HOST tests =====

    #[test]
    #[serial]
    fn test_config_port_default() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("PORT");
        env::remove_var("BIND_ADDR");
        env::remove_var("HOST");
        env::set_var("TOKEN", "test-token");

        let config = Config::from_env().unwrap();
        assert_eq!(config.port, 3000);
    }

    #[test]
    #[serial]
    fn test_config_port_custom() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("BIND_ADDR");
        env::set_var("TOKEN", "test-token");
        env::set_var("PORT", "8080");

        let config = Config::from_env().unwrap();
        assert_eq!(config.port, 8080);

        // Clean up
        env::remove_var("PORT");
    }

    #[test]
    #[serial]
    fn test_config_bind_addr_extracts_port() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("PORT");
        env::set_var("TOKEN", "test-token");
        env::set_var("BIND_ADDR", "127.0.0.1:5000");

        let config = Config::from_env().unwrap();
        assert_eq!(config.bind_addr, "127.0.0.1");
        assert_eq!(config.port, 5000);

        // Clean up
        env::remove_var("BIND_ADDR");
    }

    #[test]
    #[serial]
    fn test_config_host_env() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::remove_var("PORT");
        env::remove_var("BIND_ADDR");
        env::set_var("TOKEN", "test-token");
        env::set_var("HOST", "192.168.1.1");

        let config = Config::from_env().unwrap();
        assert_eq!(config.bind_addr, "192.168.1.1");
        assert_eq!(config.port, 3000); // Default

        // Clean up
        env::remove_var("HOST");
    }

    #[test]
    #[serial]
    fn test_config_port_overrides_bind_addr_port() {
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("KV_CACHE_CAPACITY");
        env::remove_var("KV_FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");
        env::set_var("BIND_ADDR", "0.0.0.0:4000");
        env::set_var("PORT", "9000");

        let config = Config::from_env().unwrap();
        assert_eq!(config.bind_addr, "0.0.0.0");
        assert_eq!(config.port, 9000); // PORT takes priority

        // Clean up
        env::remove_var("BIND_ADDR");
        env::remove_var("PORT");
    }
}

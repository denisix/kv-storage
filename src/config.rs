use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: String,
    pub auth_token: String,
    pub bind_addr: String,
    pub compression_level: i32,
    pub cache_capacity_bytes: Option<usize>,
    pub flush_interval_ms: Option<u64>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        // Use static strings for defaults to avoid allocations
        let db_path = env::var("DB_PATH").unwrap_or_else(|_| "./kv_db".to_string());
        let auth_token = env::var("TOKEN")
            .map_err(|_| "TOKEN environment variable must be set".to_string())?;
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let compression_level = env::var("COMPRESSION_LEVEL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);

        // Parse cache capacity (supports: 256M, 1G, 512000000, etc.)
        let cache_capacity_bytes = env::var("CACHE_CAPACITY")
            .ok()
            .and_then(|s| parse_size(&s));

        // Parse flush interval (in milliseconds)
        let flush_interval_ms = env::var("FLUSH_INTERVAL_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok());

        Ok(Config {
            db_path,
            auth_token,
            bind_addr,
            compression_level,
            cache_capacity_bytes,
            flush_interval_ms,
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
        env::remove_var("CACHE_CAPACITY");
        env::remove_var("FLUSH_INTERVAL_MS");
        // Set required env vars only
        env::set_var("TOKEN", "test-token");

        let config = Config::from_env().unwrap();

        assert_eq!(config.db_path, "./kv_db");
        assert_eq!(config.auth_token, "test-token");
        assert_eq!(config.bind_addr, "0.0.0.0:3000");
        assert_eq!(config.compression_level, 1);
        assert!(config.cache_capacity_bytes.is_none());
        assert!(config.flush_interval_ms.is_none());
    }

    #[test]
    #[serial]
    fn test_config_cache_capacity() {
        // Clean up first
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");

        env::set_var("CACHE_CAPACITY", "256M");
        let config = Config::from_env().unwrap();
        assert_eq!(config.cache_capacity_bytes, Some(256 * 1024 * 1024));

        env::set_var("CACHE_CAPACITY", "1G");
        let config = Config::from_env().unwrap();
        assert_eq!(config.cache_capacity_bytes, Some(1024 * 1024 * 1024));

        env::set_var("CACHE_CAPACITY", "512000000");
        let config = Config::from_env().unwrap();
        assert_eq!(config.cache_capacity_bytes, Some(512000000));

        // Clean up
        env::remove_var("CACHE_CAPACITY");
    }

    #[test]
    #[serial]
    fn test_config_flush_interval() {
        // Clean up first
        env::remove_var("COMPRESSION_LEVEL");
        env::remove_var("CACHE_CAPACITY");
        env::set_var("TOKEN", "test-token");

        env::set_var("FLUSH_INTERVAL_MS", "5000");
        let config = Config::from_env().unwrap();
        assert_eq!(config.flush_interval_ms, Some(5000));

        env::set_var("FLUSH_INTERVAL_MS", "100");
        let config = Config::from_env().unwrap();
        assert_eq!(config.flush_interval_ms, Some(100));

        // Clean up
        env::remove_var("FLUSH_INTERVAL_MS");
    }

    #[test]
    #[serial]
    fn test_config_compression_level() {
        // Clean up first
        env::remove_var("CACHE_CAPACITY");
        env::remove_var("FLUSH_INTERVAL_MS");
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
        env::remove_var("CACHE_CAPACITY");
        env::remove_var("FLUSH_INTERVAL_MS");
        env::remove_var("TOKEN");

        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("TOKEN"));
    }

    #[test]
    #[serial]
    fn test_config_invalid_compression() {
        // Clean up first
        env::remove_var("CACHE_CAPACITY");
        env::remove_var("FLUSH_INTERVAL_MS");
        env::set_var("TOKEN", "test-token");

        env::set_var("COMPRESSION_LEVEL", "invalid");

        let config = Config::from_env().unwrap();
        // Should default to 1 for invalid compression level
        assert_eq!(config.compression_level, 1);

        // Clean up
        env::remove_var("COMPRESSION_LEVEL");
    }
}

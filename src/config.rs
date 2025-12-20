use std::env;

/// Application configuration loaded from environment variables
#[derive(Clone, Debug)]
pub struct Config {
    /// Database connection URL
    /// Format: postgresql://USERNAME:PASSWORD@HOST:PORT/DATABASE_NAME
    pub database_url: String,

    /// Maximum payload size for all requests (in bytes)
    /// Default: 10MB (10 * 1024 * 1024)
    pub max_payload_size: usize,
}

impl Config {
    /// Load configuration from environment variables
    ///
    /// Required environment variables:
    /// - DATABASE_URL: PostgreSQL connection string
    ///
    /// Optional environment variables:
    /// - MAX_PAYLOAD_SIZE: Maximum request payload size in bytes (default: 10485760 = 10MB)
    pub fn from_env() -> Result<Self, String> {
        // Load .env file if it exists
        dotenv::dotenv().ok();

        let database_url = env::var("DATABASE_URL")
            .map_err(|_| "DATABASE_URL must be set in .env file or environment".to_string())?;

        // Parse MAX_PAYLOAD_SIZE with default fallback
        let max_payload_size = env::var("MAX_PAYLOAD_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10 * 1024 * 1024); // Default: 10MB

        Ok(Config {
            database_url,
            max_payload_size,
        })
    }
}

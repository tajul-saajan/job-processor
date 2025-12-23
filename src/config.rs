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

    /// Maximum number of database connections in the pool
    /// Default: 15
    pub max_db_connections: u32,

    /// Maximum number of jobs processing concurrently (semaphore permits)
    /// Default: 5
    pub max_concurrent_jobs: usize,

    /// Number of worker loops acquiring jobs from the queue
    /// Default: 3
    pub num_workers: u32,

    /// Directory for log files (daily rotation, separated by level)
    /// Default: "logs"
    pub log_dir: String,
}

impl Config {
    /// Load configuration from environment variables
    ///
    /// Required environment variables:
    /// - DATABASE_URL: PostgreSQL connection string
    ///
    /// Optional environment variables:
    /// - MAX_PAYLOAD_SIZE: Maximum request payload size in bytes (default: 10485760 = 10MB)
    /// - MAX_DB_CONNECTIONS: Maximum database connections in pool (default: 15)
    /// - MAX_CONCURRENT_JOBS: Maximum concurrent jobs processing (semaphore permits) (default: 5)
    /// - NUM_WORKERS: Number of worker loops acquiring jobs (default: 3)
    /// - LOG_DIR: Directory for log files with daily rotation (default: "logs")
    ///
    /// Note: Ensure MAX_DB_CONNECTIONS >= NUM_WORKERS + MAX_CONCURRENT_JOBS + API_BUFFER
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

        // Parse MAX_DB_CONNECTIONS with default fallback
        let max_db_connections = env::var("MAX_DB_CONNECTIONS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(15); // Default: 15 connections

        // Parse MAX_CONCURRENT_JOBS with default fallback
        let max_concurrent_jobs = env::var("MAX_CONCURRENT_JOBS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5); // Default: 5 concurrent jobs

        // Parse NUM_WORKERS with default fallback
        let num_workers = env::var("NUM_WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3); // Default: 3 workers

        // Parse LOG_DIR with default fallback
        let log_dir = env::var("LOG_DIR")
            .unwrap_or_else(|_| "logs".to_string()); // Default: logs directory

        Ok(Config {
            database_url,
            max_payload_size,
            max_db_connections,
            max_concurrent_jobs,
            num_workers,
            log_dir,
        })
    }
}

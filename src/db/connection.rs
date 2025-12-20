use sqlx::{Error, Pool, Postgres, postgres::PgPoolOptions};

/// Create a PostgreSQL connection pool
///
/// # Parameters
/// - `database_url`: PostgreSQL connection string
///   Format: postgresql://USERNAME:PASSWORD@HOST:PORT/DATABASE_NAME
///
/// # Returns
/// A connection pool with max 5 connections
pub async fn get_connection(database_url: &str) -> Result<Pool<Postgres>, Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

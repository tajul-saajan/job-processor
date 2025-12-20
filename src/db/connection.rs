use sqlx::{Error, Pool, Postgres, postgres::PgPoolOptions};

/// Create a PostgreSQL connection pool
///
/// # Parameters
/// - `database_url`: PostgreSQL connection string
///   Format: postgresql://USERNAME:PASSWORD@HOST:PORT/DATABASE_NAME
/// - `max_connections`: Maximum number of connections in the pool
///
/// # Returns
/// A connection pool with the specified max connections
pub async fn get_connection(database_url: &str, max_connections: u32) -> Result<Pool<Postgres>, Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

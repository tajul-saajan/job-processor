use sqlx::{Pool, Postgres};

/// Run all pending database migrations
///
/// This function embeds the SQL files from the migrations directory
/// and applies them to the database. It's safe to run multiple times
/// as sqlx tracks which migrations have already been applied.
pub async fn run_migrations(pool: &Pool<Postgres>) -> Result<(), sqlx::migrate::MigrateError> {
    println!("Running database migrations...");

    // sqlx::migrate!() macro embeds migrations at compile time
    // from the migrations/ directory
    sqlx::migrate!("./migrations")
        .run(pool)
        .await?;

    println!("Database migrations completed successfully");
    Ok(())
}

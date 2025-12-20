use sqlx::{Pool, Postgres, Row};
use tracing::{error, info, warn};

/// Run all pending database migrations
///
/// This function embeds the SQL files from the migrations directory
/// and applies them to the database. It's safe to run multiple times
/// as sqlx tracks which migrations have already been applied.
pub async fn run_migrations(pool: &Pool<Postgres>) -> Result<(), sqlx::migrate::MigrateError> {
    info!("Running database migrations...");

    // sqlx::migrate!() macro embeds migrations at compile time
    // from the migrations/ directory
    sqlx::migrate!("./migrations")
        .run(pool)
        .await?;

    info!("Database migrations completed successfully");
    Ok(())
}

/// Rollback the last N migrations
///
/// # Arguments
/// * `pool` - Database connection pool
/// * `steps` - Number of migrations to rollback (must be > 0)
///
/// # Returns
/// Result indicating success or failure
pub async fn rollback_migrations(
    pool: &Pool<Postgres>,
    steps: i64,
) -> Result<(), sqlx::migrate::MigrateError> {
    if steps <= 0 {
        warn!("Invalid rollback steps: {}. Must be greater than 0", steps);
        return Err(sqlx::migrate::MigrateError::VersionMissing(0));
    }

    info!("Rolling back {} migration(s)...", steps);

    for i in 0..steps {
        info!("Rolling back migration {} of {}", i + 1, steps);

        // Get the latest applied migration
        let latest = sqlx::query(
            "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 1"
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            error!("Failed to query migrations table: {:?}", e);
            sqlx::migrate::MigrateError::Execute(e.into())
        })?;

        match latest {
            Some(row) => {
                let version: i64 = row.try_get("version").map_err(|e| {
                    error!("Failed to get version: {:?}", e);
                    sqlx::migrate::MigrateError::Execute(e.into())
                })?;
                let description: String = row.try_get("description").map_err(|e| {
                    error!("Failed to get description: {:?}", e);
                    sqlx::migrate::MigrateError::Execute(e.into())
                })?;

                info!("Rolling back migration: {} ({})", version, description);

                // Read the down migration SQL file
                let down_file = format!("down_migrations/{}_{}.sql", version, description);
                let down_sql = std::fs::read_to_string(&down_file).map_err(|e| {
                    error!("Failed to read down migration file '{}': {:?}", down_file, e);
                    error!("Make sure down migration files exist in down_migrations/ directory");

                    // Return VersionMissing error
                    sqlx::migrate::MigrateError::VersionMissing(version)
                })?;

                // Execute the down migration SQL
                // Split by semicolon and execute each statement separately
                for statement in down_sql.split(';') {
                    // Remove comment lines and trim
                    let cleaned: String = statement
                        .lines()
                        .filter(|line| {
                            let trimmed_line = line.trim();
                            !trimmed_line.is_empty() && !trimmed_line.starts_with("--")
                        })
                        .collect::<Vec<&str>>()
                        .join("\n");

                    let trimmed = cleaned.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    info!("Executing down migration statement: {}", trimmed);

                    sqlx::query(trimmed)
                        .execute(pool)
                        .await
                        .map_err(|e| {
                            error!("Failed to execute down migration statement: {:?}", e);
                            error!("Statement: {}", trimmed);
                            sqlx::migrate::MigrateError::Execute(e.into())
                        })?;
                }

                // Remove the migration from the tracking table
                sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
                    .bind(version)
                    .execute(pool)
                    .await
                    .map_err(|e| {
                        error!("Failed to delete migration record: {:?}", e);
                        sqlx::migrate::MigrateError::Execute(e.into())
                    })?;

                info!("Successfully rolled back migration: {} ({})", version, description);
            }
            None => {
                info!("No more migrations to rollback");
                break;
            }
        }
    }

    info!("Successfully rolled back {} migration(s)", steps);
    Ok(())
}

/// Rollback all migrations to a fresh database state
///
/// This removes all migrations, returning the database to its initial state.
/// Be careful - this will drop all tables and data managed by migrations!
pub async fn rollback_all_migrations(pool: &Pool<Postgres>) -> Result<(), sqlx::migrate::MigrateError> {
    info!("Rolling back ALL migrations to fresh state...");
    warn!("This will remove all database schema changes from migrations!");

    // Get the count of applied migrations
    let applied = sqlx::query("SELECT COUNT(*) as count FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .map_err(|e| sqlx::migrate::MigrateError::Execute(e.into()))?;

    let count: i64 = applied.try_get("count")
        .map_err(|e| sqlx::migrate::MigrateError::Execute(e.into()))?;

    if count == 0 {
        info!("No migrations to rollback - database is already in fresh state");
        return Ok(());
    }

    info!("Found {} applied migration(s) to rollback", count);

    // Rollback all migrations one by one
    rollback_migrations(pool, count).await?;

    info!("Successfully rolled back all migrations - database is now in fresh state");
    Ok(())
}

/// Refresh database: rollback all migrations and re-apply them
///
/// This is useful for testing or resetting to a clean state with current schema.
pub async fn refresh_database(pool: &Pool<Postgres>) -> Result<(), sqlx::migrate::MigrateError> {
    info!("Refreshing database (rollback all + re-migrate)...");

    // Rollback all migrations
    rollback_all_migrations(pool).await?;

    // Re-apply all migrations
    run_migrations(pool).await?;

    info!("Database refresh completed successfully");
    Ok(())
}

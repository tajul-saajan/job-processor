use std::env;

use dotenv::dotenv;
use sqlx::{Error, Pool, Postgres, postgres::PgPoolOptions};

pub async fn get_connection() -> Result<Pool<Postgres>, Error> {
    let (host, port, db, user, password) = get_db_config();
    let url = format!("postgresql://{}:{}@{}:{}/{}", user, password, host,port, db);
    PgPoolOptions::new().max_connections(5).connect(&url).await
}

fn get_db_config() -> (String, String, String, String, String) {
    dotenv().ok();
    let db_host = env::var("DB_HOST").unwrap_or_else(|_| "localhost".into());
    let db_port = env::var("DB_PORT").unwrap_or_else(|_| "5432".into());
    let db_name = env::var("DB_NAME").unwrap_or_else(|_| "5432".into());
    let db_user = env::var("DB_USER").unwrap_or_else(|_| "5432".into());
    let db_password = env::var("DB_PASSWORD").unwrap_or_else(|_| "5432".into());

    (db_host, db_port, db_name, db_user, db_password)
}

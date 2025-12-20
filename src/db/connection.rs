use std::env;

use dotenv::dotenv;
use sqlx::{Error, Pool, Postgres, postgres::PgPoolOptions};

pub async fn get_connection() -> Result<Pool<Postgres>, Error> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in .env file");

    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
}

use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::FromRow;

/// Database representation of a job with all fields
#[derive(Debug, FromRow, Serialize)]
pub struct JobRow {
    pub id: i32,
    pub name: String,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

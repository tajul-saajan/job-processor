use sqlx::{Pool, Postgres};
use crate::api::job::handlers::Job;
use crate::db::models::JobRow;

/// Repository for Job database operations
pub struct JobRepository;

impl JobRepository {
    /// Create a new job in the database and return the full job record
    pub async fn create(
        pool: &Pool<Postgres>,
        job: &Job,
    ) -> Result<JobRow, sqlx::Error> {
        let status_str = format!("{:?}", job.status).to_lowercase();

        let row = sqlx::query_as!(
            JobRow,
            r#"
            INSERT INTO jobs (name, status)
            VALUES ($1, $2)
            RETURNING id, name, status, created_at, updated_at
            "#,
            job.name,
            status_str
        )
        .fetch_one(pool)
        .await?;

        Ok(row)
    }
}

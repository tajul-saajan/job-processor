use sqlx::{Pool, Postgres};
use tracing::debug;
use crate::api::job::Job;
use crate::db::models::JobRow;

/// Repository for Job database operations
pub struct JobRepository;

impl JobRepository {
    /// Create a new job in the database and return the full job record
    pub async fn create(
        pool: &Pool<Postgres>,
        job: &Job,
    ) -> Result<JobRow, sqlx::Error> {
        debug!("Creating job: name={}, status={:?}", job.name, job.status);

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

        debug!("Job created with id={}", row.id);
        Ok(row)
    }

    /// Bulk insert multiple jobs in a single transaction
    /// Returns the number of rows inserted
    pub async fn bulk_create(
        pool: &Pool<Postgres>,
        jobs: &[Job],
    ) -> Result<u64, sqlx::Error> {
        if jobs.is_empty() {
            debug!("Bulk create called with empty job list");
            return Ok(0);
        }

        debug!("Starting bulk insert of {} jobs", jobs.len());

        // Build dynamic SQL for bulk insert
        let mut query = String::from("INSERT INTO jobs (name, status) VALUES ");
        let mut values = Vec::new();

        for (i, job) in jobs.iter().enumerate() {
            let status_str = format!("{:?}", job.status).to_lowercase();

            if i > 0 {
                query.push_str(", ");
            }
            query.push_str(&format!("(${}, ${})", i * 2 + 1, i * 2 + 2));

            values.push(job.name.clone());
            values.push(status_str);
        }

        // Execute bulk insert
        let mut query_builder = sqlx::query(&query);
        for value in values {
            query_builder = query_builder.bind(value);
        }

        let result = query_builder.execute(pool).await?;
        let rows_affected = result.rows_affected();
        debug!("Bulk insert completed: {} rows inserted", rows_affected);

        Ok(rows_affected)
    }
}

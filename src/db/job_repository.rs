use sqlx::{Pool, Postgres, Row};
use tracing::{debug, info};
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

    /// Acquire the next available job with row-level locking
    ///
    /// This function safely acquires a job with status 'new' and updates it to 'processing'.
    /// Uses PostgreSQL's FOR UPDATE SKIP LOCKED to prevent race conditions between workers.
    ///
    /// # How it works
    /// - Selects one 'new' job (oldest first - FIFO)
    /// - Locks the row with FOR UPDATE SKIP LOCKED
    /// - If another worker already locked it, skips to next available job
    /// - Updates status to 'processing'
    /// - Returns the job
    ///
    /// # Returns
    /// - `Ok(Some(job))` - Successfully acquired a job
    /// - `Ok(None)` - No jobs available (all are processing/completed/failed)
    /// - `Err(e)` - Database error
    ///
    /// # Example
    /// ```rust
    /// match JobRepository::acquire_next_job(&pool).await {
    ///     Ok(Some(job)) => {
    ///         // Process the job...
    ///         println!("Acquired job: {}", job.id);
    ///     }
    ///     Ok(None) => {
    ///         println!("No jobs available");
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Error: {:?}", e);
    ///     }
    /// }
    /// ```
    pub async fn acquire_next_job(
        pool: &Pool<Postgres>,
    ) -> Result<Option<JobRow>, sqlx::Error> {
        debug!("Attempting to acquire next available job");

        // Start a transaction
        let mut tx = pool.begin().await?;

        // Select and lock one 'new' job (oldest first)
        // FOR UPDATE locks the row
        // SKIP LOCKED skips rows already locked by other workers
        let job_row = sqlx::query(
            r#"
            SELECT id, name, status, created_at, updated_at
            FROM jobs
            WHERE status = 'new'
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
            "#
        )
        .fetch_optional(&mut *tx)
        .await?;

        // If no job found, return None
        let job_row = match job_row {
            Some(row) => row,
            None => {
                debug!("No jobs available to acquire");
                tx.rollback().await?;
                return Ok(None);
            }
        };

        // Extract job ID
        let job_id: i32 = job_row.try_get("id")?;

        info!("Acquired job with id={}, updating status to 'processing'", job_id);

        // Update the job status to 'processing'
        let updated_job = sqlx::query_as!(
            JobRow,
            r#"
            UPDATE jobs
            SET status = 'processing'
            WHERE id = $1
            RETURNING id, name, status, created_at, updated_at
            "#,
            job_id
        )
        .fetch_one(&mut *tx)
        .await?;

        // Commit the transaction
        tx.commit().await?;

        info!("Successfully acquired and locked job: id={}, name={}", updated_job.id, updated_job.name);

        Ok(Some(updated_job))
    }
}

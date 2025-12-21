use actix_web::{HttpResponse, ResponseError};
use sqlx::{Pool, Postgres};
use std::fmt;
use tokio::time::{sleep, Duration};
use rand::Rng;
use tracing::{error, info, warn};
use validator::Validate;

use crate::api::validation::ErrorResponse;
use crate::db::job_repository::JobRepository;
use crate::db::models::JobRow;
use super::dto::{BulkJobResponse, JobError, JobResponse};
use super::models::Job;

/// Service-level errors
#[derive(Debug)]
pub enum ServiceError {
    /// Database operation failed
    DatabaseError(sqlx::Error),

    /// Validation failed
    ValidationError(String),

    /// Job not found
    NotFound(i32),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceError::DatabaseError(e) => write!(f, "Database error: {}", e),
            ServiceError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ServiceError::NotFound(id) => write!(f, "Job not found: {}", id),
        }
    }
}

impl std::error::Error for ServiceError {}

impl ResponseError for ServiceError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ServiceError::DatabaseError(e) => {
                error!("Database error: {}", e);
                HttpResponse::InternalServerError().json(ErrorResponse {
                    error: "Failed to process request".to_string(),
                    fields: serde_json::json!({"message": "Database error occurred"}),
                })
            }
            ServiceError::ValidationError(msg) => {
                warn!("Validation error: {}", msg);
                HttpResponse::BadRequest().json(ErrorResponse {
                    error: "Validation failed".to_string(),
                    fields: serde_json::json!({"message": msg}),
                })
            }
            ServiceError::NotFound(id) => {
                warn!("Job not found: {}", id);
                HttpResponse::NotFound().json(ErrorResponse {
                    error: "Not found".to_string(),
                    fields: serde_json::json!({"message": format!("Job with id {} not found", id)}),
                })
            }
        }
    }
}

/// Job service containing business logic
pub struct JobService {
    pool: Pool<Postgres>,
}

impl JobService {
    /// Create a new JobService instance
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Create a single job
    ///
    /// # Business Logic
    /// - Validates the job
    /// - Creates job in database
    /// - Logs the operation
    ///
    /// # Returns
    /// - `Ok(JobResponse)` - Job created successfully
    /// - `Err(ServiceError)` - Creation failed
    pub async fn create_job(&self, job: &Job) -> Result<JobResponse, ServiceError> {
        info!("Service: Creating job with name={}", job.name);

        // Create job in database
        let job_row = JobRepository::create(&self.pool, job)
            .await
            .map_err(ServiceError::DatabaseError)?;

        info!("Service: Job created successfully with id={}", job_row.id);

        Ok(JobResponse {
            message: "Job created successfully".to_string(),
            job: job_row,
        })
    }

    /// Bulk create jobs from uploaded file data
    ///
    /// # Business Logic
    /// - Validates each job individually
    /// - Collects validation errors with job names
    /// - Bulk inserts only valid jobs
    /// - Returns summary with created count and errors
    ///
    /// # Returns
    /// - `Ok(BulkJobResponse)` - Jobs processed (may have partial errors)
    /// - `Err(ServiceError)` - Complete failure
    pub async fn bulk_create_jobs(&self, jobs: Vec<Job>) -> Result<BulkJobResponse, ServiceError> {
        info!("Service: Processing bulk job creation for {} jobs", jobs.len());

        let mut valid_jobs = Vec::new();
        let mut errors = Vec::new();

        // Validate each job
        for job in jobs {
            if let Err(validation_errors) = job.validate() {
                let error_messages: Vec<String> = validation_errors
                    .field_errors()
                    .values()
                    .flat_map(|errors| {
                        errors.iter().map(|e| {
                            e.message
                                .as_ref()
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| "Validation error".to_string())
                        })
                    })
                    .collect();

                errors.push(JobError {
                    name: job.name.clone(),
                    errors: error_messages,
                });

                warn!("Service: Validation failed for job: {}", job.name);
            } else {
                valid_jobs.push(job);
            }
        }

        // Bulk insert valid jobs
        let created_count = if !valid_jobs.is_empty() {
            info!("Service: Bulk inserting {} valid jobs", valid_jobs.len());

            JobRepository::bulk_create(&self.pool, &valid_jobs)
                .await
                .map_err(ServiceError::DatabaseError)? as usize
        } else {
            warn!("Service: No valid jobs to insert");
            0
        };

        let error_count = errors.len();

        if error_count == 0 {
            info!("Service: Bulk job creation completed successfully: {} jobs created", created_count);
        } else {
            warn!("Service: Bulk job creation completed with {} validation errors", error_count);
        }

        Ok(BulkJobResponse {
            message: format!(
                "Bulk job creation completed. {} created, {} failed",
                created_count,
                error_count
            ),
            created: created_count,
            errors,
        })
    }

    /// Run worker that continuously processes jobs
    ///
    /// # Business Logic
    /// - Continuously fetches available jobs using acquire_next_job
    /// - Simulates processing with random delay (1-5 seconds)
    /// - Randomly determines success/failure (75-80% success rate)
    /// - Updates job status accordingly
    /// - Sleeps when no jobs are available
    ///
    /// # Arguments
    /// - `worker_id` - Identifier for this worker instance
    pub async fn run_worker(&self, worker_id: u32) {
        info!("Worker {} started", worker_id);

        loop {
            match JobRepository::acquire_next_job(&self.pool).await {
                Ok(Some(job)) => {
                    info!("Worker {} acquired job: id={}, name={}", worker_id, job.id, job.name);

                    // Random delay 1-5 seconds (simulate processing time)
                    let delay = rand::thread_rng().gen_range(1..=5);
                    info!("Worker {} processing job {} for {} seconds", worker_id, job.id, delay);
                    sleep(Duration::from_secs(delay)).await;

                    // Random success/failure (75-80% success rate)
                    let success_rate = rand::thread_rng().gen_range(0..100);
                    let status = if success_rate < 77 { "success" } else { "failed" };

                    // Update job status
                    match JobRepository::update_job_status(&self.pool, job.id, status).await {
                        Ok(_) => info!("Worker {} completed job {}: status={}", worker_id, job.id, status),
                        Err(e) => error!("Worker {} failed to update job {}: {:?}", worker_id, job.id, e),
                    }
                }
                Ok(None) => {
                    // No jobs available, sleep for a bit before checking again
                    info!("Worker {} found no jobs available, sleeping...", worker_id);
                    sleep(Duration::from_secs(5)).await;
                }
                Err(e) => {
                    error!("Worker {} encountered database error: {:?}", worker_id, e);
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}

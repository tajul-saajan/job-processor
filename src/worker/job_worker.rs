use sqlx::{Pool, Postgres};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tokio::sync::Semaphore;
use rand::Rng;
use tracing::{error, info};

use crate::db::job_repository::JobRepository;

/// Background worker for processing jobs
pub struct JobWorker {
    pool: Pool<Postgres>,
}

impl JobWorker {
    /// Create a new JobWorker instance
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    /// Run worker with semaphore-based bounded concurrency
    ///
    /// # Architecture
    /// - Continuously fetches available jobs using acquire_next_job
    /// - Acquires semaphore permit before spawning job processing task
    /// - Spawns concurrent tasks to process jobs (bounded by semaphore)
    /// - Each task simulates processing with random delay (1-5 seconds)
    /// - Randomly determines success/failure (75-80% success rate)
    /// - Updates job status accordingly
    /// - Sleeps when no jobs are available
    ///
    /// # Arguments
    /// - `worker_id` - Identifier for this worker instance
    /// - `semaphore` - Semaphore to control bounded concurrency
    ///
    /// # Concurrency Model
    /// - Worker acquires jobs from queue (fast, non-blocking)
    /// - Before spawning processing task, acquires semaphore permit
    /// - Multiple jobs can process in parallel, bounded by semaphore permits
    /// - Permit is released when job processing completes
    pub async fn run(&self, worker_id: u32, semaphore: Arc<Semaphore>) {
        info!("Worker {} started with semaphore-based concurrency", worker_id);

        loop {
            match JobRepository::acquire_next_job(&self.pool).await {
                Ok(Some(job)) => {
                    info!("Worker {} acquired job: id={}, name={}", worker_id, job.id, job.name);

                    // Acquire semaphore permit before spawning task
                    let permit = semaphore.clone().acquire_owned().await;
                    match permit {
                        Ok(permit) => {
                            info!("Worker {} got semaphore permit for job {}", worker_id, job.id);

                            let pool = self.pool.clone();
                            let job_id = job.id;
                            let job_name = job.name.clone();

                            // Spawn task to process job concurrently
                            tokio::spawn(async move {
                                // Random delay 1-5 seconds (simulate processing time)
                                let delay = rand::thread_rng().gen_range(1..=5);
                                info!("Processing job {} ({}) for {} seconds", job_id, job_name, delay);
                                sleep(Duration::from_secs(delay)).await;

                                // Random success/failure (75-80% success rate)
                                let success_rate = rand::thread_rng().gen_range(0..100);
                                let status = if success_rate < 77 { "success" } else { "failed" };

                                // Update job status
                                match JobRepository::update_job_status(&pool, job_id, status).await {
                                    Ok(_) => info!("Completed job {}: status={}", job_id, status),
                                    Err(e) => error!("Failed to update job {}: {:?}", job_id, e),
                                }

                                // Permit is automatically dropped here, releasing the semaphore
                                drop(permit);
                                info!("Released semaphore permit for job {}", job_id);
                            });
                        }
                        Err(e) => {
                            error!("Worker {} failed to acquire semaphore: {:?}", worker_id, e);
                        }
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

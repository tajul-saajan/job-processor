use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, ServiceConfig, scope},
};
use actix_web_validator::Json;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use validator::Validate;
use crate::db::{job_repository::JobRepository, models::JobRow};
use crate::api::validation::ErrorResponse;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    New,
    Processing,
    Success,
    Failed,
}

#[derive(Deserialize, Serialize, Debug, Validate)]
pub struct Job {
    #[validate(length(
        min = 3,
        max = 10,
        message = "Name must be between 3 and 10 characters"
    ))]
    pub name: String,
    pub status: JobStatus,
}

#[derive(Serialize)]
struct JobResponse {
    message: String,
    job: JobRow,
}

#[post("")]
async fn create_job(
    pool: Data<Pool<Postgres>>,
    job: Json<Job>,
) -> impl Responder {
    // Save job to database
    match JobRepository::create(&pool, &job).await {
        Ok(job_row) => {
            let response = JobResponse {
                message: "Job created successfully".to_string(),
                job: job_row,
            };
            HttpResponse::Created().json(response)
        }
        Err(err) => {
            // Log the full error for debugging
            eprintln!("Database error: {:?}", err);

            // Return consistent error response (safe for production)
            let error_response = ErrorResponse {
                error: "Failed to create job".to_string(),
                fields: serde_json::json!({"message": "Database error occurred"}),
            };
            HttpResponse::InternalServerError().json(error_response)
        }
    }
}

pub fn job_config(config: &mut ServiceConfig) {
    config.service(scope("jobs").service(create_job));
}

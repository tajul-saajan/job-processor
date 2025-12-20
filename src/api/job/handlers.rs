use actix_web::{
    HttpResponse, Responder, post,
    web::{Data, ServiceConfig, scope},
};
use actix_web_validator::Json;
use actix_multipart::Multipart;
use futures_util::StreamExt;
use sqlx::{Pool, Postgres};
use validator::Validate;
use crate::db::job_repository::JobRepository;
use crate::api::validation::ErrorResponse;
use super::models::Job;
use super::dto::{JobResponse, JobError, BulkJobResponse};

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

#[post("/bulk")]
async fn bulk_create_jobs(
    pool: Data<Pool<Postgres>>,
    mut payload: Multipart,
) -> impl Responder {
    let mut file_data = Vec::new();

    // Read file from multipart stream
    // Size limit is enforced by middleware in main.rs
    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(err) => {
                eprintln!("Multipart error: {:?}", err);
                let error_response = ErrorResponse {
                    error: "Failed to read uploaded file".to_string(),
                    fields: serde_json::json!({"message": "Invalid file upload"}),
                };
                return HttpResponse::BadRequest().json(error_response);
            }
        };

        // Read field data (file content)
        while let Some(chunk) = field.next().await {
            let data = match chunk {
                Ok(data) => data,
                Err(err) => {
                    eprintln!("Chunk read error: {:?}", err);
                    let error_response = ErrorResponse {
                        error: "Failed to read file content".to_string(),
                        fields: serde_json::json!({"message": "Error reading file"}),
                    };
                    return HttpResponse::BadRequest().json(error_response);
                }
            };
            file_data.extend_from_slice(&data);
        }
    }

    // Parse JSON array from file
    let jobs: Vec<Job> = match serde_json::from_slice(&file_data) {
        Ok(jobs) => jobs,
        Err(err) => {
            eprintln!("JSON parse error: {:?}", err);
            let error_response = ErrorResponse {
                error: "Failed to parse JSON file".to_string(),
                fields: serde_json::json!({"message": format!("Invalid JSON: {}", err)}),
            };
            return HttpResponse::BadRequest().json(error_response);
        }
    };

    let mut valid_jobs = Vec::new();
    let mut errors = Vec::new();

    // Validate each job and separate valid from invalid
    for job in jobs {
        if let Err(validation_errors) = job.validate() {
            let error_messages: Vec<String> = validation_errors
                .field_errors()
                .iter()
                .flat_map(|(_, errors)| {
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
        } else {
            valid_jobs.push(job);
        }
    }

    // Bulk insert all valid jobs in a single transaction
    let created_count = if !valid_jobs.is_empty() {
        match JobRepository::bulk_create(&pool, &valid_jobs).await {
            Ok(count) => count as usize,
            Err(err) => {
                eprintln!("Bulk insert error: {:?}", err);
                let error_response = ErrorResponse {
                    error: "Failed to insert jobs into database".to_string(),
                    fields: serde_json::json!({"message": "Database error occurred"}),
                };
                return HttpResponse::InternalServerError().json(error_response);
            }
        }
    } else {
        0
    };

    let response = BulkJobResponse {
        message: format!(
            "Bulk job creation completed. {} created, {} failed",
            created_count,
            errors.len()
        ),
        created: created_count,
        errors,
    };

    HttpResponse::Ok().json(response)
}

pub fn job_config(config: &mut ServiceConfig) {
    config.service(
        scope("jobs")
            .service(create_job)
            .service(bulk_create_jobs)
    );
}

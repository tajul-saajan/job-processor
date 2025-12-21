use actix_web::{
    HttpResponse, Responder, ResponseError, post,
    web::{Data, ServiceConfig, scope},
};
use actix_web_validator::Json;
use actix_multipart::Multipart;
use futures_util::StreamExt;
use tracing::error;
use crate::api::validation::ErrorResponse;
use super::models::Job;
use super::service::JobService;

#[post("")]
async fn create_job(
    service: Data<JobService>,
    job: Json<Job>,
) -> impl Responder {
    // Call service to create job (business logic)
    match service.create_job(&job).await {
        Ok(response) => HttpResponse::Created().json(response),
        Err(e) => e.error_response(),
    }
}

#[post("/bulk")]
async fn bulk_create_jobs(
    service: Data<JobService>,
    mut payload: Multipart,
) -> impl Responder {
    let mut file_data = Vec::new();

    // Read file from multipart stream (infrastructure concern)
    // Size limit is enforced by middleware in main.rs
    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(field) => field,
            Err(err) => {
                error!("Multipart error while reading file: {:?}", err);
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
                    error!("Chunk read error: {:?}", err);
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
    let jobs: Vec<Job> = match serde_json::from_slice::<Vec<Job>>(&file_data) {
        Ok(jobs) => jobs,
        Err(err) => {
            error!("JSON parse error: {:?}", err);
            let error_response = ErrorResponse {
                error: "Failed to parse JSON file".to_string(),
                fields: serde_json::json!({"message": format!("Invalid JSON: {}", err)}),
            };
            return HttpResponse::BadRequest().json(error_response);
        }
    };

    // Call service to handle business logic (validation + bulk insert)
    match service.bulk_create_jobs(jobs).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(e) => e.error_response(),
    }
}

pub fn job_config(config: &mut ServiceConfig) {
    config.service(
        scope("jobs")
            .service(create_job)
            .service(bulk_create_jobs)
    );
}

use actix_web::{
    HttpResponse, Responder, post,
    web::{ServiceConfig, scope},
};
use actix_web_validator::Json;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum JobStatus {
    New,
    Processing,
    Success,
    Failed,
}

#[derive(Deserialize, Serialize, Debug, Validate)]
struct Job {
    #[validate(length(
        min = 3,
        max = 10,
        message = "Name must be between 3 and 10 characters"
    ))]
    name: String,
    status: JobStatus,
}

#[derive(Serialize)]
struct JobResponse {
    message: String,
    job: Job,
}

#[post("")]
async fn process_job(job: Json<Job>) -> impl Responder {
    let response = JobResponse {
        message: "Job processed successfully".to_string(),
        job: job.into_inner(),
    };
    HttpResponse::Ok().json(response)
}

pub fn job_config(config: &mut ServiceConfig) {
    config.service(scope("jobs").service(process_job));
}

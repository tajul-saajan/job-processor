use serde::Serialize;
use crate::db::models::JobRow;

/// Response for single job creation
#[derive(Serialize)]
pub struct JobResponse {
    pub message: String,
    pub job: JobRow,
}

/// Error details for a failed job validation
#[derive(Serialize)]
pub struct JobError {
    pub name: String,
    pub errors: Vec<String>,
}

/// Response for bulk job creation
#[derive(Serialize)]
pub struct BulkJobResponse {
    pub message: String,
    pub created: usize,
    pub errors: Vec<JobError>,
}

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Job status enum representing the state of a job
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    New,
    Processing,
    Success,
    Failed,
}

/// Job model for creating and validating jobs
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

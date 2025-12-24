use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;
use sqlx::{Pool, Postgres};
use tracing::{error, debug};

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    database: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Health check endpoint
///
/// General health check including database connectivity.
/// Use for load balancers and uptime monitors.
#[get("/health")]
async fn health_check(pool: web::Data<Pool<Postgres>>) -> impl Responder {
    match sqlx::query("SELECT 1")
        .fetch_one(pool.get_ref())
        .await
    {
        Ok(_) => {
            HttpResponse::Ok().json(HealthResponse {
                status: "healthy".to_string(),
                database: "connected".to_string(),
                error: None,
            })
        }
        Err(e) => {
            error!("Health check failed: {:?}", e);
            HttpResponse::ServiceUnavailable().json(HealthResponse {
                status: "unhealthy".to_string(),
                database: "disconnected".to_string(),
                error: Some(format!("Database error: {}", e)),
            })
        }
    }
}

/// Readiness check endpoint
///
/// Checks if service is ready to accept traffic (includes database check).
/// Use for Kubernetes readiness probes - removes from load balancer if this fails.
///
/// Returns 503 if dependencies unavailable, but process will recover when they return.
#[get("/ready")]
async fn readiness_check(pool: web::Data<Pool<Postgres>>) -> impl Responder {
    match sqlx::query("SELECT 1")
        .fetch_one(pool.get_ref())
        .await
    {
        Ok(_) => {
            HttpResponse::Ok().json(HealthResponse {
                status: "ready".to_string(),
                database: "connected".to_string(),
                error: None,
            })
        }
        Err(e) => {
            error!("Readiness check failed: database unavailable: {:?}", e);
            HttpResponse::ServiceUnavailable().json(HealthResponse {
                status: "not_ready".to_string(),
                database: "disconnected".to_string(),
                error: Some(format!("Database unavailable: {}", e)),
            })
        }
    }
}

/// Liveness check endpoint
///
/// Simple check that the process is alive. Does not check dependencies.
/// Use for Kubernetes liveness probes - restarts pod if this fails.
#[get("/live")]
async fn liveness_check() -> impl Responder {
    HttpResponse::Ok().json(HealthResponse {
        status: "alive".to_string(),
        database: "not_checked".to_string(),
        error: None,
    })
}

pub fn health_config(config: &mut web::ServiceConfig) {
    config
        .service(health_check)
        .service(readiness_check)
        .service(liveness_check);
}

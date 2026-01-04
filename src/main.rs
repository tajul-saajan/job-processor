use actix_web::{App, HttpResponse, HttpServer, Responder, guard, web};
use actix_multipart::form::MultipartFormConfig;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer, filter::LevelFilter};
mod api;
use crate::api::{
    dummy::dummy_config,
    job::{handlers::job_config, JobService},
    state::{AppState, state_config},
    validation,
    health::health_config,
};
mod config;
mod db;
mod worker;
mod shutdown;
use crate::worker::JobWorker;
use crate::shutdown::ShutdownCoordinator;



fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/test").route(web::route().to(test)));
}

async fn test() -> impl Responder {
    HttpResponse::Gone().body("in test")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    // Load configuration from environment
    let config::Config {
        database_url,
        max_payload_size,
        max_db_connections,
        max_concurrent_jobs,
        num_workers,
        log_dir,
    } = config::Config::from_env()
        .expect("Failed to load configuration");

    // Create logs directory if it doesn't exist
    std::fs::create_dir_all(&log_dir)
        .expect("Failed to create logs directory");

    // Initialize file-based logging with daily rotation and level separation
    // Log files will be created as: logs/info.2024-12-22.log, logs/error.2024-12-22.log, etc.
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info".into());

    // Create daily rotating file appenders for each log level
    let info_file = tracing_appender::rolling::daily(&log_dir, "info.log");
    let warn_file = tracing_appender::rolling::daily(&log_dir, "warn.log");
    let error_file = tracing_appender::rolling::daily(&log_dir, "error.log");
    let debug_file = tracing_appender::rolling::daily(&log_dir, "debug.log");

    // Create layers for each log level
    let info_layer = tracing_subscriber::fmt::layer()
        .with_writer(info_file)
        .with_ansi(false)
        .with_filter(LevelFilter::INFO);

    let warn_layer = tracing_subscriber::fmt::layer()
        .with_writer(warn_file)
        .with_ansi(false)
        .with_filter(LevelFilter::WARN);

    let error_layer = tracing_subscriber::fmt::layer()
        .with_writer(error_file)
        .with_ansi(false)
        .with_filter(LevelFilter::ERROR);

    let debug_layer = tracing_subscriber::fmt::layer()
        .with_writer(debug_file)
        .with_ansi(false)
        .with_filter(LevelFilter::DEBUG);

    // Create console/stdout layer for terminal output
    let console_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true);

    // Initialize the subscriber with all layers (including console)
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer) // Add console output
        .with(info_layer)
        .with(warn_layer)
        .with(error_layer)
        .with(debug_layer)
        .init();

    // Get database connection pool
    let pool = db::connection::get_connection(&database_url, max_db_connections).await
        .expect("Failed to connect to database");

    // No command provided - start the server
    info!("Starting job-processor application");
    info!("Configuration loaded successfully:");
    info!("  - Max payload size: {} bytes", max_payload_size);
    info!("  - Max database connections: {}", max_db_connections);
    info!("  - Max concurrent jobs: {}", max_concurrent_jobs);
    info!("  - Number of workers: {}", num_workers);
    info!("Database connection pool established");

    // Run migrations on startup (auto-migrate when starting server)
    db::migrations::run_migrations(&pool).await
        .expect("Failed to run database migrations");

    info!("Database migrations completed successfully");

    db::cli::run(pool.clone()).await.expect("db cli failed");

    // Create shutdown channel for graceful shutdown
    // watch channel allows multiple receivers to get the same value
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Spawn background workers with semaphore-based bounded concurrency
    let semaphore = Arc::new(Semaphore::new(max_concurrent_jobs));
    let mut worker_handles = Vec::new();

    for worker_id in 1..=num_workers {
        let worker_pool = pool.clone();
        let worker_semaphore = semaphore.clone();
        let worker_shutdown_rx = shutdown_rx.clone();

        let handle = tokio::spawn(async move {
            let job_worker = JobWorker::new(worker_pool);
            job_worker.run(worker_id, worker_semaphore, worker_shutdown_rx).await;
        });

        worker_handles.push(handle);
        info!("Spawned worker {}", worker_id);
    }

    // Clone pool for HTTP server (original will be used for shutdown)
    let server_pool = pool.clone();

    let server = HttpServer::new(move || {
        let my_state = web::Data::new(AppState::new("my_app"));

        // Create JobService with database pool
        let job_service = web::Data::new(JobService::new(server_pool.clone()));

        // Configure payload size limits globally
        let payload_config = web::PayloadConfig::default()
            .limit(max_payload_size);

        let multipart_config = MultipartFormConfig::default()
            .total_limit(max_payload_size);

        App::new()
            .app_data(web::Data::new(server_pool.clone())) // Share DB pool across workers
            .app_data(job_service) // Inject JobService
            .app_data(my_state)
            .app_data(payload_config) // Global payload size limit
            .app_data(multipart_config) // Global multipart/file upload size limit
            .app_data(validation::json_config()) // Global validation config
            .configure(health_config) // Health check endpoints
            .configure(config)
            .configure(state_config)
            .configure(dummy_config)
            .configure(job_config)
            .service(
                web::scope("/guard")
                    .guard(guard::Host("www.tajul.com"))
                    .route("", web::to(|| async { HttpResponse::Ok().body("tajul") })),
            )
            .service(
                web::scope("/guard")
                    .guard(guard::Host("www.saajan.com"))
                    .route("", web::to(|| async { HttpResponse::Ok().body("saajan") })),
            )
            .route("/guard", web::to(HttpResponse::Ok))
    });

    info!("Server starting on http://127.0.0.1:8080");

    // Bind and start the server
    let server = server
        .bind(("127.0.0.1", 8080))?
        .run();

    // Get server handle for graceful shutdown
    let server_handle = server.handle();

    // Spawn server in background
    let server_task = tokio::spawn(server);

    // Create shutdown coordinator and wait for shutdown signal
    let coordinator = ShutdownCoordinator::new(
        server_handle,
        server_task,
        worker_handles,
        shutdown_tx,
        pool,
    );

    coordinator.wait_for_shutdown().await
}

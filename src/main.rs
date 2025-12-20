use actix_web::{App, HttpResponse, HttpServer, Responder, guard, web};
use actix_multipart::form::MultipartFormConfig;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
mod api;
use crate::api::{
    dummy::dummy_config,
    job::handlers::job_config,
    state::{AppState, state_config},
    validation,
};
mod config;
mod db;

fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/test").route(web::route().to(test)));
}

async fn test() -> impl Responder {
    HttpResponse::Gone().body("in test")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing subscriber for logging
    // Set RUST_LOG environment variable to control log level (e.g., RUST_LOG=debug)
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting job-processor application");

    // Load configuration from environment
    let config::Config {
        database_url,
        max_payload_size,
    } = config::Config::from_env()
        .expect("Failed to load configuration");

    info!("Configuration loaded successfully");
    info!("Max payload size: {} bytes", max_payload_size);

    // Get database connection pool
    let pool = db::connection::get_connection(&database_url).await
        .expect("Failed to connect to database");

    info!("Database connection pool established");

    // Run migrations on startup
    db::migrations::run_migrations(&pool).await
        .expect("Failed to run database migrations");

    info!("Database migrations completed successfully");

    let server = HttpServer::new(move || {
        let my_state = web::Data::new(AppState::new("my_app"));

        // Configure payload size limits globally
        let payload_config = web::PayloadConfig::default()
            .limit(max_payload_size);

        let multipart_config = MultipartFormConfig::default()
            .total_limit(max_payload_size);

        App::new()
            .app_data(web::Data::new(pool.clone())) // Share DB pool across workers
            .app_data(my_state)
            .app_data(payload_config) // Global payload size limit
            .app_data(multipart_config) // Global multipart/file upload size limit
            .app_data(validation::json_config()) // Global validation config
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

    server
        .bind(("127.0.0.1", 8080))?
        .run()
        .await?;

    info!("Server stopped");
    Ok(())
}

use actix_web::{App, HttpResponse, HttpServer, Responder, guard, web};
use actix_multipart::form::MultipartFormConfig;
use clap::{Parser, Subcommand};
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

/// Job Processor - A high-performance REST API for managing jobs
#[derive(Parser)]
#[command(name = "job-processor")]
#[command(about = "Job processor with database migration management", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run database migrations
    Migrate,

    /// Rollback database migrations
    Rollback {
        /// Number of migrations to rollback (default: 1)
        #[arg(short, long, default_value_t = 1)]
        steps: i64,
    },

    /// Rollback all migrations to fresh state
    Fresh,

    /// Refresh database (rollback all + re-migrate)
    Refresh,
}

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

    // Parse CLI arguments
    let cli = Cli::parse();

    // Load configuration from environment
    let config::Config {
        database_url,
        max_payload_size,
        max_db_connections,
    } = config::Config::from_env()
        .expect("Failed to load configuration");

    // Get database connection pool
    let pool = db::connection::get_connection(&database_url, max_db_connections).await
        .expect("Failed to connect to database");

    // Handle migration commands if provided
    if let Some(command) = cli.command {
        match command {
            Commands::Migrate => {
                info!("Running migrations command...");
                db::migrations::run_migrations(&pool).await
                    .expect("Failed to run migrations");
                info!("Migrations completed. Exiting.");
                return Ok(());
            }
            Commands::Rollback { steps } => {
                info!("Running rollback command with {} step(s)...", steps);
                db::migrations::rollback_migrations(&pool, steps).await
                    .expect("Failed to rollback migrations");
                info!("Rollback completed. Exiting.");
                return Ok(());
            }
            Commands::Fresh => {
                info!("Running fresh command (rollback all)...");
                db::migrations::rollback_all_migrations(&pool).await
                    .expect("Failed to rollback all migrations");
                info!("Fresh completed. Exiting.");
                return Ok(());
            }
            Commands::Refresh => {
                info!("Running refresh command (rollback all + re-migrate)...");
                db::migrations::refresh_database(&pool).await
                    .expect("Failed to refresh database");
                info!("Refresh completed. Exiting.");
                return Ok(());
            }
        }
    }

    // No command provided - start the server
    info!("Starting job-processor application");
    info!("Configuration loaded successfully");
    info!("Max payload size: {} bytes", max_payload_size);
    info!("Max database connections: {}", max_db_connections);
    info!("Database connection pool established");

    // Run migrations on startup (auto-migrate when starting server)
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

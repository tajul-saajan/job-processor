use actix_web::{App, HttpResponse, HttpServer, Responder, guard, web};
mod api;
use crate::api::{
    dummy::dummy_config,
    job::handlers::job_config,
    state::{AppState, state_config},
    validation,
};
mod db;

fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/test").route(web::route().to(test)));
}

async fn test() -> impl Responder {
    HttpResponse::Gone().body("in test")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Get database connection
    let conn = db::connection::get_connection().await
        .expect("Failed to connect to database");

    // Run migrations on startup
    db::migrations::run_migrations(&conn).await
        .expect("Failed to run database migrations");

    HttpServer::new(|| {
        let my_state = web::Data::new(AppState::new("my_app"));
        App::new()
            .app_data(my_state)
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
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

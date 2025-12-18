use actix_web::{App, HttpResponse, HttpServer, Responder, get, web};
mod http;
use http::dummy::get_scope as get_dummy_scope;

fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/test").route(web::route().to(test)));
}

async fn test() -> impl Responder {
    HttpResponse::Gone().body("in test")
}

struct AppState {
    app_name: String,
}

#[get("/state")]
async fn get_state(data: web::Data<AppState>) -> impl Responder {
    let app_name = &data.app_name;
    format!("{}", app_name)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .app_data(web::Data::new(AppState {
                app_name: "my_app".into(),
            }))
            .configure(config)
            .service(get_dummy_scope())
            .service(get_state)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

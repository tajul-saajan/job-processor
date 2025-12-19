use actix_web::{App, HttpResponse, HttpServer, Responder, guard, web};
mod http;
use http::dummy::get_scope as get_dummy_scope;
use http::state::get_scope as get_state_scope;

use crate::http::state::AppState;

fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::resource("/test").route(web::route().to(test)));
}

async fn test() -> impl Responder {
    HttpResponse::Gone().body("in test")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let my_state = web::Data::new(AppState::new("my_app"));
        App::new()
            .app_data(my_state)
            .configure(config)
            .service(get_state_scope())
            .service(get_dummy_scope())
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

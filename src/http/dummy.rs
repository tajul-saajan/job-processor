use actix_web::{HttpResponse, Responder, get, post, web};

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello world!")
}

#[post("/echo")]
async fn echo(req_body: String) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Hey there!")
}

fn get_scope() ->actix_web::Scope {
    web::scope("dummy")
        .service(hello)
        .service(echo)
        .route("/hey", web::get().to(manual_hello))
}

pub fn dummy_config(config: &mut web::ServiceConfig){
    config.service(get_scope());
}
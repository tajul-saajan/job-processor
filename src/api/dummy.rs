use actix_web::{HttpResponse, Responder, get, post, web};
use serde::Deserialize;

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

#[derive(Deserialize)]
struct MyPost {
    body: String,
}

#[derive(Deserialize)]
struct MyQuery {
    filter: bool,
}

#[post("/posts/{post_id}")]
async fn path_check(
    path: web::Path<u32>,
    post: web::Json<MyPost>,
    q: web::Query<MyQuery>,
) -> impl Responder {
    let post_id = path.into_inner();
    HttpResponse::Ok().body(format!(
        "post_id is {}. post body is {}. query filter is {}",
        post_id, post.body, q.filter
    ))
}

fn get_scope() -> actix_web::Scope {
    web::scope("dummy")
        .service(hello)
        .service(echo)
        .service(path_check)
        .route("/hey", web::get().to(manual_hello))
}

pub fn dummy_config(config: &mut web::ServiceConfig) {
    config.service(get_scope());
}

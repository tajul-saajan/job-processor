use actix_web::{HttpResponse, Responder, get, web};

pub struct AppState {
    app_name: String,
}

impl AppState {
    pub fn new(name: &str) -> Self {
        AppState {
            app_name: name.into(),
        }
    }
}

#[get("")]
async fn get_state(data: web::Data<AppState>) -> impl Responder {
    let app_name = &data.app_name;
    let d = format!("state is: {}", app_name);
    HttpResponse::Ok().body(d)
}

pub fn get_scope() -> actix_web::Scope {
    web::scope("state").service(get_state)
}

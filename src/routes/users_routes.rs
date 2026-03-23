use actix_web::web;
use crate::handlers::users_handlers;

// You need to add this into the routes.rs file to use
// Either public_routes or private_routes

pub fn users_routes(cfg: &mut web::ServiceConfig) {{
    cfg.service(
        web::scope("/{}")
            .route("", web::get().to(users_handlers::get_users))
            .route("", web::post().to(users_handlers::create_users))
            .route("/{{id}}", web::get().to(users_handlers::get_users))
            .route("/{{id}}", web::put().to(users_handlers::update_users))
            .route("/{{id}}", web::delete().to(users_handlers::delete_users))
    );
}}
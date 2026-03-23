// Route configuration module
// Defines all public API routes and their handlers
use actix_web::web;

use crate::handlers::{auth, handlers, users_handlers, ws};

/// Configures all public routes for the application
pub fn public_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/users")
            .route("/hello",    web::get().to(handlers::hello))
            .route("/register", web::post().to(auth::register))
            .route("/login",    web::post().to(auth::login))
            .route("/refresh", web::post().to(auth::refresh))
            .route("/guest",    web::post().to(auth::guest))
    );
}

/// Configures all private routes for the application
pub fn private_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/users")
            .route("",      web::get().to(users_handlers::get_users))
            .route("",      web::post().to(users_handlers::create_users))
            .route("/{id}", web::get().to(users_handlers::get_users))
            .route("/{id}", web::put().to(users_handlers::update_users))
            .route("/{id}", web::delete().to(users_handlers::delete_users))
    );

    cfg.route("/ws", web::get().to(ws::ws_handler));
}
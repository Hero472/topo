// Main entry point for the Actix Web application

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use actix_cors::Cors;
use topo::app_state::AppState;
use topo::infrastructure::ws_handler::ws_handler;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState {
        rooms: Arc::new(Mutex::new(HashMap::new())),
    });

    println!("Server running at http://localhost:8080");

    HttpServer::new(move || {
        println!("⚙️ Building app instance");
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .route("/ws/{room_id}", web::get().to(ws_handler))
            .route("/health", web::get().to(|| async { "OK" }))
            .route("/{tail:.*}", web::get().to(|req: HttpRequest| async move {
                println!("📥 Catch-all: {} {}", req.method(), req.uri());
                HttpResponse::NotFound().body("not found")
            }))
    })
    .bind("127.0.0.1:8080")?
    .bind("[::1]:8080")?
    .run()
    .await
}
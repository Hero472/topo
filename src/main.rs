// Main entry point for the Actix Web application

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use actix_cors::Cors;
use tokio::sync::mpsc;
use topo::app_state::AppState;
use topo::infrastructure::ws_handler::ws_handler;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let (shutdown_tx, mut shutdown_rx) = mpsc::unbounded_channel::<String>();
    let rooms_map = Arc::new(Mutex::new(HashMap::new()));
    let rooms_map_clone = Arc::clone(&rooms_map);

    tokio::spawn(async move {
        while let Some(room_id) = shutdown_rx.recv().await {
            if let Ok(mut rooms) = rooms_map_clone.lock() {
                rooms.remove(&room_id);
                log::info!("Room {} removed from global map", room_id);
            }
        }
    });

    let app_state = web::Data::new(AppState {
        rooms: rooms_map,
        room_shutdown_tx: shutdown_tx,
    });

    println!("Server running at http://localhost:8080");

    let server = HttpServer::new(move || {
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
    .run();

    let server_handle = server.handle();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("failed to listen for ctrl-c");
        log::info!("Received Ctrl+C, initiating graceful shutdown...");
        // true = graceful (wait for connections to finish)
        server_handle.stop(true).await;
    });

    // Run the server until it's stopped
    server.await
}
// Main entry point for the Actix Web application

use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse};
use actix_cors::Cors;
use topo::{core::game::{actions::Action}, infrastructure::{full_state::build_full_state, room::room_handler::{RoomHandle, RoomRegistry}, server_event::{OpponentView, ServerEvent}, views::{PersonalPileView, PlayerBoardView}}};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

pub struct AppState {
    pub rooms: RoomRegistry
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let app_state = web::Data::new(AppState {
        rooms: Arc::new(Mutex::new(HashMap::new())),
    });

    println!("Server running at http://localhost:8080");

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            .route("/ws/{room_id}", web::get().to(ws_handler))
            .route("/health", web::get().to(|| async { "OK" }))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, actix_web::Error> {
    let room_id = path.into_inner();

    println!("ws_handler called room={}", room_id);

    // ── Parse query parameters ──
    let query = req.query_string();
    let player_id: usize = extract_query_param(query, "player_id")
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let username: String = extract_query_param(query, "username")
        .unwrap_or_else(|| "Anonymous".into());

    // ── Get or create room ──
    let room = {
        let mut rooms = state.rooms.lock().await;
        rooms
            .entry(room_id.clone())
            .or_insert_with(|| RoomHandle::new_arc(room_id.clone(), 60, vec![1, 2]))
            .clone()
    };

    // ── Add player (may start the game if second player) ──
    println!("adding player {player_id}");
    room.add_player(player_id, username).await;
    println!("player added");

    // ── Opponent info ──
    let opponent_id = if player_id == 1 { 2 } else { 1 };
    let opponent_username = room
        .players
        .lock()
        .await
        .get(&opponent_id)
        .cloned()
        .unwrap_or_default();

    // ── Build FullState (no more .expect) ──
    let full_state_event = {
        let game_state = room.state.lock().await;
        match build_full_state(&game_state, player_id, opponent_username) {
            Some(ev) => ev,
            None => {
                eprintln!("build_full_state returned None");
                return Ok(HttpResponse::InternalServerError().body("Game state not ready"));
            }
        }
    };
    println!("FullState built");

    // ── WebSocket upgrade ──
    println!("upgrading to WebSocket");
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;
    println!("WebSocket upgrade succeeded");

    // ── Send initial FullState ──
    match serde_json::to_string(&full_state_event) {
        Ok(json) => {
            let _ = session.text(json).await;
            println!("FullState sent");
        }
        Err(e) => {
            eprintln!("failed to serialize FullState: {e}");
        }
    }

    // ── Subscribe to events ──
    let mut event_rx = room.subscribe_player(player_id).await;

    // ── Forward events to client (never panics) ──
    let mut session_clone = session.clone();
    actix_rt::spawn(async move {
        while let Some(game_msg) = event_rx.recv().await {
            match serde_json::to_string(&game_msg.event) {
                Ok(json) => {
                    if session_clone.text(json).await.is_err() {
                        break;   // client disconnected
                    }
                }
                Err(e) => {
                    eprintln!("failed to serialize event: {e}");
                    break;
                }
            }
        }
        println!("forwarding task ended");
    });

    // ── Read actions from client ──
    let room_clone = room.clone();
    actix_rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.recv().await {
            match msg {
                actix_ws::Message::Text(text) => {
                    match serde_json::from_str::<Action>(&text) {
                        Ok(action) => {
                            room_clone.apply_action(player_id, action).await;
                        }
                        Err(e) => {
                            eprintln!("❌ invalid action from client: {e}");
                        }
                    }
                }
                actix_ws::Message::Close(_) => {
                    room.remove_player(player_id).await;
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(response)
}

// Helper to extract query parameters
fn extract_query_param(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            return parts.next().map(|v| v.to_string());
        }
    }
    None
}
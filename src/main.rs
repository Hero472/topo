// Main entry point for the Actix Web application

use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse};
use topo::{core::game::{actions::Action, deck::DeckColor}, infrastructure::{full_state::build_full_state, room::room_handler::{RoomHandle, RoomRegistry}, server_event::{OpponentView, ServerEvent}, views::{PersonalPileView, PlayerBoardView}}};
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
        App::new()
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

    // Extract player_id and username from query string
    let query = req.query_string();
    let player_id: usize = extract_query_param(query, "player_id")
        .unwrap_or_default()
        .parse()
        .unwrap_or(0);
    let username: String = extract_query_param(query, "username")
        .unwrap_or_else(|| "Anonymous".into());

    // Create or get the room
    let room = {
        let mut rooms = state.rooms.lock().await;
        rooms
            .entry(room_id.clone())
            .or_insert_with(|| Arc::new(RoomHandle::new(room_id.clone(), 60, vec![1, 2])))
            .clone()
    };

    // Add the player to the room
    room.add_player(player_id, username).await;

    // Determine opponent's player_idx and username (assumes exactly two players: 1 and 2)
    let opponent_id = if player_id == 1 { 2 } else { 1 };
    let opponent_username = room.players.lock().await
        .get(&opponent_id)
        .cloned()
        .unwrap_or_default();

    // --- Build the FullState while holding the game lock ---
    let full_state = {
        let game_state = room.state.lock().await;
        build_full_state(&game_state, player_id, opponent_username)
            .expect("Player board must exist")   // we know it does because we created it
    };

    // Upgrade the connection to WebSocket
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)?;

    // Send the FullState immediately to the new client
    let full_state_json = serde_json::to_string(&full_state).unwrap();
    let _ = session.text(full_state_json).await;

    // Subscribe to room events
    let mut event_rx = room.subscribe();

    // Clone session for the writer task
    let mut session_clone = session.clone();

    // Writer task: forward events to the WebSocket (filtered by `to` field)
    let _forward_task = actix_rt::spawn(async move {
        while let Ok(game_msg) = event_rx.recv().await {
            let should_send = game_msg.to.map_or(true, |id| id == player_id);
            if should_send {
                let json = serde_json::to_string(&game_msg.event).unwrap();
                if session_clone.text(json).await.is_err() {
                    break; // connection closed
                }
            }
        }
    });

    // Reader task: receive actions from the client
    let room_clone = room.clone();
    actix_rt::spawn(async move {
        while let Some(Ok(msg)) = msg_stream.recv().await {
            match msg {
                actix_ws::Message::Text(text) => {
                    if let Ok(action) = serde_json::from_str::<Action>(&text) {
                        room_clone.apply_action(player_id, action).await;
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
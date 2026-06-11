use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::select;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::core::game::actions::Action;
use crate::core::game::state::Seconds;
use crate::core::player::PlayerId;
use crate::infrastructure::room::room_handler::RoomHandle;

#[derive(Deserialize)]
pub struct WsQuery {
    player_id: String,
    username: Option<String>,
}

pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<WsQuery>,
) -> Result<HttpResponse, actix_web::Error> {

    let room_id = path.into_inner();

    let player_uuid = Uuid::parse_str(&query.player_id)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid player_id UUID: {}", e)))?;
    let player_id = PlayerId(player_uuid);

    let username = query.username.as_deref().unwrap_or("Anonymous").to_string();

    println!("ws_handler room={}, player={:?}, username={}", room_id, player_id, username);

    let room = {
        let mut rooms = state.rooms.lock().unwrap();
        rooms
            .entry(room_id.clone())
            .or_insert_with(|| {
                // Default turn time of 60 seconds
                RoomHandle::new_arc(room_id.clone(), Seconds(60))
            })
            .clone()
    };

    let mut event_rx = room.subscribe_player(player_id);

    let _ = room.add_player(player_id, username);

    let (response, session, msg_stream) = match actix_ws::handle(&req, body) {
        Ok(tuple) => {
            println!("WebSocket upgrade succeeded");
            tuple
        }
        Err(e) => {
            eprintln!("WebSocket upgrade FAILED: {e}");
            return Err(e);
        }
    };

    let room_clone = room.clone();
    let state_clone = state.clone();
    let room_id_clone = room_id.clone();

    actix_rt::spawn(async move {
        println!("🚀 spawn task started for player {:?}", player_id);
        let mut session = session;
        let mut msg_stream = msg_stream;

        loop {
            select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(game_msg) => {
                            let json = match serde_json::to_string(&game_msg.event) {
                                Ok(s) => s,
                                Err(e) => {
                                    eprintln!("Serialization error: {e}");
                                    break;
                                }
                            };
                            if session.text(json).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }

                maybe_msg = msg_stream.next() => {
                    match maybe_msg {
                        Some(Ok(msg)) => match msg {
                            Message::Text(text) => {
                                if let Ok(action) = serde_json::from_str::<Action>(&text) {
                                    if room_clone.apply_action(player_id, action).is_err() {
                                        eprintln!("Failed to apply action");
                                        break;
                                    }
                                }
                            }
                            Message::Close(_) => break,
                            _ => {}
                        },
                        Some(Err(e)) => {
                            eprintln!("WS receive error: {e}");
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        // Cleanup on disconnect
        let _ = room_clone.remove_player(player_id);
        if let Ok(mut rooms) = state_clone.rooms.lock() {
            rooms.remove(&room_id_clone);
        }
        println!("WebSocket task ended for player {:?}", player_id);
    });

    Ok(response)
}
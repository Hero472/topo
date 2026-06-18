use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use serde::Deserialize;
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

    log::info!("ws_handler room={}, player={:?}, username={}", room_id, player_id, username);

    let room = {
        let mut rooms = state.rooms.lock().unwrap();
        rooms
            .entry(room_id.clone())
            .or_insert_with(|| {
                RoomHandle::new_arc(
                    room_id.clone(),
                    Seconds(60),
                    state.room_shutdown_tx.clone(),
                )
            })
            .clone()
    };

    // Subscribe to events BEFORE adding the player so we don't miss any.
    let mut event_rx = room.subscribe_player(player_id);

    // Add player to the room; this will broadcast PlayerJoined etc.
    if let Err(e) = room.add_player(player_id, username) {
        log::error!("Failed to add player to room: {:?}", e);
        return Err(actix_web::error::ErrorInternalServerError("Could not join room"));
    }

    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)
        .map_err(|e| {
            log::error!("WebSocket upgrade failed: {}", e);
            e
        })?;

    let room_clone = room.clone();

    actix_rt::spawn(async move {
        log::debug!("Spawned WebSocket task for player {:?}", player_id);

        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(game_msg) => {
                            let json = match serde_json::to_string(&game_msg.event) {
                                Ok(s) => s,
                                Err(e) => {
                                    log::error!("Serialization error: {}", e);
                                    // Skip this event; don't break the connection
                                    continue;
                                }
                            };
                            if session.text(json).await.is_err() {
                                // Client disconnected
                                break;
                            }
                        }
                        None => {
                            // Server closed the channel – room is shutting down
                            break;
                        }
                    }
                }

                maybe_msg = msg_stream.next() => {
                    match maybe_msg {
                        Some(Ok(msg)) => match msg {
                            Message::Text(text) => {
                                match serde_json::from_str::<Action>(&text) {
                                    Ok(action) => {
                                        if let Err(e) = room_clone.apply_action(player_id, action) {
                                            log::warn!("Failed to apply action: {:?}", e);
                                            // Send error back to client
                                            let err = serde_json::json!({
                                                "error": "invalid_move",
                                                "details": format!("{:?}", e)
                                            });
                                            let _ = session.text(err.to_string()).await;
                                            // Don't break; client can try again
                                        }
                                    }
                                    Err(e) => {
                                        // Invalid JSON – notify client
                                        let err = serde_json::json!({
                                            "error": "invalid_action",
                                            "details": e.to_string()
                                        });
                                        let _ = session.text(err.to_string()).await;
                                    }
                                }
                            }
                            Message::Close(_) => break,
                            _ => {}
                        },
                        Some(Err(e)) => {
                            log::error!("WebSocket receive error: {}", e);
                            break;
                        }
                        None => break,
                    }
                }
            }
        }

        // Player disconnected or WebSocket closed – tell the room
        log::info!("Player {:?} disconnected, cleaning up", player_id);
        let _ = room_clone.remove_player(player_id);

        // NOTE: Do NOT remove the room from the global map here.
        // That's the room actor's responsibility. The room may still be alive
        // with other players. When the room becomes empty, the actor should
        // eventually shut itself down and unregister from the map.
    });

    Ok(response)
}
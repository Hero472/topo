use std::time::Duration;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::core::game::actions::Action;
use crate::core::game_id::GameId;
use crate::core::player::PlayerId;

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
    let game_id = GameId(path.into_inner());

    let player_uuid = Uuid::parse_str(&query.player_id)
        .map_err(|e| actix_web::error::ErrorBadRequest(format!("Invalid player_id UUID: {}", e)))?;
    let player_id = PlayerId(player_uuid);
    let username = query.username.as_deref().unwrap_or("Anonymous").to_string();

    log::info!("ws_handler room={}, player={:?}, username={}", game_id, player_id, username);

    // ── Get or create the room ──
    let room = {
        let rooms = state.rooms.lock().unwrap();

        match rooms.get(&game_id) {
            Some(room) => room.clone(),
            None => {
                return Err(actix_web::error::ErrorNotFound(
                    "Game not found",
                ));
            }
        }
    };

    let is_reconnect = room.is_player_known(player_id).await;

    let mut event_rx = room.subscribe_player(player_id);

    if is_reconnect {
        if let Err(e) = room.reconnect_player(player_id) {
            log::error!("Failed to reconnect player: {:?}", e);
            let _ = room.unsubscribe_player(player_id);
            return Err(actix_web::error::ErrorInternalServerError("Reconnect failed"));
        }
    } else {
        // Fresh join
        if let Err(e) = room.add_player(player_id, username) {
            log::error!("Failed to add player to room: {:?}", e);
            let _ = room.unsubscribe_player(player_id);
            return Err(actix_web::error::ErrorInternalServerError("Could not join room"));
        }
    }

    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)
        .map_err(|e| {
            log::error!("WebSocket upgrade failed: {}", e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    let room_clone = room.clone();

    actix_rt::spawn(async move {
        log::debug!("Spawned WebSocket task for player {:?}", player_id);

        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(game_msg) => {
                            let json = match serde_json::to_string(&game_msg.event) {
                                Ok(s) => s,
                                Err(e) => {
                                    log::error!("Serialization error: {}", e);
                                    continue; // skip this event
                                }
                            };
                            if session.text(json).await.is_err() {
                                break; // client disconnected
                            }
                        }
                        None => {
                            // Room shut down – channel closed
                            let _ = session.close(Some(actix_ws::CloseReason {
                                code: actix_ws::CloseCode::Normal,
                                description: Some("Room closed".into()),
                            })).await;
                            break;
                        }
                    }
                }

                // Heartbeat – send a ping every 30 seconds
                _ = heartbeat.tick() => {
                    if session.ping(b"").await.is_err() {
                        break; // connection lost
                    }
                }

                // Client → server messages
                maybe_msg = msg_stream.next() => {
                    match maybe_msg {
                        Some(Ok(msg)) => match msg {
                            Message::Text(text) => {
                                match serde_json::from_str::<Action>(&text) {
                                    Ok(action) => {
                                        if let Err(e) = room_clone.apply_action(player_id, action) {
                                            log::warn!("Failed to apply action: {:?}", e);
                                            let err = serde_json::json!({
                                                "error": "invalid_move",
                                                "details": format!("{:?}", e)
                                            });
                                            let _ = session.text(err.to_string()).await;
                                        }
                                    }
                                    Err(e) => {
                                        let err = serde_json::json!({
                                            "error": "invalid_action",
                                            "details": e.to_string()
                                        });
                                        let _ = session.text(err.to_string()).await;
                                    }
                                }
                            }
                            Message::Ping(bytes) => {
                                if session.pong(&bytes).await.is_err() {
                                    break;
                                }
                            }
                            Message::Pong(_) => {
                                // nothing needed, but keeps connection alive
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

        // Player disconnected – tell the room
        log::info!("Player {:?} disconnected, cleaning up", player_id);
        let _ = room_clone.remove_player(player_id);
    });

    Ok(response)
}
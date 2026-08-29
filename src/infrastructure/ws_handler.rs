use std::time::Duration;

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::app_state::AppState;
use crate::core::game::actions::Action;
use crate::core::game_id::GameId;
use crate::core::player::PlayerId;
use crate::infrastructure::room::room_command::LobbyAction;

// ── Router enum for incoming WebSocket messages ──
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClientMessage {
    Lobby(LobbyAction),
    Game(Action),
}

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

    // ── 1. Get the room ──
    let room = {
        let rooms = state.rooms.lock().unwrap();
        match rooms.get(&game_id) {
            Some(room) => room.clone(),
            None => {
                return Err(actix_web::error::ErrorNotFound("Game not found"));
            }
        }
    };

    let is_reconnect = room.is_player_known(player_id).await;

    // ── 2. Subscribe FIRST to ensure the actor has a channel to send to ──
    let mut event_rx = room.subscribe_player(player_id);

    // ── 3. Handle join/reconnect logic ──
    let join_result = if is_reconnect {
        room.reconnect_player(player_id)
    } else {
        room.add_player(player_id, username.clone())
    };

    if let Err(e) = join_result {
        log::error!("Failed to join/reconnect player {:?}: {:?}", player_id, e);
        // Cleanup the subscription placeholder we just created since the join failed
        let _ = room.unsubscribe_player(player_id);
        return Err(actix_web::error::ErrorInternalServerError("Could not join room"));
    }

    // ── 4. Upgrade to WebSocket ──
    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, body)
        .map_err(|e| {
            log::error!("WebSocket upgrade failed: {}", e);
            actix_web::error::ErrorInternalServerError(e)
        })?;

    let room_clone = room.clone();

    // ── 5. Spawn the WebSocket handling task ──
    actix_rt::spawn(async move {
        log::debug!("Spawned WebSocket task for player {:?}", player_id);

        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                // ── Incoming Server Events ──
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(game_msg) => {
                            let json = match serde_json::to_string(&game_msg.event) {
                                Ok(s) => s,
                                Err(e) => {
                                    log::error!("Serialization error for player {:?}: {}", player_id, e);
                                    continue;
                                }
                            };
                            if session.text(json).await.is_err() {
                                log::debug!("Failed to send message to player {:?}, breaking loop", player_id);
                                break;
                            }
                        }
                        None => {
                            // Room shut down – channel closed
                            log::info!("Room channel closed for player {:?}", player_id);
                            let _ = session.close(Some(actix_ws::CloseReason {
                                code: actix_ws::CloseCode::Normal,
                                description: Some("Room closed".into()),
                            })).await;
                            break;
                        }
                    }
                }

                // ── Heartbeat ──
                _ = heartbeat.tick() => {
                    if session.ping(b"").await.is_err() {
                        log::debug!("Heartbeat ping failed for player {:?}", player_id);
                        break;
                    }
                }

                // ── Incoming Client Messages ──
                maybe_msg = msg_stream.next() => {
                    match maybe_msg {
                        Some(Ok(msg)) => match msg {
                            Message::Text(text) => {
                                match serde_json::from_str::<ClientMessage>(&text) {
                                    Ok(ClientMessage::Lobby(lobby_action)) => {
                                        if let Err(e) = room_clone.apply_lobby_action(player_id, lobby_action) {
                                            log::warn!("Failed to apply lobby action for {:?}: {:?}", player_id, e);
                                            let err = serde_json::json!({
                                                "type": "error",
                                                "code": "invalid_lobby_action",
                                                "message": format!("{:?}", e)
                                            });
                                            let _ = session.text(err.to_string()).await;
                                        }
                                    }
                                    Ok(ClientMessage::Game(game_action)) => {
                                        if let Err(e) = room_clone.apply_action(player_id, game_action) {
                                            log::warn!("Failed to apply game action for {:?}: {:?}", player_id, e);
                                            let err = serde_json::json!({
                                                "type": "error",
                                                "code": "invalid_move",
                                                "message": format!("{:?}", e)
                                            });
                                            let _ = session.text(err.to_string()).await;
                                        }
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to parse client message from {:?}: {}", player_id, e);
                                        let err = serde_json::json!({
                                            "type": "error",
                                            "code": "invalid_action",
                                            "message": e.to_string()
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
                            Message::Pong(_) => {}
                            Message::Close(reason) => {
                                log::info!("Client sent Close frame for player {:?}: {:?}", player_id, reason);
                                break;
                            }
                            _ => {}
                        },
                        Some(Err(e)) => {
                            log::error!("WebSocket receive error for player {:?}: {}", player_id, e);
                            break;
                        }
                        None => {
                            log::info!("WebSocket stream ended for player {:?}", player_id);
                            break;
                        }
                    }
                }
            }
        }

        // ── 6. Cleanup on disconnect ──
        // We use `network_disconnect` to trigger the 30-second grace period.
        // If the player intentionally clicked "Leave", the frontend should have 
        // sent `LobbyAction::Leave` (which maps to `PlayerLeft`), bypassing this.
        log::info!("Player {:?} network disconnected, starting grace period", player_id);
        let _ = room_clone.network_disconnect(player_id);
    });

    Ok(response)
}
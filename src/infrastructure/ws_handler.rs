use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::select;

use crate::app_state::AppState;
use crate::infrastructure::room::room_handler::RoomHandle;
use crate::core::game::actions::Action;

#[derive(Deserialize)]
pub struct WsQuery {
    player_id: usize,
    username: Option<String>,
}

pub async fn ws_handler_dummy(
    req: HttpRequest,
    body: web::Payload,
    _state: web::Data<AppState>,
    path: web::Path<String>,
    _query: web::Query<WsQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let room_id = path.into_inner();

    // Log the raw request immediately
    println!(">>> Raw request: {} {}, query: {}", req.method(), req.uri(), req.query_string());

    // Manually extract player_id (default to 0)
    let player_id = req
        .query_string()
        .split('&')
        .find(|s| s.starts_with("player_id="))
        .and_then(|s| s.split('=').nth(1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    let username = req
        .query_string()
        .split('&')
        .find(|s| s.starts_with("username="))
        .and_then(|s| s.split('=').nth(1))
        .map(|v| v.to_string())
        .unwrap_or_else(|| "Anonymous".into());

    println!("ws_handler room={}, player={}, username={}", room_id, player_id, username);

    // Attempt WebSocket upgrade
    let (response, mut session, _) = actix_ws::handle(&req, body)?;
    println!("✅ WebSocket upgrade ok, keeping alive");

    // Keep the session alive indefinitely
    tokio::spawn(async move {
        // Just hold the session – do nothing
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        }
    });

    Ok(response)
}


pub async fn ws_handler(
    req: HttpRequest,
    body: web::Payload,
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<WsQuery>,
) -> Result<HttpResponse, actix_web::Error> {
    let room_id = path.into_inner();
    let player_id = query.player_id;
    let username = query.username.as_deref().unwrap_or("Anonymous").to_string();

    println!("ws_handler room={}, player={}, username={}", room_id, player_id, username);

    let room = {
        let mut rooms = state.rooms.lock().unwrap();
        rooms
            .entry(room_id.clone())
            .or_insert_with(|| {
                // Adjust to match your actual new_arc signature.
                // If it now takes 2 args, remove vec![1, 2].
                RoomHandle::new_arc(room_id.clone(), 60)
            })
            .clone()
    };

    let _ = room.add_player(player_id, username);

    let (response, session, msg_stream) = match actix_ws::handle(&req, body) {
        Ok(tuple) => {
            println!("✅ WebSocket upgrade succeeded");
            tuple
        }
        Err(e) => {
            eprintln!("❌ WebSocket upgrade FAILED: {e}");
            return Err(e);
        }
    };

    let mut event_rx = room.subscribe_player(player_id);

    // 6. Combined send + receive loop
    let room_clone = room.clone();
    let state_clone = state.clone();
    let room_id_clone = room_id.clone();

    actix_rt::spawn(async move {
        println!("🚀 spawn task started for player {player_id}");
        // session is !Send, but actix_rt::spawn supports local tasks
        let mut session = session;
        let mut msg_stream = msg_stream;

        loop {
            select! {
                // Incoming game event from the room actor
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
                                break; // client disconnected
                            }
                        }
                        None => break, // channel closed
                    }
                }

                // Incoming WebSocket message from the client
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

        // Cleanup
        let _ = room_clone.remove_player(player_id);
        if let Ok(mut rooms) = state_clone.rooms.lock() {
            rooms.remove(&room_id_clone);
        }
        println!("WebSocket task ended for player {}", player_id);
    });

    Ok(response)
}
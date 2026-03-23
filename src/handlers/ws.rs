use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use actix_ws::Message;
use futures_util::StreamExt;
use std::sync::Arc;
use cargo_smith::Claims;

use crate::game::room::{RoomHandle, RoomRegistry, ServerEvent};
use crate::game::state::GamePhase;
use crate::game::timer::start_turn_timer;
use crate::handlers::game_actions::{broadcast_state_sync, handle_action};

pub async fn ws_handler(
    req:   HttpRequest,
    body:  web::Payload,
    rooms: web::Data<RoomRegistry>
) -> Result<HttpResponse, actix_web::Error> {
    // Claims were injected by JwtMiddleware — no manual token validation needed
    let claims = req.extensions().get::<Claims<serde_json::Value>>().cloned()
        .ok_or_else(|| actix_web::error::ErrorUnauthorized("missing claims"))?;

    let player_id = claims.sub.clone();
    let username  = claims.data
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Get room_id from query param: /ws?room_id=abc123
    let room_id = web::Query::<std::collections::HashMap<String, String>>
        ::from_query(req.query_string())
        .ok()
        .and_then(|q| q.get("room_id").cloned())
        .ok_or_else(|| actix_web::error::ErrorBadRequest("missing room_id"))?;

    // Find or create the room
    let room_handle = {
        let mut registry = rooms.lock().await;
        registry
            .entry(room_id.clone())
            .or_insert_with(|| Arc::new(RoomHandle::new(room_id.clone(), 30)))
            .clone()
    };

    // Upgrade to WebSocket
    let (response, mut session, mut stream) = actix_ws::handle(&req, body)?;

    // Join the game
    {
        let mut gs = room_handle.state.lock().await;

        log::info!("Player {} joining room {}. Current players: {}", 
            player_id, room_id, gs.players.len());

        if !gs.add_player(player_id.clone(), username.clone()) {
            log::info!("Room full — rejecting {}", player_id);
            return Err(actix_web::error::ErrorForbidden("room is full"));
        }

        log::info!("Player {} joined. Total players: {}", player_id, gs.players.len());

        room_handle.broadcast(ServerEvent::PlayerJoined { username: username.clone() });

        // Both players joined — game just started
        if gs.phase == GamePhase::Playing {
            let current = gs.current_player_id()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let secs  = gs.turn_seconds;
            let turn  = gs.current_turn;
            drop(gs);

            room_handle.broadcast(ServerEvent::GameStarted {
                current_player_id: current,
            });

            // Draw first card for the starting player
            {
                let mut gs = room_handle.state.lock().await;
                if let Some(card) = gs.draw_for_current() {
                    let pid = gs.current_player_id()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    room_handle.broadcast(ServerEvent::CardDrawn {
                        player_id: pid,
                        card,
                    });
                }
            }

            broadcast_state_sync(&room_handle.state, &room_handle.tx).await;

            start_turn_timer(room_handle.state.clone(), room_handle.tx.clone(), secs, turn);
        }
    }

    let mut rx = room_handle.subscribe();
    let state = room_handle.state.clone();
    let tx = room_handle.tx.clone();
    let pid = player_id.clone();

    // Actor loop
    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                Some(Ok(msg)) = stream.next() => {
                    match msg {
                        Message::Text(text) => {
                            handle_action(&text, &pid, &state, &tx).await;
                        }
                        Message::Close(_) => break,
                        Message::Ping(b)  => { let _ = session.pong(&b).await; }
                        _ => {}
                    }
                }
                Ok(event) = rx.recv() => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    if session.text(json).await.is_err() { break; }
                }
                else => break,
            }
        }

        // Cleanup on disconnect
        let mut gs = state.lock().await;
        gs.remove_player(&pid);
        let _ = tx.send(ServerEvent::PlayerLeft { username: pid });
    });

    Ok(response)
}
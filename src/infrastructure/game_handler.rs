use std::println;

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::{
    app_state::AppState, core::{game::state::Seconds, game_id::GameId}, infrastructure::room::room_handler::RoomHandle, utils::invite_code::InviteCode,
};

#[derive(Deserialize)]
pub struct CreateGameRequest {
    pub duration_seconds: u64,
}


#[derive(Serialize)]
pub struct CreateGameResponse {
    pub game_id: GameId,
    pub invite_code: String
}

pub async fn create_game(
    state: web::Data<AppState>,
    request: web::Json<CreateGameRequest>,
) -> HttpResponse {
    let duration = request.duration_seconds;

    if !(30..=300).contains(&duration) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "error": "Duration must be between 30 seconds and 5 minutes"
        }));
    }

    let game_id = GameId::new();

    let room = RoomHandle::new_arc(
        game_id.clone(),
        Seconds(duration),
        state.room_shutdown_tx.clone(),
    );

    let invite_code = {
        let mut codes = state.invite_codes.lock().unwrap();
        codes.create_code(game_id.clone())
    }.to_string();

    {
        let mut rooms = state.rooms.lock().unwrap();
        rooms.insert(game_id.clone(), room);
    }

    HttpResponse::Ok().json(CreateGameResponse {
        game_id,
        invite_code,
    })
}

pub async fn get_game_status(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let invite_code = InviteCode(path.into_inner());
    
    let codes = state.invite_codes.lock().unwrap();
    match codes.resolve(&invite_code) {
        Some(game_id) => {
            // Double-check that the room actually still exists in memory
            let rooms = state.rooms.lock().unwrap();
            if rooms.contains_key(game_id) {
                HttpResponse::Ok().json(serde_json::json!({ "exists": true }))
            } else {
                // 410 Gone is the perfect HTTP status for "existed, but is now closed"
                HttpResponse::Gone().json(serde_json::json!({ "error": "Game session has ended" }))
            }
        }
        None => {
            HttpResponse::NotFound().json(serde_json::json!({ "error": "Invalid or expired game invite code" }))
        }
    }
}
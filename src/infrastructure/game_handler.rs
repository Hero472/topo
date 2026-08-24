use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};

use crate::{
    app_state::AppState, core::{game::state::Seconds, game_id::GameId}, infrastructure::room::room_handler::RoomHandle,
};

#[derive(Deserialize)]
pub struct CreateGameRequest {
    pub duration_seconds: u64,
}


#[derive(Serialize)]
pub struct CreateGameResponse {
    pub game_id: GameId,
    pub invite_url: String
}

pub async fn create_game(
    state: web::Data<AppState>,
    request: web::Json<CreateGameRequest>
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

    {
        let mut rooms = state.rooms.lock().unwrap();

        rooms.insert(game_id.clone(), room);
    }

    let invite_url = format!("/game/{}", game_id);

    HttpResponse::Ok().json(CreateGameResponse {
        game_id,
        invite_url,
    })
}
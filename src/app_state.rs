use tokio::sync::mpsc;

use crate::core::game_id::GameId;
use crate::infrastructure::room::room_registry::RoomRegistry;

pub struct AppState {
    pub rooms: RoomRegistry,
    pub room_shutdown_tx: mpsc::UnboundedSender<GameId>,
}
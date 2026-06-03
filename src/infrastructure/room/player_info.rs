use tokio::sync::mpsc;
use crate::{core::player::PlayerIdx, infrastructure::message::GameMessage};

#[derive(Debug)]
pub struct PlayerInfo {
    pub username: String,
    pub tx: mpsc::UnboundedSender<GameMessage>,
    pub player_idx: PlayerIdx
}
use tokio::sync::mpsc;
use crate::{core::player::{PlayerId, PlayerIdx}, infrastructure::message::GameMessage};

#[derive(Debug)]
pub struct PlayerInfo {
    pub username: String,
    pub tx: mpsc::UnboundedSender<GameMessage>,
    pub player_id: PlayerId,
    pub player_idx: PlayerIdx,
    pub connected: bool,
    pub is_ready: bool
}
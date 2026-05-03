use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{Mutex, broadcast};
use serde::Serialize;

use crate::core::game::{board::PlayerBoard, card::Card, scale::Scale, state::GameState};

// ── Shared types ──────────────────────────────────────────────────────────────

pub type SharedRoom    = Arc<Mutex<GameState>>;
pub type RoomRegistry  = Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>;

// ── Broadcast events ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    PlayerJoined  { username: String },
    PlayerLeft    { username: String },
    GameStarted   { current_player_id: String },
    CardDrawn     { player_id: String, card: Card },   // only sent to the drawing player
    CardPlayed    { player_id: String, card: Card },
    ScaleUpdated  { scale_id: usize, completed: bool, player_id: String },
    TurnChanged   { current_player_id: String, seconds: u64 },
    TurnTimeout   { player_id: String },
    GameOver      { winner: String, reason: String },
    StateSync {
        your_board: PlayerBoard,
        opponent_personal_count: usize,
        opponent_personal_top: Option<Card>,
        opponent_side: [Vec<Card>; 4],  // visible but no hand
        opponent_username: String,
        scales: Vec<Scale>,
    },
}

// ── Room handle ───────────────────────────────────────────────────────────────

pub struct RoomHandle {
    pub state: SharedRoom,
    pub tx:    broadcast::Sender<ServerEvent>,
}

impl RoomHandle {
    pub fn new(room_id: String, turn_seconds: u64) -> Self {
        let (tx, _) = broadcast::channel(32);
        Self {
            state: Arc::new(Mutex::new(GameState::new(room_id, [1,2].to_vec(), 4, 13, 5))),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }

    pub fn broadcast(&self, event: ServerEvent) {
        let _ = self.tx.send(event);
    }
}
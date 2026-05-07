use serde::Serialize;

use crate::{core::game::{card::Card, scale::Scale}, infrastructure::views::PlayerBoardView};

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    // ── Lobby ──
    PlayerJoined { player_id: usize, username: String },
    PlayerLeft   { player_id: usize },

    // ── Game lifecycle ──
    GameStarted { current_player_id: usize, turn_seconds: u64 },

    // ── Private: drawn card (only to the player who drew) ──
    CardDrawn {
        player_id: usize,
        card: Option<Card>,
    },

    // ── Public: a card was placed onto a scale ──
    CardPlayedOnScale {
        player_id: usize,
        card: Card,
        scale_id: usize,
        completed: bool,     // true when scale reaches queen
    },

    // ── Public: a card was placed onto a side stack ──
    CardPlacedOnSide {
        player_id: usize,
        card: Card,
        stack: usize,        // 0..3
    },

    // ── Public: scale completion notice ──
    ScaleCompleted {
        scale_id: usize,
        by_player: usize,
    },

    // ── Private: opponent’s visible state changed ──
    OpponentUpdate {
        player_idx: usize,
        personal_count: usize,
        personal_top: Option<Card>,
        side: [Vec<Card>; 4],
    },

    // ── Turn ended ──
    TurnEnded {
        next_player_id: usize,
        turn_seconds: u64,
    },

    // ── Game over ──
    GameOver {
        winner_id: usize,
        reason: String,
    },

    TurnTimedOut {
        player_id: usize,
        next_player_id: usize,
        turn_seconds: u64,
    },

    // ── Full state sync (join/reconnect) ──
    FullState {
        your_board: PlayerBoardView,
        your_turn: bool,
        opponent: OpponentView,
        scales: Vec<Scale>,
        turn_seconds_remaining: u64,
    },
}
#[derive(Debug, Clone, Serialize)]
pub struct OpponentView {
    pub player_idx: usize,
    pub username: String,
    pub personal_count: usize,
    pub personal_top: Option<Card>,
    pub side: [Vec<Card>; 4],
}
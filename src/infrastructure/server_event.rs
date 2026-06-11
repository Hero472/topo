use serde::Serialize;

use crate::{
    core::{
        game::{card::Card, deck::DeckColor, scale::Scale, state::Seconds},
        game_index::{ScaleIdx, StackIdx},
        player::{PlayerId, PlayerIdx}
    }, 
    infrastructure::{
        error::{ErrorCode, ErrorDetails},
        views::PlayerBoardView
    }
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    // ── Lobby ──
    PlayerJoined {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        username: String 
    },
    PlayerLeft {
        player_id: PlayerId,
        player_idx: PlayerIdx 
    },

    // ── Game lifecycle ──
    GameStarted {
        current_player_id: PlayerId,
        current_player_idx: PlayerIdx,
        turn_seconds: Seconds,
    },

    // ── Private: drawn card (only to the player who drew) ──
    CardDrawn {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        card: Option<Card>,
    },

    // ── Public: a card was placed onto a scale ──
    CardPlayedOnScale {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        card: Card,
        scale_idx: ScaleIdx,
        completed: bool,
    },

    // ── Public: a card was placed onto a side stack ──
    CardPlacedOnSide {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        card: Card,
        stack_idx: StackIdx,
    },

    // ── Public: scale completion notice ──
    ScaleCompleted {
        scale_idx: ScaleIdx,
        by_player_id: PlayerId,
        by_player_idx: PlayerIdx,
    },

    // ── Private: opponent’s visible state changed ──
    OpponentUpdate {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        personal_count: usize,
        personal_top: Option<Card>,
        side: [Vec<Card>; 4],
    },

    // ── Turn ended ──
    TurnEnded {
        next_player_id: PlayerId,
        next_player_idx: PlayerIdx,
        turn_seconds: Seconds,
        timed_out_player_id: Option<PlayerId>,
        timed_out_player_idx: Option<PlayerIdx>,
    },

    // ── Game over ──
    GameOver {
        winner_id: PlayerId,
        winner_idx: PlayerIdx,
        reason: String,
    },

    // ── Full state sync (join/reconnect) ──
    FullState {
        your_board: PlayerBoardView,
        your_turn: bool,
        opponent: OpponentView,
        scales: Vec<Scale>,
        dealer_top: Option<DeckColor>,
        dealer_count: usize,
        turn_seconds_remaining: Seconds,
    },

    PlayerDisconnected {
        player_id: PlayerId,
        player_idx: PlayerIdx,
    },

    PlayerReconnected {
        player_id: PlayerId,
        player_idx: PlayerIdx,
    },

    Error {
        code: ErrorCode,
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<ErrorDetails>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpponentView {
    pub player_idx: PlayerIdx,
    pub username: String,
    pub hand_count: usize,
    pub personal_count: usize,
    pub personal_top: Option<Card>,
    pub side: [Vec<Card>; 4],
}
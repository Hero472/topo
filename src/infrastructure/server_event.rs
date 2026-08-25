use serde::Serialize;

use crate::{
    core::{
        game::{card::Card, deck::DeckColor, scale::Scale, state::Seconds}, game_id::GameId, game_index::{ScaleIdx, StackIdx}, player::{PlayerId, PlayerIdx}
    }, infrastructure::{
        error::{ErrorCode, ErrorDetails}, views::{PersonalPileView, PlayerBoardView}
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

    PlayerReady { player_id: PlayerId },

    WaitingForPlayer {
        game_id: GameId,
    },

    // ── Game lifecycle ──
    GameStarted {
        current_player_id: PlayerId,
        current_player_idx: PlayerIdx,
        turn_seconds: Seconds,
    },

    // ── Private: drawn cards (only to the player who drew) ──
    HandRefill {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        cards: Vec<Card>,
        turn_seconds_remaining: Seconds,
    },

    // ── Public: When personal is used needs to show the next ──
    PersonalPileUpdated {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        personal_view: PersonalPileView,
    },

    // ── Public: a card was placed onto a scale ──
    CardPlayedOnScale {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        card: Card,
        scale_idx: ScaleIdx,
        completed: bool,
        turn_seconds_remaining: Seconds,
    },

    // ── Public: a card was placed onto a side stack ──
    CardPlacedOnSide {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        card: Card,
        stack_idx: StackIdx,
        turn_seconds_remaining: Seconds,
    },

    // ── Public: scale completion notice ──
    ScaleCompleted {
        scale_idx: ScaleIdx,
        by_player_id: PlayerId,
        by_player_idx: PlayerIdx,
    },

    GameStarting {
        seconds_remaining: u8,
    },

    // ── Private: opponent’s visible state changed ──
    OpponentUpdate {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        personal_count: usize,
        personal_top: Option<Card>,
        hand: Vec<Card>,
        side: [Vec<Card>; 4],
        turn_seconds_remaining: Seconds,
    },

    OpponentLeft,

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
        player_id: PlayerId,
        your_board: PlayerBoardView,
        your_turn: bool,
        opponent: OpponentView,
        scales: Vec<Option<Scale>>,
        dealer_preview: Vec<DeckColor>,
        dealer_count: usize,
        turn_seconds_remaining: Seconds,
    },

    PlayerDisconnected {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        grace_period_seconds: u64
    },

    PlayerReconnected {
        player_id: PlayerId,
        player_idx: PlayerIdx,
        turn_seconds_remaining: Seconds,
    },

    PlayAgain { player_id: PlayerId },

    PlayAgainRequested { player_id: PlayerId },

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
    pub hand: Vec<Card>,
    pub personal_count: usize,
    pub personal_top: Option<Card>,
    pub side: [Vec<Card>; 4],
}
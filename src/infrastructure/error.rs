use serde::Serialize;

use crate::core::player::PlayerId;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotYourTurn,
    InvalidMove,
    CardNotFound,
    StackFull,
    GameNotStarted,
    GameOver
}

/// Context that helps the frontend (or devtools) understand the error.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErrorDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<PlayerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,       // JSON string of the attempted action
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,      // e.g. "5_diamonds"
}
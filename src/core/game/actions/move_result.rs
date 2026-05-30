use serde::{Serialize, Deserialize};

/// Successful outcomes of a move.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MoveSuccess {
    ScalePlaced { scale_id: usize, completed: bool },
    ScaleOpened { scale_id: usize },
    Success,
    TurnEnded,
    GameWon { winner_id: usize },
}

impl MoveSuccess {
    pub fn turn_ended(&self) -> bool {
        matches!(self, MoveSuccess::TurnEnded | MoveSuccess::GameWon { .. })
    }
}

/// Domain‑level reasons a move failed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MoveError {
    DoesNotFit,
    NotYourTurn,
    NotAllowed,
    InvalidIndex { kind: String },
}
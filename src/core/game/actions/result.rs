use serde::{Deserialize, Serialize};

use crate::core::game::state::GameState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlayResult {
    /// Card was placed on an existing scale.
    ScalePlaced {
        scale_id: usize,
        /// True if the scale became completed after this placement.
        completed: bool,
    },
    /// A new scale was opened (only with an Ace).
    ScaleOpened {
        scale_id: usize,
    },
    /// A non‑scale action succeeded (draw, move to side, etc.).
    Success,
    /// The turn was ended (after a terminal move like MoveToSide).
    TurnEnded,
    /// The game finished because a player met the win condition.
    GameWon {
        winner_id: usize,
    },

    // ── Error variants ──
    /// The card cannot be placed on the chosen scale/position.
    DoesNotFit,
    /// Attempted to act when it’s not the player’s turn.
    NotYourTurn,
    /// The action is disallowed in the current game/turn phase.
    NotAllowed,
    /// Requested card/stack index is out of bounds.
    InvalidIndex {
        /// Which index was wrong, e.g., "hand_index", "stack", "scale_id".
        kind: String,
    },
}

impl PlayResult {
    /// Whether the turn ended (TurnEnded or GameWon).
    pub fn turn_ended(&self) -> bool {
        matches!(self, PlayResult::TurnEnded | PlayResult::GameWon { .. })
    }

    pub fn is_turn_ended(&self) -> bool {
        self.turn_ended()   // reuses the existing method
    }

    pub fn is_success(&self) -> bool {
        matches!(self, PlayResult::Success)
    }

    /// Returns the player who should move next if the turn ended.
    pub fn next_player(&self, state: &GameState) -> Option<usize> {
        match self {
            PlayResult::TurnEnded => {
                state.players.get(state.current_turn).map(|p| p.player_idx)
            }
            PlayResult::GameWon { .. } => None,
            _ => None,
        }
    }
}
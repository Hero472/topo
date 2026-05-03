use serde::{Deserialize, Serialize};

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
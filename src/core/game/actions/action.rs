use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Draw,
    /// Open a **new** scale using a card from the hand.
    OpenScale {
        hand_idx: usize
    },
    /// Play a card from hand onto an **existing** scale.
    PlayHand {
        hand_idx: usize,
        scale_idx: usize
    },
    /// Play the top card of the personal pile onto an existing scale.
    PlayPersonal {
        scale_idx: usize
    },
    /// Play the top card of a side stack onto an existing scale.
    PlaySide {
        stack:    usize,
        scale_idx: usize,
    },
    /// Move a card from hand to one of the side stacks.
    MoveToSide {
        hand_idx: usize,
        stack:      usize,
    },
    /// Move the **top personal card** (only a King) to a side stack.
    MovePersonalToSide {
        stack_idx: usize,
    },
}
use serde::{Serialize, Deserialize};

use crate::core::game_index::{HandIdx, ScaleIdx, StackIdx};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Draw,
    /// Open a new scale using a card from the hand.
    OpenScale {
        hand_idx: HandIdx
    },
    /// Play a card from hand onto an existing scale.
    PlayHand {
        hand_idx: HandIdx,
        scale_idx: ScaleIdx
    },
    /// Play the top card of the personal pile onto an existing scale.
    PlayPersonal {
        scale_idx: ScaleIdx
    },
    /// Play the top card of a side stack onto an existing scale.
    PlaySide {
        stack_idx: StackIdx,
        scale_idx: ScaleIdx,
    },
    /// Move a card from hand to one of the side stacks.
    MoveToSide {
        hand_idx: HandIdx,
        stack_idx: StackIdx,
    },
    /// Move the top personal card (only a King) to a side stack.
    MovePersonalToSide {
        stack_idx: StackIdx,
    },
}
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    PlayHand { index: usize },
    PlayPersonal,
    PlaySide { stack: usize },

    MoveToSide { hand_index: usize, stack: usize },

    Draw,
}
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerId(pub Uuid); // external user id

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlayerIdx(pub usize); // internal 0 or 1

impl PlayerIdx {
    pub fn as_usize(self) -> usize { self.0 }
}

impl From<PlayerIdx> for usize {
    fn from(idx: PlayerIdx) -> usize {
        idx.0
    }
}

impl From<usize> for PlayerIdx {
    fn from(n: usize) -> Self {
        PlayerIdx(n)
    }
}

impl PlayerId {
    pub fn to_idx(&self, mapping: &HashMap<PlayerId, PlayerIdx>) -> Option<PlayerIdx> {
        mapping.get(self).copied()
    }
}

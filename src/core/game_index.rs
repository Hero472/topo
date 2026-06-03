use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HandIdx(pub usize);

impl From<HandIdx> for usize {
    fn from(idx: HandIdx) -> usize {
        idx.0
    }
}

impl From<usize> for HandIdx {
    fn from(n: usize) -> Self {
        HandIdx(n)
    }
}

impl HandIdx { pub fn as_usize(self) -> usize { self.0 } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScaleIdx(pub usize);

impl From<ScaleIdx> for usize {
    fn from(idx: ScaleIdx) -> usize {
        idx.0
    }
}

impl From<usize> for ScaleIdx {
    fn from(n: usize) -> Self {
        ScaleIdx(n)
    }
}

impl ScaleIdx { pub fn as_usize(self) -> usize { self.0 } }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StackIdx(pub usize);

impl From<StackIdx> for usize {
    fn from(idx: StackIdx) -> usize {
        idx.0
    }
}

impl From<usize> for StackIdx {
    fn from(n: usize) -> Self {
        StackIdx(n)
    }
}

impl StackIdx { pub fn as_usize(self) -> usize { self.0 } }
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Seconds(pub u64);

impl Seconds {
    pub fn as_duration(self) -> Duration {
        Duration::from_secs(self.0)
    }

    pub fn zero() -> Self {
        Self(0)
    }

    pub fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl From<u64> for Seconds {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Seed(pub u64);

impl Seed {
    pub fn as_usize(self) -> u64 { self.0 }
}
// src/core/invite_code.rs

use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A short, shareable code like "xk7m2p"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InviteCode(pub String);

// Ambiguous characters removed: 0/O, 1/l/I
const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
const CODE_LENGTH: usize = 6; // 31^6 ≈ 887M possibilities

impl InviteCode {
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let code: String = (0..CODE_LENGTH)
            .map(|_| {
                let idx = rng.random_range(0..ALPHABET.len());
                ALPHABET[idx] as char
            })
            .collect();
        Self(code)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for InviteCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return write!(f, "{}", self.0)
    }
}
use serde::Serialize;
use crate::core::game::{card::Card, deck::DeckColor};

/// What a player can see about their own personal pile.
#[derive(Debug, Clone, Serialize)]
pub struct PersonalPileView {
    pub count: usize,
    pub top: Option<Card>,
    pub colors: Vec<DeckColor>,
}

/// The full player board that a player sees about themselves.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerBoardView {
    pub player_idx: usize,
    pub personal: PersonalPileView,
    pub side: [Vec<Card>; 4],         // side stacks are all face‑up
    pub hand: Vec<Card>,              // the player's own hand
}
use serde::Serialize;
use crate::core::game::{card::Card, deck::DeckColor};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PersonalPileView {
    pub count: usize,
    pub top: Option<Card>,
    pub colors: Vec<DeckColor>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlayerBoardView {
    pub player_idx: usize,
    pub personal: PersonalPileView,
    pub side: [Vec<Card>; 4],
    pub hand: Vec<Card>,
}
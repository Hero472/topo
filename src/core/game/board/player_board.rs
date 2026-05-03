use serde::{Deserialize, Serialize};

use crate::core::game::card::Card;

const SIDE_STACKS: usize = 4;
pub const NORMAL_HAND_SIZE: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerBoard {
    pub player_idx: usize,
    pub personal:  Vec<Card>,
    pub side:      [Vec<Card>; SIDE_STACKS],
    pub hand:      Vec<Card>,
}

impl PlayerBoard {
    pub fn new(player_idx: usize) -> Self {
        Self {
            player_idx,
            personal: vec![],
            side: Default::default(),
            hand: vec![],
        }
    }

    /// Sets the personal pile once (normally during the initial deal).
    pub fn set_personal(&mut self, cards: Vec<Card>) {
        self.personal = cards;
    }

    /// Returns the top card of the personal pile (visible to all players).
    pub fn personal_top(&self) -> Option<&Card> {
        self.personal.last()
    }

    /// Number of cards remaining in the personal pile.
    pub fn personal_len(&self) -> usize {
        self.personal.len()
    }

    /// Removes and returns the top card of the personal pile.
    pub fn pop_personal(&mut self) -> Option<Card> {
        self.personal.pop()
    }

    /// Adds a card to the hand (used during the draw phase).
    /// Can temporarily increase hand size beyond [`NORMAL_HAND_SIZE`].
    pub fn draw_to_hand(&mut self, card: Card) {
        self.hand.push(card);
    }

    /// Current number of cards in hand.
    pub fn hand_len(&self) -> usize {
        self.hand.len()
    }

    /// Removes the card at the given index from the hand.
    /// Returns `None` if the index is out of bounds.
    pub fn take_from_hand(&mut self, index: usize) -> Option<Card> {
        if index >= self.hand.len() { return None; }
        Some(self.hand.remove(index))
    }

    /// Places `card` on top of the specified side stack.
    /// The stack index must be in `0..SIDE_STACKS`.
    /// Returns `true` on success.
    pub fn place_on_side(&mut self, stack: usize, card: Card) -> bool {
        if stack >= SIDE_STACKS { return false; }
        self.side[stack].push(card);
        true
    }

    /// Removes and returns the top card of the specified side stack.
    /// Returns `None` if the stack is empty or index invalid.
    pub fn take_from_side(&mut self, stack: usize) -> Option<Card> {
        if stack >= SIDE_STACKS { return None; }
        self.side[stack].pop()
    }

    /// The player wins when both personal pile and hand are completely empty.
    /// (Side stacks do **not** count toward the win condition.)
    pub fn has_won(&self) -> bool {
        self.personal.is_empty() && self.hand.is_empty()
    }
}

#[cfg(test)]

mod tests {
    use super::*;
    use crate::core::game::{card::{Card, Suit}, deck::DeckColor};

    fn card(value: u8) -> Card {
        Card {
            suit: Suit::Hearts,
            value,
            deck: DeckColor::Red,
        }
    }

    fn cards(range: std::ops::RangeInclusive<u8>) -> Vec<Card> {
        range.map(card).collect()
    }

    #[test]
    fn new_board_is_empty() {
        let board = PlayerBoard::new(0);
        assert_eq!(board.player_idx, 0);
        assert!(board.personal.is_empty());
        assert!(board.side.iter().all(|s| s.is_empty()));
        assert!(board.hand.is_empty());
    }

    #[test]
    fn set_personal_replaces_cards() {
        let mut board = PlayerBoard::new(0);
        let initial = cards(1..=13);
        board.set_personal(initial.clone());
        assert_eq!(board.personal, initial);
        assert_eq!(board.personal_len(), 13);
    }

    #[test]
    fn personal_top_returns_last() {
        let mut board = PlayerBoard::new(0);
        board.set_personal(cards(1..=3));
        assert_eq!(board.personal_top().unwrap().value, 3);
    }

    #[test]
    fn personal_top_empty() {
        let board = PlayerBoard::new(0);
        assert!(board.personal_top().is_none());
    }

    #[test]
    fn pop_personal_removes_and_returns_top() {
        let mut board = PlayerBoard::new(0);
        board.set_personal(cards(1..=3));
        let card = board.pop_personal().unwrap();
        assert_eq!(card.value, 3);
        assert_eq!(board.personal_len(), 2);
        assert_eq!(board.personal_top().unwrap().value, 2);
    }

    #[test]
    fn pop_personal_empty_returns_none() {
        let mut board = PlayerBoard::new(0);
        assert!(board.pop_personal().is_none());
    }

    #[test]
    fn draw_to_hand_adds_card() {
        let mut board = PlayerBoard::new(0);
        board.draw_to_hand(card(5));
        assert_eq!(board.hand_len(), 1);
        assert_eq!(board.hand[0].value, 5);
    }

    #[test]
    fn hand_can_exceed_normal_size() {
        let mut board = PlayerBoard::new(0);
        // Simulate start of turn: already have 5, draw one -> 6.
        for _ in 0..5 {
            board.draw_to_hand(card(1));
        }
        assert_eq!(board.hand_len(), 5);
        board.draw_to_hand(card(2));
        assert_eq!(board.hand_len(), 6);
        // Still works fine.
    }

    #[test]
    fn take_from_hand_valid_index() {
        let mut board = PlayerBoard::new(0);
        for v in 1..=5 {
            board.draw_to_hand(card(v));
        }
        let taken = board.take_from_hand(2).unwrap();
        assert_eq!(taken.value, 3); // 0-based: 0→1, 1→2, 2→3
        assert_eq!(board.hand_len(), 4);
    }

    #[test]
    fn take_from_hand_out_of_bounds_returns_none() {
        let mut board = PlayerBoard::new(0);
        board.draw_to_hand(card(1));
        assert!(board.take_from_hand(1).is_none()); // only index 0 valid
        assert!(board.take_from_hand(5).is_none());
        assert_eq!(board.hand_len(), 1);
    }

    #[test]
    fn place_on_side_valid_stack() {
        let mut board = PlayerBoard::new(0);
        assert!(board.place_on_side(0, card(1)));
        assert_eq!(board.side[0].len(), 1);
        assert_eq!(board.side[0][0].value, 1);
        // Other stacks unaffected
        assert!(board.side[1..].iter().all(|s| s.is_empty()));
    }

    #[test]
    fn place_on_side_invalid_stack_fails() {
        let mut board = PlayerBoard::new(0);
        assert!(!board.place_on_side(4, card(1)));
        assert!(!board.place_on_side(10, card(1)));
        // No side stack modified
        assert!(board.side.iter().all(|s| s.is_empty()));
    }

    #[test]
    fn take_from_side_returns_top() {
        let mut board = PlayerBoard::new(0);
        board.place_on_side(2, card(5));
        board.place_on_side(2, card(7));
        let taken = board.take_from_side(2).unwrap();
        assert_eq!(taken.value, 7);
        assert_eq!(board.side[2].len(), 1);
        assert_eq!(board.side[2][0].value, 5);
    }

    #[test]
    fn take_from_side_empty_stack_returns_none() {
        let mut board = PlayerBoard::new(0);
        assert!(board.take_from_side(0).is_none());
    }

    #[test]
    fn take_from_side_invalid_stack_returns_none() {
        let mut board = PlayerBoard::new(0);
        assert!(board.take_from_side(4).is_none());
    }

    #[test]
    fn has_won_true_only_when_personal_and_hand_empty() {
        let mut board = PlayerBoard::new(0);
        // Empty personal and empty hand -> win
        assert!(board.has_won());

        // Add a card to hand -> false
        board.draw_to_hand(card(1));
        assert!(!board.has_won());

        // Remove it, add to personal -> false
        board.take_from_hand(0);
        board.set_personal(cards(1..=1));
        assert!(!board.has_won());

        // Clear personal -> true
        board.pop_personal();
        assert!(board.has_won());
    }

    #[test]
    fn side_stacks_do_not_affect_win() {
        let mut board = PlayerBoard::new(0);
        board.place_on_side(0, card(1));
        // Still true because personal and hand are empty
        assert!(board.has_won());
    }
}
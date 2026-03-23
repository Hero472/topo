use serde::{Deserialize, Serialize};
use crate::game::card::Card;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerBoard {
    pub player_id: String,
    pub username:  String,
    pub personal:  Vec<Card>, // 13 cards
    pub side:      [Vec<Card>; 4],
    pub hand:      Vec<Card>,
}

impl PlayerBoard {
    pub fn new(player_id: String, username: String) -> Self {
        Self {
            player_id,
            username,
            personal: vec![],
            side: [vec![], vec![], vec![], vec![]],
            hand: vec![],
        }
    }

    pub fn personal_top(&self) -> Option<&Card> {
        self.personal.last()
    }

    pub fn has_won(&self) -> bool {
        self.personal.is_empty() && self.hand.is_empty()
    }

    pub fn pop_personal(&mut self) -> Option<Card> {
        self.personal.pop()
    }

    pub fn take_from_hand(&mut self, index: usize) -> Option<Card> {
        if index >= self.hand.len() { return None; }
        Some(self.hand.remove(index))
    }

    pub fn take_from_side(&mut self, stack: usize) -> Option<Card> {
        if stack >= 4 { return None; }
        self.side[stack].pop()
    }

    pub fn place_on_side(&mut self, stack: usize, card: Card) -> bool {
        if stack >= 4 { return false; }
        self.side[stack].push(card);
        true
    }

    pub fn add_to_hand(&mut self, card: Card) {
        self.hand.push(card);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::card::{Card, Suit, DeckColor};

    fn card(value: u8) -> Card {
        Card {
            suit: Suit::Hearts,
            value,
            deck: DeckColor::Red,
        }
    }

    #[test]
    fn new_board_is_empty() {
        let board = PlayerBoard::new("1".into(), "Joe".into());

        assert_eq!(board.player_id, "1");
        assert_eq!(board.username, "Joe");
        assert!(board.personal.is_empty());
        assert!(board.hand.is_empty());
        assert!(board.side.iter().all(|s| s.is_empty()));
    }

    #[test]
    fn personal_top_and_pop() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        board.personal.push(card(1));
        board.personal.push(card(2));

        assert_eq!(board.personal_top().unwrap().value, 2);

        let popped = board.pop_personal().unwrap();
        assert_eq!(popped.value, 2);

        assert_eq!(board.personal_top().unwrap().value, 1);
    }

    #[test]
    fn take_from_hand_valid_and_invalid() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        board.add_to_hand(card(1));
        board.add_to_hand(card(2));

        let c = board.take_from_hand(0).unwrap();
        assert_eq!(c.value, 1);
        assert_eq!(board.hand.len(), 1);

        // invalid index
        assert!(board.take_from_hand(5).is_none());
    }

    #[test]
    fn take_from_side_valid_and_invalid() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        board.side[0].push(card(1));
        board.side[0].push(card(2));

        let c = board.take_from_side(0).unwrap();
        assert_eq!(c.value, 2);

        // empty stack now
        board.take_from_side(0);
        assert!(board.take_from_side(0).is_none());

        // invalid index
        assert!(board.take_from_side(5).is_none());
    }

    #[test]
    fn place_on_side_valid_and_invalid() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        assert!(board.place_on_side(0, card(1)));
        assert_eq!(board.side[0].len(), 1);

        // invalid index
        assert!(!board.place_on_side(5, card(2)));
    }

    #[test]
    fn add_to_hand_works() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        board.add_to_hand(card(1));
        board.add_to_hand(card(2));

        assert_eq!(board.hand.len(), 2);
    }

    #[test]
    fn has_won_only_when_empty() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        // Initially empty → technically true
        assert!(board.has_won());

        board.personal.push(card(1));
        assert!(!board.has_won());

        board.personal.clear();
        board.hand.push(card(2));
        assert!(!board.has_won());

        board.hand.clear();
        assert!(board.has_won());
    }

    #[test]
    fn side_stacks_are_independent() {
        let mut board = PlayerBoard::new("1".into(), "Joe".into());

        board.place_on_side(0, card(1));
        board.place_on_side(1, card(2));

        assert_eq!(board.side[0].len(), 1);
        assert_eq!(board.side[1].len(), 1);
        assert_eq!(board.side[2].len(), 0);
    }
}
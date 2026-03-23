use serde::{Deserialize, Serialize};
use rand::seq::SliceRandom;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Suit { Hearts, Diamonds, Clubs, Spades }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeckColor { Red, Blue }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Card {
    pub suit:   Suit,
    pub value:  u8,         // 1=Ace, 11=Jack, 12=Queen, 13=King
    pub deck:   DeckColor,  // which of the two decks this came from
}

impl Card {
    pub fn display_value(&self) -> &'static str {
        match self.value {
            1  => "A",
            2  => "2",
            3  => "3", 
            4  => "4", 
            5  => "5", 
            6  => "6",
            7  => "7", 
            8  => "8", 
            9  => "9", 
            10 => "10", 
            11 => "J",
            12 => "Q",
            13 => "K",
            _  => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    pub fn double() -> Self {
        let mut cards = Vec::with_capacity(104);

        for color in [DeckColor::Red, DeckColor::Blue] {
            for suit in [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades] {
                for value in 1..=13 {
                    cards.push(Card { suit, value, deck: color });
                }
            }
        }

        Self { cards }
    }

    pub fn shuffle(&mut self) {
        let mut rng = rand::rng();
        self.cards.shuffle(&mut rng);
    }

    pub fn shuffle_with_seed(&mut self, seed: u64) {
        use rand::{SeedableRng, rngs::StdRng};

        let mut rng = StdRng::seed_from_u64(seed);
        self.cards.shuffle(&mut rng);
    }

    pub fn deal(&mut self, n: usize) -> Vec<Card> {
        self.cards.drain(..n.min(self.cards.len())).collect()
    }

    pub fn draw_one(&mut self) -> Option<Card> {
        self.cards.pop()
    }

    pub fn remaining(&self) -> usize {
        self.cards.len()
    }

    pub fn add_bottom(&mut self, mut cards: Vec<Card>) {
        cards.append(&mut self.cards);
        self.cards = cards;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_deck_has_104_cards() {
        let deck = Deck::double();
        assert_eq!(deck.remaining(), 104);
    }

    #[test]
    fn double_deck_has_all_values() {
        let deck = Deck::double();

        let mut counts = [0; 14]; // index 1..13

        for card in deck.cards.iter() {
            counts[card.value as usize] += 1;
        }

        for value in 1..=13 {
            assert_eq!(counts[value], 8, "Value {} should appear 8 times", value);
        }
    }

    #[test]
    fn draw_one_reduces_size() {
        let mut deck = Deck::double();

        let _ = deck.draw_one();
        assert_eq!(deck.remaining(), 103);
    }

    #[test]
    fn draw_until_empty() {
        let mut deck = Deck::double();

        for _ in 0..104 {
            assert!(deck.draw_one().is_some());
        }

        assert!(deck.draw_one().is_none());
        assert_eq!(deck.remaining(), 0);
    }

    #[test]
    fn deal_removes_correct_amount() {
        let mut deck = Deck::double();

        let hand = deck.deal(10);

        assert_eq!(hand.len(), 10);
        assert_eq!(deck.remaining(), 94);
    }

    #[test]
    fn deal_more_than_available() {
        let mut deck = Deck::double();

        let hand = deck.deal(200);

        assert_eq!(hand.len(), 104);
        assert_eq!(deck.remaining(), 0);
    }

    #[test]
    fn shuffle_changes_order() {
        let deck1 = Deck::double();
        let mut deck2 = Deck::double();

        deck2.shuffle();

        // Very small chance this fails randomly, but acceptable
        assert_ne!(deck1.cards, deck2.cards);
    }

    #[test]
    fn shuffle_with_seed_is_deterministic() {
        let mut deck1 = Deck::double();
        let mut deck2 = Deck::double();

        deck1.shuffle_with_seed(42);
        deck2.shuffle_with_seed(42);

        assert_eq!(deck1.cards, deck2.cards);
    }

    #[test]
    fn shuffle_with_different_seeds_is_different() {
        let mut deck1 = Deck::double();
        let mut deck2 = Deck::double();

        deck1.shuffle_with_seed(1);
        deck2.shuffle_with_seed(2);

        assert_ne!(deck1.cards, deck2.cards);
    }

    #[test]
    fn add_bottom_places_cards_under_deck() {
        let mut deck = Deck::double();

        let bottom_cards = vec![
            Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red },
            Card { suit: Suit::Spades, value: 13, deck: DeckColor::Blue },
        ];

        let original_top = deck.cards.last().cloned();

        deck.add_bottom(bottom_cards.clone());

        // Top should stay the same
        assert_eq!(deck.cards.last(), original_top.as_ref());

        // Bottom should contain inserted cards
        assert_eq!(deck.cards[0], bottom_cards[0]);
        assert_eq!(deck.cards[1], bottom_cards[1]);
    }
}
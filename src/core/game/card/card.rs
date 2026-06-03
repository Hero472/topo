use serde::{Deserialize, Serialize};
use crate::core::game::deck::DeckColor;

use super::suit::Suit;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Card {
    pub suit: Suit,
    pub value: u8,
    pub deck: DeckColor,
}

impl Card {
    pub fn display_value(&self) -> &'static str {
        match self.value {
            1 => "A",
            2 => "2",
            3 => "3",
            4 => "4",
            5 => "5",
            6 => "6",
            7 => "7",
            8 => "8",
            9 => "9",
            10 => "10",
            11 => "J",
            12 => "Q",
            13 => "K",
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json;

    fn sample_card() -> Card {
        Card {
            suit: Suit::Hearts,
            value: 1,
            deck: DeckColor::Red,
        }
    }

    #[test]
    fn display_value_ace() {
        let card = Card { value: 1, ..sample_card() };
        assert_eq!(card.display_value(), "A");
    }

    #[test]
    fn display_value_numeric() {
        for (val, expected) in (2..=10).zip(["2","3","4","5","6","7","8","9","10"]) {
            let card = Card { value: val, ..sample_card() };
            assert_eq!(card.display_value(), expected, "failed for value {}", val);
        }
    }

    #[test]
    fn display_value_face_cards() {
        let j = Card { value: 11, ..sample_card() };
        assert_eq!(j.display_value(), "J");
        let q = Card { value: 12, ..sample_card() };
        assert_eq!(q.display_value(), "Q");
        let k = Card { value: 13, ..sample_card() };
        assert_eq!(k.display_value(), "K");
    }

    #[test]
    #[should_panic]
    fn display_value_invalid_panics() {
        let invalid = Card { value: 0, ..sample_card() };
        invalid.display_value(); // hits unreachable!()
    }

    #[test]
    fn test_equality() {
        let a = Card { suit: Suit::Spades, value: 5, deck: DeckColor::Blue };
        let b = Card { suit: Suit::Spades, value: 5, deck: DeckColor::Blue };
        assert_eq!(a, b);
        let c = Card { value: 6, ..a };
        assert_ne!(a, c);
    }

    #[test]
    fn test_copy_clone() {
        let original = sample_card();
        let copied = original;         // Copy because Card is Copy
        let cloned = original.clone(); // Clone also works
        assert_eq!(original, copied);
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let card = Card {
            suit: Suit::Clubs,
            value: 10,
            deck: DeckColor::Blue,
        };
        let json = serde_json::to_string(&card).expect("serialization failed");
        let deserialized: Card = serde_json::from_str(&json).expect("deserialization failed");
        assert_eq!(card, deserialized);
    }
}
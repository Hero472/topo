use serde::{Deserialize, Serialize};
use crate::core::game::card::Card;
use crate::core::game::actions::PlayResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scale {
    pub id:    usize,
    pub cards: Vec<Card>,
}

impl Scale {
    pub fn new(id: usize) -> Self {
        Self { id, cards: vec![] }
    }

    fn next_value(&self) -> u8 {
        self.cards.len() as u8 + 1
    }

    pub fn accepts(&self, card: &Card) -> bool {

        if self.is_complete() {
            return false;
        }

        if self.cards.is_empty() {
            return card.value == 1;
        }

        let expected = self.next_value();

        if card.value == 13 {
            // King is a wildcard, but cannot be placed directly on another King
            if let Some(top) = self.cards.last() {
                if top.value == 13 {
                    return false;
                }
            }
            return true;
        }

        card.value == expected
    }

    pub fn is_complete(&self) -> bool {
        self.cards.len() == 12
    }

    pub fn push(&mut self, card: Card) -> PlayResult {
        if !self.accepts(&card) {
            return PlayResult::DoesNotFit;
        }

        self.cards.push(card);

        PlayResult::ScalePlaced {
            scale_id: self.id,
            completed: self.is_complete(),
        }
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

    #[test]
    fn cannot_start_with_king() {
        let mut scale = Scale::new(0);
        assert!(matches!(scale.push(card(13)), PlayResult::DoesNotFit));
    }

    #[test]
    fn accepts_normal_sequence() {
        let mut scale = Scale::new(0);
        assert!(matches!(scale.push(card(1)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(2)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(3)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(4)), PlayResult::ScalePlaced { .. }));
    }

    #[test]
    fn king_replaces_missing_value() {
        let mut scale = Scale::new(0);
        assert!(matches!(scale.push(card(1)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(2)), PlayResult::ScalePlaced { .. }));
        // K replaces 3
        assert!(matches!(scale.push(card(13)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(4)), PlayResult::ScalePlaced { .. }));
    }

    #[test]
    fn cannot_stack_kings() {
        let mut scale = Scale::new(0);
        assert!(matches!(scale.push(card(1)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(13)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(13)), PlayResult::DoesNotFit));
    }

    #[test]
    fn rejects_wrong_value() {
        let mut scale = Scale::new(0);
        assert!(matches!(scale.push(card(1)), PlayResult::ScalePlaced { .. }));
        assert!(matches!(scale.push(card(2)), PlayResult::ScalePlaced { .. }));
        // Expected is 3
        assert!(matches!(scale.push(card(5)), PlayResult::DoesNotFit));
    }

    #[test]
    fn full_example_valid() {
        let mut scale = Scale::new(0);
        let sequence = vec![1, 2, 13, 4, 5, 6, 7, 13, 9, 10, 11, 13];
        for v in sequence {
            assert!(
                matches!(scale.push(card(v)), PlayResult::ScalePlaced { .. }),
                "Failed at value {}",
                v
            );
        }
        assert!(scale.is_complete());
    }

    #[test]
    fn second_example_valid() {
        let mut scale = Scale::new(0);
        let sequence = vec![1, 2, 3, 4, 13, 6, 13, 8, 9, 13, 11, 12];
        for v in sequence {
            assert!(
                matches!(scale.push(card(v)), PlayResult::ScalePlaced { .. }),
                "Failed at value {}",
                v
            );
        }
        assert!(scale.is_complete());
    }

    #[test]
    fn completion_flag_is_true_on_last_card() {
        let mut scale = Scale::new(0);
        for v in 1..=11 {
            match scale.push(card(v)) {
                PlayResult::ScalePlaced { completed: false, .. } => {}
                other => panic!("Unexpected result before completion: {:?}", other),
            }
        }
        match scale.push(card(12)) {
            PlayResult::ScalePlaced { completed: true, .. } => {}
            _ => panic!("Expected completion on last card"),
        }
    }

    #[test]
    fn king_can_complete_scale() {
        let mut scale = Scale::new(0);
        for v in 1..=11 {
            scale.push(card(v));
        }
        let result = scale.push(card(13));
        assert!(matches!(result, PlayResult::ScalePlaced { completed: true, .. }));
    }
}
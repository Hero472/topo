use serde::{Deserialize, Serialize};
use crate::game::{card::Card, result::PlayResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scale {
    pub id:    usize,
    pub cards: Vec<Card>,
}

impl Scale {
    pub fn new(id: usize) -> Self {
        Self { id, cards: vec![] }
    }

    pub fn next_value(&self) -> u8 {
        self.cards.len() as u8 + 1
    }

    pub fn accepts(&self, card: &Card) -> bool {
        // Rule 1: Cannot start with King
        if self.cards.is_empty() {
            return card.value != 13;
        }

        // Rule 2: Cannot place King on top of King
        if let Some(top) = self.cards.last() {
            if top.value == 13 && card.value == 13 {
                return false;
            }
        }

        let expected = self.next_value();

        // Rule 3: King can replace the expected value
        if card.value == 13 {
            return true;
        }

        // Rule 4: Otherwise must match expected value
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

        let completed = self.is_complete();

        PlayResult::Ok {
            scale_id: self.id,
            completed,
        }
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
    fn cannot_start_with_king() {
        let mut scale = Scale::new(0);
        assert!(matches!(scale.push(card(13)), PlayResult::DoesNotFit));
    }

    #[test]
    fn accepts_normal_sequence() {
        let mut scale = Scale::new(0);

        assert!(matches!(scale.push(card(1)), PlayResult::Ok { .. }));
        assert!(matches!(scale.push(card(2)), PlayResult::Ok { .. }));
        assert!(matches!(scale.push(card(3)), PlayResult::Ok { .. }));
    }

    #[test]
    fn king_replaces_missing_value() {
        let mut scale = Scale::new(0);

        assert!(matches!(scale.push(card(1)), PlayResult::Ok { .. }));
        assert!(matches!(scale.push(card(2)), PlayResult::Ok { .. }));

        // K replaces 3
        assert!(matches!(scale.push(card(13)), PlayResult::Ok { .. }));

        assert!(matches!(scale.push(card(4)), PlayResult::Ok { .. }));
    }

    #[test]
    fn cannot_stack_kings() {
        let mut scale = Scale::new(0);

        assert!(matches!(scale.push(card(1)), PlayResult::Ok { .. }));
        assert!(matches!(scale.push(card(13)), PlayResult::Ok { .. }));

        assert!(matches!(scale.push(card(13)), PlayResult::DoesNotFit));
    }

    #[test]
    fn rejects_wrong_value() {
        let mut scale = Scale::new(0);

        assert!(matches!(scale.push(card(1)), PlayResult::Ok { .. }));
        assert!(matches!(scale.push(card(2)), PlayResult::Ok { .. }));

        // Expected is 3
        assert!(matches!(scale.push(card(5)), PlayResult::DoesNotFit));
    }

    #[test]
    fn full_example_valid() {
        let mut scale = Scale::new(0);

        let sequence = vec![
            1, 2, 13, 4, 5, 6, 7, 13, 9, 10, 11, 13
        ];

        for v in sequence {
            assert!(
                matches!(scale.push(card(v)), PlayResult::Ok { .. }),
                "Failed at value {}",
                v
            );
        }

        assert!(scale.is_complete());
    }

    #[test]
    fn second_example_valid() {
        let mut scale = Scale::new(0);

        let sequence = vec![
            1, 2, 3, 4, 13, 6, 13, 8, 9, 13, 11, 12
        ];

        for v in sequence {
            assert!(
                matches!(scale.push(card(v)), PlayResult::Ok { .. }),
                "Failed at value {}",
                v
            );
        }

        assert!(scale.is_complete());
    }

    #[test]
    fn completion_flag_is_true_on_last_card() {
        let mut scale = Scale::new(0);

        let sequence = vec![1,2,3,4,5,6,7,8,9,10,11];

        for v in sequence {
            scale.push(card(v));
        }

        // Last card (12 = Queen)
        match scale.push(card(12)) {
            PlayResult::Ok { completed: true, .. } => {}
            _ => panic!("Expected completion on last card"),
        }
    }
}
use crate::core::game::card::Card;
use crate::core::game::scale::scale::Scale;
use crate::core::game::actions::{MoveSuccess, MoveError};
use crate::core::game_index::ScaleIdx;

#[derive(Debug)]
pub struct ScaleManager {
    pub scales: Vec<Scale>,
}

impl ScaleManager {
    pub fn new() -> Self {
        Self {
            scales: vec![],
        }
    }

    pub fn can_place_on_scale(&self, scale_id: ScaleIdx, card: &Card) -> bool {
        let Some(scale) = self.scales.get(scale_id.as_usize()) else {
            return false
        };

        if !scale.accepts(card) {
            return false
        }

        true
    }

    /// Player wants to place a card on an existing scale.
    pub fn place_on_scale(
        &mut self,
        scale_id: ScaleIdx,
        card: Card,
    ) -> Result<MoveSuccess, MoveError> {
        let scale = self.scales.get(scale_id.as_usize()).ok_or(MoveError::DoesNotFit)?;
        if !scale.accepts(&card) {
            return Err(MoveError::DoesNotFit);
        }
        let _ = self.scales[scale_id.as_usize()].push(card);
        let completed = self.scales[scale_id.as_usize()].is_complete();
        Ok(MoveSuccess::ScalePlaced { scale_id, completed })
    }

    /// Player wants to open a new scale (usually with an Ace).
    /// Only succeeds if the card is an Ace.
    pub fn open_scale(&mut self, card: Card) -> Result<MoveSuccess, MoveError> {
        if card.value != 1 {
            return Err(MoveError::DoesNotFit);
        }
        let scale_id = self.scales.len();
        self.scales.push(Scale::new(ScaleIdx(scale_id)));
        let _ = self.scales[scale_id].push(card);
        Ok(MoveSuccess::ScaleOpened { scale_id: ScaleIdx(scale_id) })
    }

    pub fn reset(&mut self) {
        self.scales.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game::{card::{Card, Suit}, deck::DeckColor};

    // Helper to quickly create a card with a given value
    fn card(value: u8) -> Card {
        Card {
            suit: Suit::Hearts,
            value,
            deck: DeckColor::Red,
        }
    }

    // ---------- open_scale ----------

    #[test]
    fn open_scale_with_ace_succeeds() {
        let mut mgr = ScaleManager::new();
        let result = mgr.open_scale(card(1));
        assert_eq!(result, Ok(MoveSuccess::ScaleOpened { scale_id: ScaleIdx(0) }));
        assert_eq!(mgr.scales.len(), 1);
        assert_eq!(mgr.scales[0].cards.len(), 1);
        assert_eq!(mgr.scales[0].cards[0].value, 1);
    }

    #[test]
    fn open_scale_with_non_ace_fails() {
        let mut mgr = ScaleManager::new();
        assert_eq!(mgr.open_scale(card(5)), Err(MoveError::DoesNotFit));
        assert!(mgr.scales.is_empty());
    }

    #[test]
    fn open_multiple_scales_increments_ids() {
        let mut mgr = ScaleManager::new();
        assert_eq!(mgr.open_scale(card(1)), Ok(MoveSuccess::ScaleOpened { scale_id: ScaleIdx(0) }));
        assert_eq!(mgr.open_scale(card(1)), Ok(MoveSuccess::ScaleOpened { scale_id: ScaleIdx(1) }));
        assert_eq!(mgr.open_scale(card(1)), Ok(MoveSuccess::ScaleOpened { scale_id: ScaleIdx(2) }));
        assert_eq!(mgr.scales.len(), 3);
    }

    // ---------- place_on_scale ----------

    #[test]
    fn place_on_valid_scale_succeeds() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1)); // scale 0 has Ace
        let result = mgr.place_on_scale(ScaleIdx(0), card(2));
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: false }));
        assert_eq!(mgr.scales[0].cards.len(), 2);
        assert_eq!(mgr.scales[0].cards[1].value, 2);
    }

    #[test]
    fn place_on_nonexistent_scale_fails() {
        let mut mgr = ScaleManager::new();
        assert_eq!(mgr.place_on_scale(ScaleIdx(0), card(2)), Err(MoveError::DoesNotFit));
        assert!(mgr.scales.is_empty());
    }

    #[test]
    fn place_invalid_card_fails() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1)); // expecting next value 2
        assert_eq!(mgr.place_on_scale(ScaleIdx(0), card(5)), Err(MoveError::DoesNotFit));
        // scale still only has the Ace
        assert_eq!(mgr.scales[0].cards.len(), 1);
    }

    #[test]
    fn place_on_correct_scale_among_many() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1)); // scale 0: Ace
        let _ = mgr.open_scale(card(1)); // scale 1: Ace
        // Place a 2 on scale 1, not scale 0
        let result = mgr.place_on_scale(ScaleIdx(1), card(2));
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(1), completed: false }));
        assert_eq!(mgr.scales[0].cards.len(), 1); // still only Ace
        assert_eq!(mgr.scales[1].cards.len(), 2); // Ace + 2
    }

    #[test]
    fn place_completes_scale_and_flags_completion() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1));
        // Build up to Jack (1..=11)
        for v in 2..=11 {
            assert_eq!(
                mgr.place_on_scale(ScaleIdx(0), card(v)),
                Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: false })
            );
        }
        // Queen (12) should complete the scale
        let result = mgr.place_on_scale(ScaleIdx(0), card(12));
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: true }));
        assert!(mgr.scales[0].is_complete());
    }

    #[test]
    fn place_on_completed_scale_fails() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1));
        for v in 2..=12 {
            let _ = mgr.place_on_scale(ScaleIdx(0), card(v));
        }
        assert!(mgr.scales[0].is_complete());
        // try to place anything
        assert_eq!(mgr.place_on_scale(ScaleIdx(0), card(1)), Err(MoveError::DoesNotFit));
        assert_eq!(mgr.place_on_scale(ScaleIdx(0), card(5)), Err(MoveError::DoesNotFit));
    }

    // ---------- King (13) special behavior ----------

    #[test]
    fn king_wildcard_placed_on_non_empty_scale() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1)); // Ace
        // Place a 2, then a King as wildcard 3
        let _ = mgr.place_on_scale(ScaleIdx(0), card(2));
        let result = mgr.place_on_scale(ScaleIdx(0), card(13)); // should be allowed
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: false }));
        // Now the scale expects 4 (since length is 3: Ace,2,King)
        // Verify that placing 4 works
        assert_eq!(mgr.place_on_scale(ScaleIdx(0), card(4)), Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: false }));
    }

    #[test]
    fn king_after_ace_allowed() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1));
        let result = mgr.place_on_scale(ScaleIdx(0), card(13));
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: false }));
    }

    #[test]
    fn king_after_king_fails() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1));
        let _ = mgr.place_on_scale(ScaleIdx(0), card(13)); // ace -> king
        let result = mgr.place_on_scale(ScaleIdx(0), card(13)); // king after king
        assert_eq!(result, Err(MoveError::DoesNotFit));
    }

    #[test]
    fn king_cannot_complete_scale_if_already_queen() {
        // Completion is exactly 12 cards. If we place a King as the 12th card, it completes.
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1));
        for v in 2..=11 {
            let _ = mgr.place_on_scale(ScaleIdx(0), card(v));
        }
        // place King as 12th card -> completes
        let result = mgr.place_on_scale(ScaleIdx(0), card(13));
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: ScaleIdx(0), completed: true }));
    }

    // ---------- reset ----------

    #[test]
    fn reset_clears_all_scales() {
        let mut mgr = ScaleManager::new();
        let _ = mgr.open_scale(card(1));
        let _ = mgr.open_scale(card(1));
        assert_eq!(mgr.scales.len(), 2);
        mgr.reset();
        assert!(mgr.scales.is_empty());
    }
}
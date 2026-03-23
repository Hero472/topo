use crate::game::card::Card;
use crate::game::scale::Scale;
use crate::game::result::PlayResult;

pub struct ScaleManager {
    pub scales: Vec<Scale>,
    pub discard_pile: Vec<Card>,
}

impl ScaleManager {
    pub fn new() -> Self {
        Self { scales: vec![], discard_pile: vec![] }
    }

    pub fn find_or_open(&mut self, card: &Card) -> Option<usize> {
        for (i, scale) in self.scales.iter().enumerate() {
            if scale.accepts(card) { return Some(i); }
        }
        if card.value == 1 {
            let idx = self.scales.len();
            self.scales.push(Scale::new(idx));
            return Some(idx);
        }
        None
    }

    pub fn place(&mut self, card: Card) -> PlayResult {
        let Some(scale_idx) = self.find_or_open(&card) else {
            return PlayResult::DoesNotFit;
        };

        self.scales[scale_idx].push(card);

        let completed = self.scales[scale_idx].is_complete();

        if completed {
            let cards = self.scales[scale_idx].cards.drain(..);
            self.discard_pile.extend(cards);
        }

        PlayResult::Ok { scale_id: scale_idx, completed }
    }

    pub fn reset(&mut self) {
        self.scales.clear();
        self.discard_pile.clear();
    }
}
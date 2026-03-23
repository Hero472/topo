use crate::game::card::{Card, Deck};

pub struct CardDealer {
    pub draw_pile: Deck,
    pub discard_pile: Vec<Card>,
}

impl CardDealer {
    pub fn new(seed: u64) -> Self {
        let mut draw_pile = Deck::double();
        draw_pile.shuffle_with_seed(seed);
        Self { draw_pile, discard_pile: vec![] }
    }

    /// Draw one card, recycling the discard pile if the draw pile runs low.
    pub fn draw_one(&mut self) -> Option<Card> {
        self.maybe_recycle();
        self.draw_pile.draw_one()
    }

    /// Draw up to `n` cards.
    pub fn draw_up_to(&mut self, n: usize) -> Vec<Card> {
        let mut cards = Vec::with_capacity(n);
        for _ in 0..n {
            self.maybe_recycle();
            let Some(card) = self.draw_pile.draw_one() else { break };
            cards.push(card);
        }
        cards
    }

    pub fn deal(&mut self, n: usize) -> Vec<Card> {
        self.draw_pile.deal(n)
    }

    pub fn return_to_discard(&mut self, cards: impl IntoIterator<Item = Card>) {
        self.discard_pile.extend(cards);
    }

    pub fn remaining(&self) -> usize {
        self.draw_pile.remaining()
    }

    fn maybe_recycle(&mut self) {
        if self.draw_pile.remaining() <= 5 && !self.discard_pile.is_empty() {
            let cards = self.discard_pile.drain(..).collect();
            self.draw_pile.add_bottom(cards);
        }
    }
}
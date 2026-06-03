use crate::core::game::{card::Card, deck::deck::Deck, state::state_types::Seed};

#[derive(Debug)]
pub struct CardDealer {
    pub draw_pile: Deck,
    pub discard_pile: Vec<Card>,
}

impl CardDealer {
    pub fn new(seed: Seed) -> Self {
        let mut draw_pile = Deck::double();
        draw_pile.shuffle_with_seed(seed.as_usize());
        Self {
            draw_pile,
            discard_pile: vec![],
        }
    }

    pub fn deal_initial(
        &mut self,
        num_players: usize,
        personal_count: usize,
        hand_count: usize,
    ) -> (Vec<Vec<Card>>, Vec<Vec<Card>>) {
        let mut personal_piles = Vec::with_capacity(num_players);
        let mut hands = Vec::with_capacity(num_players);

        for _ in 0..num_players {
            personal_piles.push(self.draw_pile.deal(personal_count));
        }
        for _ in 0..num_players {
            hands.push(self.draw_pile.deal(hand_count));
        }
        (personal_piles, hands)
    }

    /// Draw a single card (for the per‑round phase). Automatically recycles
    /// the discard pile when the draw pile is low.
    pub fn draw_one(&mut self) -> Option<Card> {
        self.maybe_recycle();
        self.draw_pile.draw_one()
    }

    pub fn peek(&self) -> Option<&Card> {
        self.draw_pile.cards.last()
    }

    pub fn draw_up_to(&mut self, n: usize) -> Vec<Card> {
        let mut cards = Vec::with_capacity(n);
        for _ in 0..n {
            self.maybe_recycle();
            let Some(card) = self.draw_pile.draw_one() else {
                break;
            };
            cards.push(card);
        }
        cards
    }

    /// Return cards to the discard pile (e.g. from completed scales or player discards).
    pub fn return_to_discard(&mut self, cards: impl IntoIterator<Item = Card>) {
        self.discard_pile.extend(cards);
    }

    /// Number of cards still in the draw pile.
    pub fn remaining(&self) -> usize {
        self.draw_pile.remaining()
    }

    fn maybe_recycle(&mut self) {
        if self.draw_pile.remaining() <= 10 && !self.discard_pile.is_empty() {
            let cards: Vec<Card> = self.discard_pile.drain(..).collect();
            self.draw_pile.add_bottom(cards);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game::{card::{Card, Suit}, deck::DeckColor};

    fn test_card(value: u8) -> Card {
        Card {
            suit: Suit::Hearts,
            value,
            deck: DeckColor::Red,
        }
    }

    #[test]
    fn new_dealer_initial_state() {
        let dealer = CardDealer::new(Seed(42));
        assert_eq!(dealer.remaining(), 104);
        assert!(dealer.discard_pile.is_empty());
    }

    #[test]
    fn draw_one_reduces_remaining() {
        let mut dealer = CardDealer::new(Seed(123));
        let before = dealer.remaining();
        let card = dealer.draw_one();
        assert!(card.is_some());
        assert_eq!(dealer.remaining(), before - 1);
    }

    #[test]
    fn draw_one_returns_none_when_both_piles_empty() {
        let mut dealer = CardDealer::new(Seed(0));
        for _ in 0..104 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 0);
        assert!(dealer.discard_pile.is_empty());
        assert!(dealer.draw_one().is_none());
    }

    #[test]
    fn recycling_triggered_when_draw_low() {
        let mut dealer = CardDealer::new(Seed(0));
        for _ in 0..94 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 10);
        dealer.return_to_discard(vec![test_card(1), test_card(2), test_card(3)]);
        assert_eq!(dealer.discard_pile.len(), 3);
        // now draw one – should trigger recycling because remaining <= 10
        let card = dealer.draw_one();
        assert!(card.is_some());
        // discard should now be empty, and the 3 recycled cards added to draw pile
        assert!(dealer.discard_pile.is_empty());
        // after drawing one, we added 3 from discard and drew 1, so net = previous(10) + 3 - 1 = 12
        assert_eq!(dealer.remaining(), 12);
    }

    #[test]
    fn recycling_not_triggered_when_discard_empty() {
        let mut dealer = CardDealer::new(Seed(0));
        // draw down to 10 cards
        for _ in 0..94 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 10);
        // draw again, no discard to recycle, still works
        let card = dealer.draw_one();
        assert!(card.is_some());
        assert_eq!(dealer.remaining(), 9); // no extra cards added
    }

    #[test]
    fn recycling_does_not_trigger_above_threshold() {
        let mut dealer = CardDealer::new(Seed(0));
        // draw 93 cards, leaving 11
        for _ in 0..93 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 11);
        dealer.return_to_discard(vec![test_card(5)]);
        // draw one – remaining > 10, so no recycle
        dealer.draw_one();
        assert_eq!(dealer.discard_pile.len(), 1); // still there
        assert_eq!(dealer.remaining(), 10); // just drew one
    }

    #[test]
    fn draw_up_to_respects_available_cards() {
        let mut dealer = CardDealer::new(Seed(0));
        // draw 100 cards, leaving 4
        for _ in 0..100 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 4);
        // ask for 10, should only get 4
        let drawn = dealer.draw_up_to(10);
        assert_eq!(drawn.len(), 4);
        assert_eq!(dealer.remaining(), 0);
        // next draw_up_to returns empty
        let drawn2 = dealer.draw_up_to(5);
        assert!(drawn2.is_empty());
    }

    #[test]
    fn draw_up_to_can_trigger_recycling() {
        let mut dealer = CardDealer::new(Seed(0));
        // leave 5 cards
        for _ in 0..99 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 5);
        // add 2 cards to discard
        dealer.return_to_discard(vec![test_card(10), test_card(11)]);
        // draw_up_to(4) – will trigger recycle (remaining 5 <=10 and discard not empty)
        let drawn = dealer.draw_up_to(4);
        assert_eq!(drawn.len(), 4);
        // after recycle: previous 5 + 2 from discard = 7, minus 4 drawn = 3 remaining
        assert_eq!(dealer.remaining(), 3);
        assert!(dealer.discard_pile.is_empty());
    }

    #[test]
    fn deal_initial_does_not_trigger_recycling() {
        let mut dealer = CardDealer::new(Seed(0));
        // Draw down to 10 cards using the normal round-by-round draw.
        for _ in 0..94 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 10);

        // Add cards to discard – they should NOT be recycled during initial deal.
        dealer.return_to_discard(vec![test_card(7), test_card(8)]);

        // Ask for 2 players, each with 4 personal cards and 0 hand cards.
        let (personal_piles, hands) = dealer.deal_initial(2, 4, 0);

        // 2 players × 4 cards = 8 cards dealt.
        assert_eq!(personal_piles.len(), 2);
        assert_eq!(personal_piles[0].len(), 4);
        assert_eq!(personal_piles[1].len(), 4);
        // Discard should remain untouched (recycling did not happen).
        assert_eq!(dealer.discard_pile.len(), 2);
        // Remaining: 10 - 8 = 2 cards.
        assert_eq!(dealer.remaining(), 2);
        // Hands are empty because we asked for 0.
        assert!(hands.iter().all(|h| h.is_empty()));
    }

    #[test]
    fn deal_initial_respects_available_cards() {
        let mut dealer = CardDealer::new(Seed(0));
        // Draw almost everything.
        for _ in 0..100 {
            dealer.draw_one();
        }
        assert_eq!(dealer.remaining(), 4);

        // Ask for 1 player, personal pile of 10, hand of 0 – only 4 cards exist.
        let (personal_piles, _) = dealer.deal_initial(1, 10, 0);
        // The player gets only the remaining 4 cards.
        assert_eq!(personal_piles[0].len(), 4);
        assert_eq!(dealer.remaining(), 0);
    }

    #[test]
    fn return_to_discard_accumulates_cards() {
        let mut dealer = CardDealer::new(Seed(0));
        dealer.return_to_discard(vec![test_card(1), test_card(2)]);
        assert_eq!(dealer.discard_pile.len(), 2);
        dealer.return_to_discard(vec![test_card(3)]);
        assert_eq!(dealer.discard_pile.len(), 3);
    }

    #[test]
    fn shuffle_with_seed_is_deterministic() {
        let mut dealer1 = CardDealer::new(Seed(12345));
        let mut dealer2 = CardDealer::new(Seed(12345));
        let cards1: Vec<_> = (0..10).map(|_| dealer1.draw_one().unwrap()).collect();
        let cards2: Vec<_> = (0..10).map(|_| dealer2.draw_one().unwrap()).collect();
        assert_eq!(cards1, cards2);
    }

    #[test]
    fn different_seeds_produce_different_order() {
        let mut dealer1 = CardDealer::new(Seed(111));
        let mut dealer2 = CardDealer::new(Seed(222));
        let cards1: Vec<_> = (0..10).map(|_| dealer1.draw_one().unwrap()).collect();
        let cards2: Vec<_> = (0..10).map(|_| dealer2.draw_one().unwrap()).collect();
        assert_ne!(cards1, cards2);
    }
}
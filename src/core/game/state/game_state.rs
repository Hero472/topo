use serde::{Deserialize, Serialize};

use crate::core::game::{
    actions::{Action, MoveError, MoveSuccess, TurnPhase},
    board::PlayerBoard,
    card::Card,
    dealer::CardDealer,
    scale::{Scale, ScaleManager}
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GamePhase {
    Waiting,
    Playing,
    Finished,
}

#[derive(Debug)]
pub struct GameState {
    pub room_id:        String,
    pub phase:          GamePhase,
    pub players:        Vec<PlayerBoard>,
    pub card_dealer:    CardDealer,
    pub scale_manager:  ScaleManager,
    pub current_turn:   usize,
    pub turn_phase:     TurnPhase,
    pub turn_seconds:   u64,           // remaining time in current turn
    pub play_time:      u64,           // total play time
    pub seed:           u64,           // game seed (used for dealer)
}


impl GameState {
    /// Creates a new game with the given player IDs and seed.
    /// Automatically deals the initial 13 personal cards and 5 hand cards to each player.
    pub fn new(
        room_id: String,
        player_ids: Vec<usize>,
        seed: u64,
        personal_count: usize,
        hand_count: usize,
    ) -> Self {
        let num_players = player_ids.len();
        let mut dealer = CardDealer::new(seed);

        let (personal_piles, hands) = dealer.deal_initial(num_players, personal_count, hand_count);

        let players = player_ids
            .into_iter()
            .zip(personal_piles)
            .zip(hands)
            .map(|((id, personal), hand)| {
                let mut board = PlayerBoard::new(id);
                board.set_personal(personal);
                board.hand = hand;
                board
            })
            .collect();

        Self {
            room_id,
            phase: GamePhase::Waiting,
            players,
            card_dealer: dealer,
            scale_manager: ScaleManager::new(),
            current_turn: 0,
            turn_phase: TurnPhase::Draw,
            turn_seconds: 60,
            play_time: 0,
            seed,
        }
    }

    #[cfg(test)]
    pub fn test_new(
        players: Vec<PlayerBoard>,
        current_turn: usize,
        card_dealer: CardDealer,
        turn_seconds: u64,
    ) -> Self {
        Self {
            room_id: "test_room".to_string(),
            phase: GamePhase::Waiting,
            players,
            card_dealer,
            scale_manager: ScaleManager::new(),
            current_turn,
            turn_phase: TurnPhase::Draw,
            turn_seconds,
            play_time: 0,
            seed: 0,
        }
    }

    // ── Player management ─────────────────────────────────────

    pub fn add_player(&mut self, player_idx: usize) -> bool {
        if self.players.len() >= 2 { return false; }
        if self.players.iter().any(|p| p.player_idx == player_idx) { return false; }
        self.players.push(PlayerBoard::new(player_idx));
        if self.players.len() == 2 {
            self.start_game();
        }
        true
    }

    pub fn remove_player(&mut self, player_idx: usize) {
        let before = self.players.len();
        self.players.retain(|p| p.player_idx != player_idx);
        if self.players.len() != before && self.phase == GamePhase::Playing {
            self.phase = GamePhase::Finished;
        }
        if self.current_turn >= self.players.len() {
            self.current_turn = 0;
        }
    }

    pub fn player(&self, id: usize) -> Option<PlayerBoard> {
        self.players.iter().find(|p| p.player_idx == id).cloned()
    }

    /// (Re)start the game with a fresh deck, deal, and reset scales.
    pub fn start_game(&mut self) {
        self.card_dealer = CardDealer::new(self.seed);
        let (personal_piles, hands) = self.card_dealer.deal_initial(
            self.players.len(),
            13,
            5,
        );
        for (board, (personal, hand)) in self.players.iter_mut().zip(
            personal_piles.into_iter().zip(hands.into_iter())
        ) {
            board.set_personal(personal);
            board.hand = hand;
            board.side = Default::default();
        }
        self.scale_manager.reset();
        self.phase = GamePhase::Playing;
        self.turn_phase = TurnPhase::Draw;
        self.current_turn = 0;
    }

    // ── Turn & phase helpers ──────────────────────────────────
    pub fn current_idx(&self) -> usize {
        self.current_turn
    }

    pub fn current_player_id(&self) -> Option<usize> {
        if self.phase != GamePhase::Playing {
            return None;
        }
        self.players.get(self.current_turn).map(|p| p.player_idx)
    }

    pub fn is_playing(&self) -> bool {
        self.phase == GamePhase::Playing
    }

    pub fn scale(&self, id: &usize) -> &Scale {
        &self.scale_manager.scales[*id]
    }

    // ── Winning & end‑game ────────────────────────────────────
    pub fn winner(&self) -> Option<&PlayerBoard> {
        self.players.iter().find(|p| p.has_won())
    }

    pub fn check_win_condition(&mut self, player_idx: usize) -> bool {
        if self.players[player_idx].has_won() {
            self.phase = GamePhase::Finished;
            return true;
        }
        false
    }

    // ── Core draw ─────────────────────────────────────────────
    /// Draw one card from the dealer (with automatic recycling).
    fn draw_one(&mut self) -> Option<Card> {
        self.card_dealer.draw_one()
    }

    // ── Hand refill ──────────────────────────────────────────
    /// Called **only during the Draw action**.
    /// 1. Always draws one card (the turn draw).
    /// 2. Then fills the hand to exactly 5 cards if possible.
    fn execute_draw_phase(&mut self, player_idx: usize) {
        // 1. Obligatory turn draw (unless deck empty)
        if let Some(card) = self.draw_one() {
            self.players[player_idx].draw_to_hand(card);
        }

        // 2. Top up to 5 if hand still below 5
        while self.players[player_idx].hand.len() < 5 {
            if let Some(card) = self.draw_one() {
                self.players[player_idx].draw_to_hand(card);
            } else {
                break;
            }
        }
    }

    fn refill_hand_to_five(&mut self, player_idx: usize) {
        if self.phase != GamePhase::Playing {
            return;
        }
        
        if self.players[player_idx].hand_len() == 0 {
            while self.players[player_idx].hand.len() < 5 {
                if let Some(card) = self.draw_one() {
                    self.players[player_idx].draw_to_hand(card);
                } else {
                    break; // deck empty, stop refilling
                }
            }
        }
    }

    pub fn advance_turn(&mut self) -> usize {
        assert_eq!(self.phase, GamePhase::Playing, "Cannot advance turn: not playing");
        self.current_turn = (self.current_turn + 1) % self.players.len();
        self.turn_phase = TurnPhase::Draw;
        self.turn_seconds = 60;
        self.players[self.current_turn].player_idx  // panic if index missing
    }

    // ── Scale interaction ────────────────────────────────────

    /// Check if a specific scale accepts a card (for pre‑validation).
    pub fn can_place_on_scale(&self, scale_id: usize, card: &Card) -> bool {
        self.scale_manager.can_place_on_scale(scale_id, card)
    }

    // ── Apply an action from the current player ──────────────
    pub fn apply_move(&mut self, player_idx: usize, action: Action) -> Result<MoveSuccess, MoveError> {
        let Some(idx) = self.players.iter().position(|p| p.player_idx == player_idx) else {
            return Err(MoveError::NotAllowed);
        };
        if self.phase != GamePhase::Playing {
            return Err(MoveError::NotAllowed);
        }
        if idx != self.current_turn {
            return Err(MoveError::NotYourTurn);
        }

        match action {
            Action::Draw => {
                if self.turn_phase != TurnPhase::Draw {
                    return Err(MoveError::NotAllowed);
                }
                self.execute_draw_phase(idx);
                self.turn_phase = TurnPhase::Play;
                Ok(MoveSuccess::Success)
            }

            Action::OpenScale { hand_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let Some(card) = self.players[idx].hand.get(hand_idx).cloned() else {
                    return Err(MoveError::InvalidIndex { kind: "hand_idx".into() });
                };

                // Only Aces can open a scale
                if card.value != 1 {
                    return Err(MoveError::DoesNotFit);
                }

                // Remove card from hand and try to open scale
                let card = self.players[idx].take_from_hand(hand_idx).unwrap();
                let result = self.scale_manager.open_scale(card)?;  // propagates any error
                self.refill_hand_to_five(idx);
                Ok(result)   // result is already MoveSuccess::ScaleOpened
            }

            Action::PlayHand { hand_idx, scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let Some(card) = self.players[idx].hand.get(hand_idx).cloned() else {
                    return Err(MoveError::InvalidIndex { kind: "hand_idx".into() });
                };

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    return Err(MoveError::DoesNotFit);
                }

                let card = self.players[idx].take_from_hand(hand_idx).unwrap();
                let result = self.scale_manager.place_on_scale(scale_idx, card)?;
                self.refill_hand_to_five(idx);
                Ok(result)
            }

            Action::PlayPersonal { scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let Some(card) = self.players[idx].pop_personal() else {
                    return Err(MoveError::InvalidIndex { kind: "personal".into() });
                };

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    // Return card to hand since we already popped it
                    self.players[idx].hand.push(card);
                    return Err(MoveError::DoesNotFit);
                }

                let result = self.scale_manager.place_on_scale(scale_idx, card)?;
                self.refill_hand_to_five(idx);
                Ok(result)
            }

            Action::PlaySide { stack, scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let Some(card) = self.players[idx].take_from_side(stack) else {
                    return Err(MoveError::InvalidIndex { kind: "stack".into() });
                };

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    // Return card to hand (common rule)
                    self.players[idx].hand.push(card);
                    return Err(MoveError::DoesNotFit);
                }

                let result = self.scale_manager.place_on_scale(scale_idx, card)?;
                self.refill_hand_to_five(idx);
                Ok(result)
            }

            Action::MoveToSide { hand_idx, stack } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let Some(card) = self.players[idx].take_from_hand(hand_idx) else {
                    return Err(MoveError::InvalidIndex { kind: "hand_idx".into() });
                };

                if self.players[idx].place_on_side(stack, card) {
                    if self.players[idx].has_won() {
                        self.phase = GamePhase::Finished;
                        return Ok(MoveSuccess::GameWon { winner_id: idx });
                    }
                    self.advance_turn();
                    Ok(MoveSuccess::TurnEnded)
                } else {
                    self.players[idx].hand.push(card);
                    Err(MoveError::InvalidIndex { kind: "stack".into() })
                }
            }

            Action::MovePersonalToSide { stack_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                // Only a King (13) can be moved from personal to side
                if self.players[idx].personal_top().map_or(true, |c| c.value != 13) {
                    return Err(MoveError::NotAllowed);
                }

                let card = self.players[idx].pop_personal().unwrap();
                if self.players[idx].place_on_side(stack_idx, card) {
                    self.refill_hand_to_five(idx);
                    Ok(MoveSuccess::Success)
                } else {
                    self.players[idx].personal.push(card);
                    Err(MoveError::InvalidIndex { kind: "stack".into() })
                }
            }
        }
    }
        
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game::{
        actions::{Action, MoveSuccess, MoveError, TurnPhase},
        card::{Card, Suit},
        deck::DeckColor,
        scale::Scale,
    };

    // ── Helpers ─────────────────────────────────────────────────

    fn make_game() -> GameState {
        let player_ids = vec![0, 1];
        let mut gs = GameState::new("room".into(), player_ids, 42, 13, 5);
        gs.start_game(); // ensure start_game is pub(crate) or use make_game_via_add
        gs
    }

    fn hand_len(gs: &GameState, idx: usize) -> usize {
        gs.players[idx].hand.len()
    }

    fn personal_len(gs: &GameState, idx: usize) -> usize {
        gs.players[idx].personal.len()
    }

    fn current_turn_player(gs: &GameState) -> usize {
        gs.players[gs.current_turn].player_idx
    }

    // ── Initialisation & player management ───────────────────
    #[test]
    fn new_game_has_two_players_waiting() {
        let gs = GameState::new("r".into(), vec![0, 1], 0, 13, 5);
        assert_eq!(gs.players.len(), 2);
        assert_eq!(gs.phase, GamePhase::Waiting);
    }

    #[test]
    fn start_game_deals_cards() {
        let gs = make_game();
        for p in &gs.players {
            assert_eq!(p.personal.len(), 13);
            assert_eq!(p.hand.len(), 5);
            assert_eq!(p.side.iter().all(|s| s.is_empty()), true);
        }
        assert_eq!(gs.phase, GamePhase::Playing);
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(current_turn_player(&gs), 0);
    }

    #[test]
    fn add_player_beyond_two_fails() {
        let mut gs = make_game();
        assert!(!gs.add_player(2));
    }

    #[test]
    fn remove_during_play_ends_game() {
        let mut gs = make_game();
        let before = gs.players.len();
        gs.remove_player(1);
        assert_eq!(gs.phase, GamePhase::Finished);
        assert_eq!(gs.players.len(), before - 1);
    }

    // ── Turn / phase helpers ─────────────────────────────────
    #[test]
    fn not_your_turn_rejected() {
        let mut gs = make_game();
        let result = gs.apply_move(1, Action::Draw);
        assert_eq!(result, Err(MoveError::NotYourTurn));
    }

    #[test]
    fn wrong_phase_action_rejected() {
        let mut gs = make_game();
        gs.phase = GamePhase::Waiting;
        let result = gs.apply_move(0, Action::Draw);
        assert_eq!(result, Err(MoveError::NotAllowed));
    }

    // ── Draw action ──────────────────────────────────────────
    #[test]
    fn draw_in_draw_phase_works() {
        let mut gs = make_game();
        let player_idx = current_turn_player(&gs);
        let initial_hand = hand_len(&gs, 0);
        let result = gs.apply_move(player_idx, Action::Draw);
        assert_eq!(initial_hand, 5);
        assert_eq!(result, Ok(MoveSuccess::Success));
        assert!(hand_len(&gs, 0) >= 6, "expected hand size >=6, got {}", hand_len(&gs, 0));
        assert_eq!(gs.turn_phase, TurnPhase::Play);
    }

    #[test]
    fn draw_in_play_phase_rejected() {
        let mut gs = make_game();
        gs.turn_phase = TurnPhase::Play;
        let result = gs.apply_move(current_turn_player(&gs), Action::Draw);
        assert_eq!(result, Err(MoveError::NotAllowed));
    }

    #[test]
    fn draw_with_empty_deck_still_succeeds() {
        let mut gs = make_game();
        while gs.card_dealer.remaining() > 0 {
            gs.card_dealer.draw_one();
        }
        let result = gs.apply_move(0, Action::Draw);
        assert_eq!(result, Ok(MoveSuccess::Success));
        assert_eq!(gs.turn_phase, TurnPhase::Play);
    }

    #[test]
    fn draw_with_non_zero_based_ids_does_not_panic() {
        let player_ids = vec![100, 200];   // IDs differ from array indices
        let mut gs = GameState::new("room".into(), player_ids, 42, 13, 5);
        gs.start_game();

        // current player should be the first one (array index 0, ID 100)
        assert_eq!(gs.current_player_id(), Some(100));

        let result = gs.apply_move(100, Action::Draw);   // use the ID, not the index
        assert_eq!(result, Ok(MoveSuccess::Success));
        assert!(hand_len(&gs, 0) >= 6);
    }

    // ── OpenScale action ─────────────────────────────────────
    #[test]
    fn open_scale_with_ace() {
        let mut gs = make_game();

        let _ = gs.apply_move(0, Action::Draw);
        gs.players[0].hand[0] = Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red };
        let hand_size_before = hand_len(&gs, 0);
        let result = gs.apply_move(0, Action::OpenScale { hand_idx: 0 });
        assert_eq!(result, Ok(MoveSuccess::ScaleOpened { scale_id: 0 }));
        assert_eq!(gs.scale_manager.scales.len(), 1);
        assert_eq!(hand_len(&gs, 0), hand_size_before - 1);
    }

    #[test]
    fn open_scale_with_non_ace_rejected() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        gs.players[0].hand[0] = Card { suit: Suit::Hearts, value: 5, deck: DeckColor::Red };
        let result = gs.apply_move(0, Action::OpenScale { hand_idx: 0 });
        assert_eq!(result, Err(MoveError::DoesNotFit));
        assert_eq!(hand_len(&gs, 0), 6); // hand unchanged
    }

    // ── PlayHand action ──────────────────────────────────────
    #[test]
    fn play_hand_valid() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        // open scale with Ace at index 0
        gs.players[0].hand[0] = Card { value: 1, ..gs.players[0].hand[0] };
        gs.apply_move(0, Action::OpenScale { hand_idx: 0 });
        // place 2 at index 0
        gs.players[0].hand[0] = Card { value: 2, ..gs.players[0].hand[0] };
        let result = gs.apply_move(0, Action::PlayHand { hand_idx: 0, scale_idx: 0 });
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: 0, completed: false }));
        assert_eq!(hand_len(&gs, 0), 4); // started 6 -> open 5 -> play 4
    }

    #[test]
    fn play_hand_invalid_scale() {
        let mut gs = make_game();
        gs.apply_move(0, Action::Draw);
        let result = gs.apply_move(0, Action::PlayHand { hand_idx: 0, scale_idx: 99 });
        assert_eq!(result, Err(MoveError::DoesNotFit));
    }

    // ── PlayPersonal action ──────────────────────────────────
    #[test]
    fn play_personal_to_scale() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);

        gs.players[0].personal[12] = Card {
            suit: Suit::Hearts,
            value: 1,
            deck: DeckColor::Red,
        };

        gs.scale_manager.scales.push(Scale::new(0));
        let result = gs.apply_move(0, Action::PlayPersonal { scale_idx: 0 });
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: 0, completed: false }));
        assert_eq!(personal_len(&gs, 0), 12);
    }

    // ── PlaySide action ──────────────────────────────────────
    #[test]
    fn play_side_to_scale() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        let card = gs.players[0].hand[0];
        gs.players[0].side[0].push(Card { value: 1, ..card });

        gs.scale_manager.scales.push(Scale::new(0));
        let result = gs.apply_move(0, Action::PlaySide { stack: 0, scale_idx: 0 });
        assert_eq!(result, Ok(MoveSuccess::ScalePlaced { scale_id: 0, completed: false }));
        assert!(gs.players[0].side[0].is_empty());
    }

    // ── MoveToSide action ────────────────────────────────────
    #[test]
    fn move_to_side_ends_turn() {
        let mut gs = make_game();
        assert_eq!(gs.apply_move(0, Action::Draw), Ok(MoveSuccess::Success));
        assert_eq!(hand_len(&gs, 0), 6);
        let initial_turn = gs.current_turn;
        let result = gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 });
        assert_eq!(result, Ok(MoveSuccess::TurnEnded));
        assert_eq!(gs.current_turn, (initial_turn + 1) % 2);
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(gs.players[0].side[0].len(), 1);
        assert_eq!(hand_len(&gs, 0), 5);
    }

    // ── MovePersonalToSide ───────────────────────────────────
    #[test]
    fn move_king_from_personal_to_side() {
        let mut gs = make_game();

        assert_eq!(gs.apply_move(0, Action::Draw), Ok(MoveSuccess::Success));
        let king = Card { suit: Suit::Spades, value: 13, deck: DeckColor::Blue };
        gs.players[0].personal.clear();
        gs.players[0].personal.push(king);
        let result = gs.apply_move(0, Action::MovePersonalToSide { stack_idx: 0 });
        assert_eq!(result, Ok(MoveSuccess::Success));
        assert_eq!(gs.players[0].personal.is_empty(), true);
        assert_eq!(gs.players[0].side[0].len(), 1);
    }

    #[test]
    fn move_non_king_from_personal_rejected() {
        let mut gs = make_game();
        gs.apply_move(0, Action::Draw);

        let card = Card { value: 5, ..gs.players[0].personal[0] };
        gs.players[0].personal.clear();
        gs.players[0].personal.push(card);
        let result = gs.apply_move(0, Action::MovePersonalToSide { stack_idx: 0 });
        assert_eq!(result, Err(MoveError::NotAllowed));
    }

    // ── Win condition ────────────────────────────────────────
    #[test]
    fn win_on_move_to_side_when_both_piles_empty() {
        let mut gs = make_game();
        gs.apply_move(0, Action::Draw);
        gs.players[0].personal.clear();
        gs.players[0].hand = vec![Card { value: 2, ..gs.players[0].hand[0] }];
        let result = gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 });
        assert_eq!(result, Ok(MoveSuccess::GameWon { winner_id: 0 }));
        assert_eq!(gs.phase, GamePhase::Finished);
    }

    // ── Simulated partial game ───────────────────────────────
    #[test]
    fn simulated_game_partial() {
        let mut gs = make_game();

        // Turn 1: Player 0
        assert_eq!(gs.apply_move(0, Action::Draw), Ok(MoveSuccess::Success));
        assert_eq!(hand_len(&gs, 0), 6);

        gs.players[0].hand[0] = Card { value: 1, suit: Suit::Hearts, deck: DeckColor::Red };
        assert_eq!(gs.apply_move(0, Action::OpenScale { hand_idx: 0 }), Ok(MoveSuccess::ScaleOpened { scale_id: 0 }));
        assert_eq!(hand_len(&gs, 0), 5);
        assert_eq!(gs.scale_manager.scales.len(), 1);

        gs.players[0].hand[0] = Card { value: 2, suit: Suit::Hearts, deck: DeckColor::Red };
        assert_eq!(gs.apply_move(0, Action::PlayHand { hand_idx: 0, scale_idx: 0 }), Ok(MoveSuccess::ScalePlaced { scale_id: 0, completed: false }));
        assert_eq!(hand_len(&gs, 0), 4);

        assert_eq!(gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 }), Ok(MoveSuccess::TurnEnded));
        assert_eq!(gs.current_turn, 1);
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(gs.players[0].side[0].len(), 1);
        assert_eq!(hand_len(&gs, 0), 3);

        // Turn 2: Player 1
        assert_eq!(gs.apply_move(1, Action::Draw), Ok(MoveSuccess::Success));
        assert_eq!(hand_len(&gs, 1), 6);

        gs.players[1].hand[0] = Card { value: 1, suit: Suit::Diamonds, deck: DeckColor::Blue };
        assert_eq!(gs.apply_move(1, Action::OpenScale { hand_idx: 0 }), Ok(MoveSuccess::ScaleOpened { scale_id: 1 }));
        assert_eq!(hand_len(&gs, 1), 5);

        gs.players[1].hand[0] = Card { value: 2, suit: Suit::Diamonds, deck: DeckColor::Blue };
        assert_eq!(gs.apply_move(1, Action::PlayHand { hand_idx: 0, scale_idx: 1 }), Ok(MoveSuccess::ScalePlaced { scale_id: 1, completed: false }));
        assert_eq!(hand_len(&gs, 1), 4);
    }

    // ── Utility methods ──────────────────────────────────────
    #[test]
    fn current_idx_returns_current_turn() {
        let mut gs = make_game();
        assert_eq!(gs.current_idx(), 0);
        gs.apply_move(0, Action::Draw).unwrap();
        gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 }).unwrap();
        assert_eq!(gs.current_idx(), 1);
    }

    #[test]
    fn current_player_id_returns_correct_id() {
        let gs = make_game();
        assert_eq!(gs.current_player_id(), Some(0));
    }

    #[test]
    fn current_player_id_none_if_no_players() {
        let gs = GameState::new("room".into(), vec![], 0, 13, 5);
        assert_eq!(gs.current_player_id(), None);
    }

    #[test]
    fn is_playing_returns_true_only_while_playing() {
        let mut gs = make_game();
        assert!(gs.is_playing());
        gs.phase = GamePhase::Waiting;
        assert!(!gs.is_playing());
        gs.phase = GamePhase::Finished;
        assert!(!gs.is_playing());
    }

    #[test]
    fn winner_returns_none_when_no_one_has_won() {
        let gs = make_game();
        assert!(gs.winner().is_none());
    }

    #[test]
    fn winner_returns_some_after_win() {
        let mut gs = make_game();
        // Simulate a winning state for player 0 without calling MoveToSide
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        assert!(gs.players[0].has_won());
        assert_eq!(gs.winner().unwrap().player_idx, 0);
    }

    #[test]
    fn check_win_condition_sets_finished() {
        let mut gs = make_game();
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        assert!(!gs.check_win_condition(1));
        assert_eq!(gs.phase, GamePhase::Playing);
        assert!(gs.check_win_condition(0));
        assert_eq!(gs.phase, GamePhase::Finished);
        assert!(gs.check_win_condition(0)); // still true, idempotent
    }
}
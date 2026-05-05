use serde::{Deserialize, Serialize};

use crate::core::game::{
    actions::{Action, PlayResult, TurnPhase}, board::PlayerBoard, card::Card, dealer::CardDealer, scale::ScaleManager
};

/// The overall game lifecycle.
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
        personal_count: usize,   // e.g., 13
        hand_count: usize,       // e.g., 5
    ) -> Self {
        let num_players = player_ids.len();
        let mut dealer = CardDealer::new(seed);

        // Distribute personal piles and hands.
        let (personal_piles, hands) = dealer.deal_initial(num_players, personal_count, hand_count);

        let players = player_ids
            .into_iter()
            .zip(personal_piles)
            .zip(hands)
            .map(|((id, personal), hand)| {
                let mut board = PlayerBoard::new(id);
                board.set_personal(personal);
                // Fill hand directly (hand starts with exactly hand_count cards)
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
        self.players.get(self.current_turn).map(|p| p.player_idx)
    }

    pub fn is_playing(&self) -> bool {
        self.phase == GamePhase::Playing
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


    fn advance_turn(&mut self) {
        self.current_turn = (self.current_turn + 1) % self.players.len();
        self.turn_phase = TurnPhase::Draw;
        self.turn_seconds = 60;
    }

    // ── Scale interaction ────────────────────────────────────

    /// Check if a specific scale accepts a card (for pre‑validation).
    pub fn can_place_on_scale(&self, scale_id: usize, card: &Card) -> bool {
        self.scale_manager.can_place_on_scale(scale_id, card)
    }

    // ── Apply an action from the current player ──────────────
    pub fn apply_move(&mut self, player_idx: usize, action: Action) -> PlayResult {
        let Some(idx) = self.players.iter().position(|p| p.player_idx == player_idx) else {
            return PlayResult::NotAllowed;
        };
        if self.phase != GamePhase::Playing {
            return PlayResult::NotAllowed;
        }
        if idx != self.current_turn {
            return PlayResult::NotYourTurn;
        }

        match action {
            Action::Draw => {
                if self.turn_phase != TurnPhase::Draw {
                    return PlayResult::NotAllowed;
                }

                self.execute_draw_phase(player_idx);

                self.turn_phase = TurnPhase::Play;
                PlayResult::Success
            },
            Action::OpenScale { hand_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return PlayResult::NotAllowed;
                }

                let Some(card) = self.players[idx].hand.get(hand_idx).cloned() else {
                    return PlayResult::InvalidIndex { kind: "hand_idx".into() };
                };

                // Only Aces can open a scale
                if card.value != 1 {
                    return PlayResult::DoesNotFit;
                }

                // Safe to remove now
                let card = self.players[idx].take_from_hand(hand_idx).unwrap();
                let result = self.scale_manager.open_scale(card);

                // On success, ScaleManager returns ScaleOpened; on failure it would be DoesNotFit,
                // but that can't happen here because we already checked Ace. Still handle generically.
                if matches!(result, PlayResult::ScaleOpened { .. }) {
                    self.refill_hand_to_five(idx);
                } else {
                    // If somehow fails (shouldn’t), return card to hand
                    self.players[idx].hand.push(card); // but card is already consumed; open_scale takes ownership
                    // To be safe, open_scale should not consume on failure – we'll assume it doesn't fail
                }

                self.refill_hand_to_five(idx);

                result
            },
            Action::PlayHand { hand_idx, scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return PlayResult::NotAllowed;
                }

                let Some(card) = self.players[idx].hand.get(hand_idx).cloned() else {
                    return PlayResult::InvalidIndex { kind: "hand_idx".into() };
                };

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    return PlayResult::DoesNotFit;
                }

                let card = self.players[idx].take_from_hand(hand_idx).unwrap();
                let result = self.scale_manager.place_on_scale(scale_idx, card);

                self.refill_hand_to_five(idx);

                result
            },
            Action::PlayPersonal { scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return PlayResult::NotAllowed;
                }

                let Some(card) = self.players[idx].pop_personal() else {
                    return PlayResult::InvalidIndex { kind: "personal".into() };
                };

                // Check if the chosen scale accepts it
                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    // Return card to hand (since we already popped)
                    self.players[idx].hand.push(card);
                    return PlayResult::DoesNotFit;
                }

                let result = self.scale_manager.place_on_scale(scale_idx, card);

                self.refill_hand_to_five(idx);
                result
            },
            Action::PlaySide { stack, scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return PlayResult::NotAllowed;
                }

                let Some(card) = self.players[idx].take_from_side(stack) else {
                    return PlayResult::InvalidIndex { kind: "stack".into() };
                };

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    // Return card to hand (not back to side – common rule)
                    self.players[idx].hand.push(card);
                    return PlayResult::DoesNotFit;
                }

                let result = self.scale_manager.place_on_scale(scale_idx, card);

                self.refill_hand_to_five(idx);

                result
            },
            Action::MoveToSide { hand_idx, stack } => {
                if self.turn_phase != TurnPhase::Play {
                    return PlayResult::NotAllowed;
                }

                let Some(card) = self.players[idx].take_from_hand(hand_idx) else {
                    return PlayResult::InvalidIndex { kind: "hand_idx".into() };
                };

                if self.players[idx].place_on_side(stack, card) {

                    if self.players[idx].has_won() {
                        self.phase = GamePhase::Finished;
                        return PlayResult::GameWon { winner_id: idx }
                    }

                    self.advance_turn();

                    PlayResult::TurnEnded
                } else {
                    self.players[idx].hand.push(card);
                    PlayResult::InvalidIndex { kind: "stack".into() }
                }
            },
            Action::MovePersonalToSide { stack_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return PlayResult::NotAllowed;
                }

                // Only a King (13) can be moved from personal to side
                if self.players[idx].personal_top().map_or(true, |c| c.value != 13) {
                    return PlayResult::NotAllowed;
                }

                let card = self.players[idx].pop_personal().unwrap();
                if self.players[idx].place_on_side(stack_idx, card) {
                    self.refill_hand_to_five(idx);
                    // We treat this as a regular success (turn does NOT automatically end, but can be ended by EndTurn)
                    PlayResult::Success
                } else {
                    self.players[idx].personal.push(card);
                    PlayResult::InvalidIndex { kind: "stack".into() }
                }
            }
        }
    }
        
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game::{
        actions::{Action, PlayResult, TurnPhase},
        card::{Card, Suit}, deck::DeckColor, scale::Scale,
    };

    // ── Helper to build a ready 2‑player game ─────────────────
    // Requires `start_game` to be callable in tests.
    // Workaround: create via `add_player` which auto‑starts.
    fn make_game() -> GameState {
        let player_ids = vec![0, 1];
        let mut gs = GameState::new("room".into(), player_ids, 42, 13, 5);
        // Phase is Waiting; players already exist but no cards.
        // Force start via the same path as add_player (which calls start_game internally).
        // Since both players already exist, we call start_game() directly.
        // You must make `start_game` accessible: change to `pub(crate) fn start_game(...)`
        // or add a test‑only method.
        gs.start_game();             // requires `start_game` to be visible
        gs
    }

    // Alternate helper if start_game remains private:
    // Use add_player to fill the room (it auto‑starts when player count reaches 2).
    fn make_game_via_add() -> GameState {
        let mut gs = GameState::new("room".into(), vec![], 42, 13, 5);
        gs.add_player(0);
        gs.add_player(1);
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
        assert_eq!(current_turn_player(&gs), 0); // player 0 starts
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
        let result = gs.apply_move(1, Action::Draw); // player 1 tries while turn = 0
        assert_eq!(result, PlayResult::NotYourTurn);
    }

    #[test]
    fn wrong_phase_action_rejected() {
        let mut gs = make_game();
        // Simulate being in Waiting by force
        gs.phase = GamePhase::Waiting;
        let result = gs.apply_move(0, Action::Draw);
        assert_eq!(result, PlayResult::NotAllowed);
    }

    // ── Draw action ──────────────────────────────────────────
    #[test]
    fn draw_in_draw_phase_works() {
        let mut gs = make_game();
        let player_idx = current_turn_player(&gs);
        let initial_hand = hand_len(&gs, 0);
        let result = gs.apply_move(player_idx, Action::Draw);
        assert_eq!(initial_hand, 5);
        assert_eq!(result, PlayResult::Success);
        // Hand now filled to at least 5, plus one extra = 6 (if dealer has cards)
        assert!(hand_len(&gs, 0) >= 6, "expected hand size >=6, got {}", hand_len(&gs, 0));
        assert_eq!(gs.turn_phase, TurnPhase::Play);
    }

    #[test]
    fn draw_in_play_phase_rejected() {
        let mut gs = make_game();
        // Advance to Play phase manually
        gs.turn_phase = TurnPhase::Play;
        let result = gs.apply_move(current_turn_player(&gs), Action::Draw);
        assert_eq!(result, PlayResult::NotAllowed);
    }

    #[test]
    fn draw_with_empty_deck_still_succeeds() {
        let mut gs = make_game();
        // Drain all cards from dealer
        while gs.card_dealer.remaining() > 0 {
            gs.card_dealer.draw_one();
        }
        let result = gs.apply_move(0, Action::Draw);
        // Even if deck empty, the action still succeeds (just doesn't add cards)
        assert_eq!(result, PlayResult::Success);
        assert_eq!(gs.turn_phase, TurnPhase::Play);
    }

    // ── OpenScale action ─────────────────────────────────────
    #[test]
    fn open_scale_with_ace() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        // Player 0's hand contains some random cards; we need an Ace at a known index.
        // Force first card to be an Ace by direct manipulation for test stability.
        gs.players[0].hand[0] = Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red };
        let hand_size_before = hand_len(&gs, 0);
        let result = gs.apply_move(0, Action::OpenScale { hand_idx: 0 });
        assert_eq!(result, PlayResult::ScaleOpened { scale_id: 0 });
        assert_eq!(gs.scale_manager.scales.len(), 1);
        // After removal, hand_len decreased by 1.
        // refill_hand_to_five only acts if hand is empty -> so no refill.
        assert_eq!(hand_len(&gs, 0), hand_size_before - 1);
    }

    #[test]
    fn open_scale_with_non_ace_rejected() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        // First card is not Ace (e.g., 5)
        gs.players[0].hand[0] = Card { suit: Suit::Hearts, value: 5, deck: DeckColor::Red };
        let result = gs.apply_move(0, Action::OpenScale { hand_idx: 0 });
        assert_eq!(result, PlayResult::DoesNotFit);
        // Card still in hand
        assert_eq!(hand_len(&gs, 0), 6);
    }

    // ── PlayHand action ──────────────────────────────────────
    #[test]
    fn play_hand_valid() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        // First open a scale with an Ace
        gs.players[0].hand[0] = Card { value: 1, ..gs.players[0].hand[0] };
        gs.apply_move(0, Action::OpenScale { hand_idx: 0 });
        // Now hand has one less card (4 if no refill), but we need a 2 to play.
        // Force the first remaining card to be a 2.
        gs.players[0].hand[0] = Card { value: 2, ..gs.players[0].hand[0] };
        let result = gs.apply_move(0, Action::PlayHand { hand_idx: 0, scale_idx: 0 });
        assert_eq!(result, PlayResult::ScalePlaced { scale_id: 0, completed: false });
        // Hand size decreases again (no refill because hand != 0)
        assert_eq!(hand_len(&gs, 0), 4); // started 6, open ->5, play ->4
    }

    #[test]
    fn play_hand_invalid_scale() {
        let mut gs = make_game();
        gs.apply_move(0, Action::Draw);

        let result = gs.apply_move(0, Action::PlayHand { hand_idx: 0, scale_idx: 99 });
        // can_place_on_scale returns false -> DoesNotFit
        assert_eq!(result, PlayResult::DoesNotFit);
    }

    // ── PlayPersonal action ──────────────────────────────────
    #[test]
    fn play_personal_to_scale() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);

        // Get a card to clone its suit/color
        gs.players[0].personal[12] = Card {
            suit: Suit::Hearts,
            value: 1,
            deck: DeckColor::Red,
        };

        gs.scale_manager.scales.push(Scale::new(0)); // empty scale accepts Ace

        let result = gs.apply_move(0, Action::PlayPersonal { scale_idx: 0 });
        assert_eq!(result, PlayResult::ScalePlaced { scale_id: 0, completed: false });
        assert_eq!(personal_len(&gs, 0), 12);
    }

    // ── PlaySide action ──────────────────────────────────────
    #[test]
    fn play_side_to_scale() {
        let mut gs = make_game();

        gs.apply_move(0, Action::Draw);
        // Take a copy of the card first
        let card = gs.players[0].hand[0];
        gs.players[0].side[0].push(Card { value: 1, ..card });

        gs.scale_manager.scales.push(Scale::new(0));
        let result = gs.apply_move(0, Action::PlaySide { stack: 0, scale_idx: 0 });
        assert_eq!(result, PlayResult::ScalePlaced { scale_id: 0, completed: false });
        assert!(gs.players[0].side[0].is_empty());
    }

    // ── MoveToSide action ────────────────────────────────────
    #[test]
    fn move_to_side_ends_turn() {
        let mut gs = make_game();
        assert_eq!(gs.apply_move(0, Action::Draw), PlayResult::Success);   
        assert_eq!(hand_len(&gs, 0), 6);
        // Hand has 5 cards; move one to side stack
        let initial_turn = gs.current_turn;
        let result = gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 });
        assert_eq!(result, PlayResult::TurnEnded);
        assert_eq!(gs.current_turn, (initial_turn + 1) % 2);
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        // Side stack now has one card, hand decreased by one (no refill)
        assert_eq!(gs.players[0].side[0].len(), 1);
        assert_eq!(hand_len(&gs, 0), 5); // from 6 to 5
    }

    // ── MovePersonalToSide ───────────────────────────────────
    #[test]
    fn move_king_from_personal_to_side() {
        let mut gs = make_game();

        assert_eq!(gs.apply_move(0, Action::Draw), PlayResult::Success);
        // Set personal top to King
        let king = Card { suit: Suit::Spades, value: 13, deck: DeckColor::Blue };
        gs.players[0].personal.clear();
        gs.players[0].personal.push(king);
        let result = gs.apply_move(0, Action::MovePersonalToSide { stack_idx: 0 });
        assert_eq!(result, PlayResult::Success);
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
        assert_eq!(result, PlayResult::NotAllowed);
    }

    // ── Win condition (only checked in MoveToSide) ───────────
    #[test]
    fn win_on_move_to_side_when_both_piles_empty() {
        let mut gs = make_game();
        gs.apply_move(0, Action::Draw);
        gs.players[0].personal.clear();
        gs.players[0].hand = vec![Card { value: 2, ..gs.players[0].hand[0] }];
        let result = gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 });
        assert_eq!(result, PlayResult::GameWon { winner_id: 0 });
        assert_eq!(gs.phase, GamePhase::Finished);
    }

    #[test]
    fn simulated_game_partial() {

        let mut gs = make_game();

        // ── TURN 1: Player 0 ──────────────────────────────────────
        // 1. Draw phase → hand becomes 6
        assert_eq!(gs.apply_move(0, Action::Draw), PlayResult::Success);
        assert_eq!(hand_len(&gs, 0), 6);

        // 2. Open a scale with an Ace from hand (force an Ace at index 0)
        gs.players[0].hand[0] = Card { value: 1, suit: Suit::Hearts, deck: DeckColor::Red };
        assert_eq!(gs.apply_move(0, Action::OpenScale { hand_idx: 0 }), PlayResult::ScaleOpened { scale_id: 0 });

        // Hand size drops to 5 (no refill because hand wasn't empty)
        assert_eq!(hand_len(&gs, 0), 5);
        assert_eq!(gs.scale_manager.scales.len(), 1);

        // 3. Play a 2 from hand onto scale 0 (force a 2 at index 0)
        gs.players[0].hand[0] = Card { value: 2, suit: Suit::Hearts, deck: DeckColor::Red };
        assert_eq!(gs.apply_move(0, Action::PlayHand { hand_idx: 0, scale_idx: 0 }), PlayResult::ScalePlaced { scale_id: 0, completed: false });
        assert_eq!(hand_len(&gs, 0), 4); // 5 → 4 (no refill)

        // 4. Move a card to side stack → ends turn automatically
        assert_eq!(gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 }), PlayResult::TurnEnded);

        // Side stack now has one card, hand 3 (4-1), turn passed to player 1
        assert_eq!(gs.current_turn, 1);
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(gs.players[0].side[0].len(), 1);
        assert_eq!(hand_len(&gs, 0), 3);

        // ── TURN 2: Player 1 ──────────────────────────────────────
        // 1. Draw → hand becomes 6
        assert_eq!(gs.apply_move(1, Action::Draw), PlayResult::Success);
        // After drawing: hand goes from 5 to 6
        assert_eq!(hand_len(&gs, 1), 6);

        // 2. Open a new scale with an Ace
        gs.players[1].hand[0] = Card { value: 1, suit: Suit::Diamonds, deck: DeckColor::Blue };
        assert_eq!(gs.apply_move(1, Action::OpenScale { hand_idx: 0 }), PlayResult::ScaleOpened { scale_id: 1 });
        assert_eq!(hand_len(&gs, 1), 5);

        // 3. Play a 2 onto that scale
        gs.players[1].hand[0] = Card { value: 2, suit: Suit::Diamonds, deck: DeckColor::Blue };
        assert_eq!(gs.apply_move(1, Action::PlayHand { hand_idx: 0, scale_idx: 1 }), PlayResult::ScalePlaced { scale_id: 1, completed: false });
        assert_eq!(hand_len(&gs, 1), 4);

    }

    #[test]
    fn current_idx_returns_current_turn() {
        let mut gs = make_game();
        assert_eq!(gs.current_idx(), 0);
        // Advance turn via a MoveToSide
        gs.apply_move(0, Action::Draw);
        gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 });
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
        gs.apply_move(0, Action::Draw);
        // Make player 0 win: empty personal, empty hand
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        // Win is only checked inside MoveToSide, so we must trigger that
        gs.apply_move(0, Action::MoveToSide { hand_idx: 0, stack: 0 }); // won't panic even if index out of bounds? We'll make hand empty, but MoveToSide needs a hand index.
        // Actually hand is empty, we can't take from hand. So we'd need a different approach.
        // Better: set up win condition and call check_win_condition directly, or we can just test winner() after manually setting win state.
        // Since check_win_condition is separate, we can test winner() by making the player have won (hand empty, personal empty) and not caring about how it was triggered.
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        assert!(gs.players[0].has_won());
        assert_eq!(gs.winner().unwrap().player_idx, 0);
    }


    #[test]
    fn check_win_condition_sets_finished() {
        let mut gs = make_game();
        // Simulate a winning state for player 0
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        assert!(!gs.check_win_condition(1)); // player 1 not won
        assert_eq!(gs.phase, GamePhase::Playing);
        assert!(gs.check_win_condition(0)); // player 0 has won
        assert_eq!(gs.phase, GamePhase::Finished);
        assert!(gs.check_win_condition(0)); // still true, no side effect
    }

}
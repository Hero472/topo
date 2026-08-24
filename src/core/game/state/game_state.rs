use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::{
    game::{
        actions::{Action, MoveError, MoveSuccess, TurnPhase, move_result::MoveResult}, board::PlayerBoard, card::Card, dealer::CardDealer, scale::{Scale, ScaleManager}, state::{Seconds, state_types::Seed},
    }, game_id::GameId, game_index::ScaleIdx, player::{PlayerId, PlayerIdx}
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
    pub game_id: GameId,
    pub phase: GamePhase,
    pub players: Vec<PlayerBoard>,
    pub card_dealer: CardDealer,
    pub scale_manager: ScaleManager,
    pub current_turn: PlayerIdx,
    pub turn_phase: TurnPhase,
    pub turn_seconds: Seconds,
    pub play_time: Seconds,
    pub seed: Seed,
}


impl GameState {
    pub fn new(
        game_id: GameId,
        seed: Seed,
        personal_count: usize,
        hand_count: usize,
        turn_seconds: Seconds
    ) -> Self {
        let num_players = 2;
        let mut dealer = CardDealer::new(seed);

        let (personal_piles, hands) = dealer.deal_initial(num_players, personal_count, hand_count);

        let players = personal_piles
            .into_iter()
            .zip(hands)
            .enumerate()
            .map(|(i, (personal, hand))| {
                
                let mut board = PlayerBoard::new(
                    PlayerId(Uuid::nil()),
                    PlayerIdx(i)
                );   // PlayerIdx(0), PlayerIdx(1)
                
                board.set_personal(personal);
                board.hand = hand;
                board
            })
            .collect();

        Self {
            game_id,
            phase: GamePhase::Waiting,
            players,
            card_dealer: dealer,
            scale_manager: ScaleManager::new(),
            current_turn: PlayerIdx(0),
            turn_phase: TurnPhase::Draw,
            turn_seconds,
            play_time: Seconds(0),
            seed,
        }
    }

    #[cfg(test)]
    pub fn test_new(
        players: Vec<PlayerBoard>,
        current_turn: PlayerIdx,
        card_dealer: CardDealer,
        turn_seconds: Seconds,
    ) -> Self {
        Self {
            game_id: GameId("test_room".to_string()),
            phase: GamePhase::Waiting,
            players,
            card_dealer,
            scale_manager: ScaleManager::new(),
            current_turn,
            turn_phase: TurnPhase::Draw,
            turn_seconds,
            play_time: Seconds(0),
            seed: Seed(0),
        }
    }

    // ── Player management ─────────────────────────────────────

    pub fn add_player(&mut self, player_id: PlayerId, player_idx: PlayerIdx) -> bool {
        if self.players.len() >= 2 {
            return false;
        }
        if self.players.iter().any(|p| p.player_id == Some(player_id)) {
            return false;
        }
        if self.players.iter().any(|p| p.player_idx == player_idx) {
            return false;
        }
        self.players.push(PlayerBoard::new(player_id, player_idx));
        if self.players.len() == 2 {
            self.start_game();
        }
        true
    }

    pub fn remove_player(&mut self, player_idx: PlayerIdx) {
        let before = self.players.len();
        self.players.retain(|p| p.player_idx != player_idx);
        if self.players.len() != before && self.phase == GamePhase::Playing {
            self.phase = GamePhase::Finished;
        }
        // Adjust current turn if needed
        if self.current_turn.0 >= PlayerIdx(self.players.len()).0 {
            self.current_turn = PlayerIdx(0);
        }
    }

    pub fn player(&self, idx: PlayerIdx) -> Option<&PlayerBoard> {
        self.players.iter().find(|p| p.player_idx == idx)
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
        self.current_turn = PlayerIdx(rand::random_range(0..=1));
    }

    // ── Turn & phase helpers ──────────────────────────────────

    pub fn current_player_idx(&self) -> Option<PlayerIdx> {
        if self.phase != GamePhase::Playing {
            return None;
        }
        Some(self.current_turn)
    }

    pub fn is_playing(&self) -> bool {
        self.phase == GamePhase::Playing
    }

    pub fn scale(&self, scale_idx: ScaleIdx) -> &Option<Scale> {
        &self.scale_manager.scales[scale_idx.as_usize()]
    }

    // ── Winning & end‑game ────────────────────────────────────
    pub fn winner(&self) -> Option<&PlayerBoard> {
        self.players.iter().find(|p| p.has_won())
    }

    pub fn check_win_condition(&mut self, player_idx: PlayerIdx) -> bool {
        if self.players[player_idx.0].has_won() {
            self.phase = GamePhase::Finished;
            return true;
        }
        false
    }

    // Helper to get vector index from PlayerIdx
    fn current_vec_index_of(&self, player_idx: PlayerIdx) -> usize {
        self.players
            .iter()
            .position(|p| p.player_idx == player_idx)
            .expect("Player not found")
    }

    // ── Core draw ─────────────────────────────────────────────
    fn draw_one(&mut self) -> Option<Card> {
        self.card_dealer.draw_one()
    }

    // ── Hand refill ──────────────────────────────────────────
    fn execute_draw_phase(&mut self, player_idx: PlayerIdx) -> Vec<Card> {
        let mut drawn = vec![];
        let idx = self.current_vec_index_of(player_idx);
        if let Some(card) = self.draw_one() {
            drawn.push(self.players[idx].draw_to_hand(card));
        }
        while self.players[idx].hand.len() < 5 {
            if let Some(card) = self.draw_one() {
                drawn.push(self.players[idx].draw_to_hand(card));
            } else {
                break;
            }
        }
        drawn
    }

    fn refill_hand_to_five(&mut self, player_idx: PlayerIdx) -> Option<Vec<Card>> {
        if self.phase != GamePhase::Playing {
            return None;
        }

        let mut drawn = vec![];
        let idx = self.current_vec_index_of(player_idx);

        if self.players[idx].hand_len() == 0 {
            while self.players[idx].hand.len() < 5 {
                if let Some(card) = self.draw_one() {
                    self.players[idx].draw_to_hand(card);
                    drawn.push(card);
                } else {
                    break;
                }
            }
        }
        Some(drawn)
    }

    pub fn advance_turn(&mut self) -> PlayerIdx {
        assert_eq!(self.phase, GamePhase::Playing, "Cannot advance turn: not playing");
        let current_raw = self.current_turn.0;
        let next_raw = (current_raw + 1) % self.players.len();
        self.current_turn = PlayerIdx(next_raw);
        self.turn_phase = TurnPhase::Draw;
        self.players[next_raw].player_idx
    }

    // ── Scale interaction ────────────────────────────────────

    pub fn can_place_on_scale(&self, scale_id: ScaleIdx, card: &Card) -> bool {
        self.scale_manager.can_place_on_scale(scale_id, card)
    }

    // ── Apply an action from the current player ──────────────
    pub fn apply_move(
        &mut self,
        player_idx: PlayerIdx,
        action: Action
    ) -> Result<MoveResult, MoveError> {

        let idx = self
            .players
            .iter()
            .position(|p| p.player_idx == player_idx)
            .ok_or(MoveError::NotAllowed)?;

        if self.phase != GamePhase::Playing {
            return Err(MoveError::NotAllowed);
        }

        if self.players[idx].player_idx != self.current_turn {
            return Err(MoveError::NotYourTurn);
        }

        match action {
            Action::Draw => {
                if self.turn_phase != TurnPhase::Draw {
                    return Err(MoveError::NotAllowed);
                }
                let drawn = self.execute_draw_phase(player_idx);
                self.turn_phase = TurnPhase::Play;
                Ok(MoveResult {
                    success: MoveSuccess::Success,
                    drawn_cards: Some(drawn),
                    discarded_cards: None,
                })
            }

            Action::OpenScale { hand_idx } => {

                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let card = self.players[idx].hand_card(hand_idx).cloned()
                    .ok_or(MoveError::InvalidIndex { kind: "hand_idx".into() })?;

                if card.value != 1 {
                    return Err(MoveError::DoesNotFit);
                }

                let card = self.players[idx].take_from_hand(hand_idx.as_usize()).unwrap();
                let success = self.scale_manager.open_scale(card)?;
                let drawn = self.refill_hand_to_five(player_idx);
                Ok(MoveResult {
                    success,
                    drawn_cards: drawn,
                    discarded_cards: None,
                })
            }

            Action::PlayHand { hand_idx, scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let card = self.players[idx].hand_card(hand_idx).cloned()
                    .ok_or(MoveError::InvalidIndex { kind: "hand_idx".into() })?;

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    return Err(MoveError::DoesNotFit);
                }

                let card = self.players[idx].take_from_hand(hand_idx.as_usize()).unwrap();
                
                let (success, discarded) = self.scale_manager.place_on_scale(scale_idx, card)?;
                
                if let Some(cards) = &discarded {
                    self.card_dealer.return_to_discard(cards.iter().cloned());
                }

                let drawn = self.refill_hand_to_five(player_idx);
                
                Ok(MoveResult {
                    success,
                    drawn_cards: drawn,
                    discarded_cards: discarded,
                })
            }

            Action::PlayPersonal { scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let card = self.players[idx].pop_personal()
                    .ok_or(MoveError::InvalidIndex { kind: "personal".into() })?;

                if card.value == 1 {
                    let success = match self.scale_manager.open_scale(card) {
                        Ok(success) => success,
                        Err(e) => {
                            self.players[idx].push_personal(card);
                            return Err(e);
                        }
                    };

                    let drawn = self.refill_hand_to_five(player_idx);

                    Ok(MoveResult {
                            success,
                            drawn_cards: drawn,
                            discarded_cards: None,
                        }
                    )
                } else {
                    if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                        self.players[idx].push_personal(card);
                        return Err(MoveError::DoesNotFit);
                    }

                    let (success, discarded) = self.scale_manager.place_on_scale(scale_idx, card)?;

                    if let Some(cards) = &discarded {
                        self.card_dealer.return_to_discard(cards.iter().cloned());
                    }

                    let drawn = self.refill_hand_to_five(player_idx);

                    Ok(MoveResult {
                        success,
                        drawn_cards: drawn,
                        discarded_cards: discarded,
                    })
                }
            }

            Action::PlaySide { stack_idx, scale_idx } => {
                if self.turn_phase != TurnPhase::Play {
                    return Err(MoveError::NotAllowed);
                }

                let card = self.players[idx].take_from_side(stack_idx)
                    .ok_or(MoveError::InvalidIndex { kind: "stack".into() })?;

                if !self.scale_manager.can_place_on_scale(scale_idx, &card) {
                    self.players[idx].hand.push(card);
                    return Err(MoveError::DoesNotFit);
                }

                let (success, discarded) = self.scale_manager.place_on_scale(scale_idx, card)?;
                
                if let Some(cards) = &discarded {
                    self.card_dealer.return_to_discard(cards.iter().cloned());
                }
                
                let drawn = self.refill_hand_to_five(player_idx);
                Ok(MoveResult {
                    success,
                    drawn_cards: drawn,
                    discarded_cards: discarded,
                })
            }

            Action::MoveToSide { hand_idx, stack_idx } => {
                    if self.turn_phase != TurnPhase::Play {
                        return Err(MoveError::NotAllowed);
                    }

                    let card = self.players[idx].take_from_hand(hand_idx.as_usize())
                        .ok_or(MoveError::InvalidIndex { kind: "hand_idx".into() })?;

                    if self.players[idx].place_on_side(stack_idx, card) {
                        if self.players[idx].has_won() {
                            self.phase = GamePhase::Finished;

                            Ok(MoveResult {
                                success: MoveSuccess::GameWon {
                                    winner_idx: player_idx,
                                },
                                drawn_cards: None,
                                discarded_cards: None,
                            })
                        } else {
                            self.advance_turn();

                            Ok(MoveResult {
                                success: MoveSuccess::TurnEnded,
                                drawn_cards: None,
                                discarded_cards: None,
                            })
                        }
                    } else {
                        self.players[idx].hand.push(card);

                        return Err(MoveError::InvalidIndex {
                            kind: "stack".into(),
                        });
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
                    let drawn = self.refill_hand_to_five(player_idx);
                    Ok(MoveResult {
                        success: MoveSuccess::Success,
                        drawn_cards: drawn,
                        discarded_cards: None,
                    })
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

    use crate::core::game_index::{HandIdx, ScaleIdx, StackIdx};
    use crate::core::player::{PlayerId, PlayerIdx};
    use uuid::Uuid;

    // ── Helpers ─────────────────────────────────────────────────

    fn make_game() -> GameState {
        let mut gs = GameState::new(
            GameId("room".into()),
            Seed(0),
            13,
            5,
            Seconds(30)
        );
        gs.start_game(); // ensure start_game is pub(crate) or use make_game_via_add
        gs
    }

    fn empty_game_for_lobby() -> GameState {
        let mut gs = GameState::new(GameId("test".into()), Seed(0), 13, 5, Seconds(30));
        gs.players.clear();
        gs.phase = GamePhase::Waiting;
        gs
    }

    fn hand_len(gs: &GameState, idx: usize) -> usize {
        gs.players[idx].hand.len()
    }

    fn personal_len(gs: &GameState, idx: usize) -> usize {
        gs.players[idx].personal.len()
    }

    fn current_turn_player(gs: &GameState) -> PlayerIdx {
        gs.current_turn
    }

    fn card(value: u8) -> Card {
        Card { suit: Suit::Hearts, value, deck: DeckColor::Red }
    }

    // ── Initialization & player management ───────────────────
    #[test]
    fn new_game_has_two_players_waiting() {
        let gs = GameState::new(GameId("r".into()), Seed(0), 13, 5, Seconds(30));
        assert_eq!(gs.players.len(), 2);
        assert_eq!(gs.phase, GamePhase::Waiting);
        assert_eq!(gs.players[0].player_idx, PlayerIdx(0));
        assert_eq!(gs.players[1].player_idx, PlayerIdx(1));
    }

    #[test]
    fn start_game_deals_cards() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        for p in &gs.players {
            assert_eq!(p.personal.len(), 13);
            assert_eq!(p.hand.len(), 5);
            assert_eq!(p.side.iter().all(|s| s.is_empty()), true);
        }
        assert_eq!(gs.phase, GamePhase::Playing);
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(current_turn_player(&gs), PlayerIdx(0));
    }

    #[test]
    fn add_player_first_succeeds() {
        let mut gs = empty_game_for_lobby();

        assert!(gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(42)));
        assert_eq!(gs.players.len(), 1);
        assert_eq!(gs.players[0].player_idx, PlayerIdx(42));
        assert_eq!(gs.phase, GamePhase::Waiting);
    }

    #[test]
    fn add_player_beyond_two_fails() {
        let mut gs = make_game();
        assert!(!gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(2)));
    }

    #[test]
    fn add_player_second_starts_game() {
        let mut gs = empty_game_for_lobby();
        let id0 = PlayerId(Uuid::from_u128(100));
        let id1 = PlayerId(Uuid::from_u128(200));
        assert!(gs.add_player(id0, PlayerIdx(0)));
        assert!(gs.add_player(id1, PlayerIdx(1)));
        assert_eq!(gs.players.len(), 2);
        assert_eq!(gs.phase, GamePhase::Playing);
        assert_eq!(gs.players[0].personal.len(), 13);
        assert_eq!(gs.players[0].hand.len(), 5);
        assert_eq!(gs.players[1].personal.len(), 13);
        assert_eq!(gs.players[1].hand.len(), 5);
    }

    #[test]
    fn add_player_third_fails() {
        let mut gs = empty_game_for_lobby();
        let id0 = PlayerId(Uuid::from_u128(100));
        let id1 = PlayerId(Uuid::from_u128(200));
        let id2 = PlayerId(Uuid::from_u128(300));
        gs.add_player(id0, PlayerIdx(0));
        gs.add_player(id1, PlayerIdx(1));
        assert!(!gs.add_player(id2, PlayerIdx(2)));
        assert_eq!(gs.players.len(), 2);
    }

    #[test]
    fn add_player_duplicate_id_fails() {
        let mut gs = empty_game_for_lobby();
        // Start with an empty board for this test
        gs.players.clear();
        gs.phase = GamePhase::Waiting;

        let player_id = PlayerId(Uuid::new_v4());
        assert!(gs.add_player(player_id, PlayerIdx(0)));
        assert_eq!(gs.players.len(), 1);

        // Try adding the same external ID again – must fail
        assert!(!gs.add_player(player_id, PlayerIdx(1)));
        assert_eq!(gs.players.len(), 1);
    }

    #[test]
    fn add_player_duplicate_idx_fails() {
        let mut gs = empty_game_for_lobby();

        // Start with 0 players for this test
        gs.players.clear();
        gs.phase = GamePhase::Waiting;

        // Add first player with a custom PlayerIdx
        assert!(gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(5)));
        assert_eq!(gs.players.len(), 1);

        // Attempt to add another player with the *same* PlayerIdx → must fail
        assert!(!gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(5)));
        assert_eq!(gs.players.len(), 1);
    }

    #[test]
    fn remove_player_during_play_ends_game() {
        let mut gs = empty_game_for_lobby();

        // Use two different external IDs
        let id0 = PlayerId(Uuid::from_u128(100));
        let id1 = PlayerId(Uuid::from_u128(200));

        assert!(gs.add_player(id0, PlayerIdx(0)));
        assert!(gs.add_player(id1, PlayerIdx(1)));
        assert_eq!(gs.phase, GamePhase::Playing);   // game has started

        // Remove one player – game should finish
        gs.remove_player(PlayerIdx(1));
        assert_eq!(gs.players.len(), 1);
        assert_eq!(gs.phase, GamePhase::Finished);
    }

    #[test]
    fn remove_during_play_ends_game() {
        let mut gs = make_game();
        let before = gs.players.len();
        gs.remove_player(PlayerIdx(1));
        assert_eq!(gs.phase, GamePhase::Finished);
        assert_eq!(gs.players.len(), before - 1);
    }

    #[test]
    fn remove_player_adjusts_current_turn_if_needed() {
        let mut gs = empty_game_for_lobby();
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(0));
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(1));
        gs.current_turn = PlayerIdx(1);
        gs.remove_player(PlayerIdx(1));
        assert_eq!(gs.current_turn, PlayerIdx(0));
    }

    #[test]
    fn remove_player_nonexistent_does_nothing() {
        let mut gs = empty_game_for_lobby();
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(0));
        let before_len = gs.players.len();
        gs.remove_player(PlayerIdx(99));
        assert_eq!(gs.players.len(), before_len);
        assert_eq!(gs.phase, GamePhase::Waiting);
    }

    // ── Turn / phase helpers ─────────────────────────────────
    #[test]
    fn not_your_turn_rejected() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let result = gs.apply_move(PlayerIdx(1), Action::Draw);
        assert_eq!(result, Err(MoveError::NotYourTurn));
    }

    #[test]
    fn wrong_phase_action_rejected() {
        let mut gs = make_game();
        gs.phase = GamePhase::Waiting;
        let result = gs.apply_move(PlayerIdx(0), Action::Draw);
        assert_eq!(result, Err(MoveError::NotAllowed));
    }

    // ── Draw action ──────────────────────────────────────────
    #[test]
    fn draw_in_draw_phase_works() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let player_idx = current_turn_player(&gs);
        let initial_hand = hand_len(&gs, 0);
        let result = gs.apply_move(player_idx, Action::Draw);

        assert!(result.is_ok());

        let move_result = result.unwrap();

        assert_eq!(move_result.success, MoveSuccess::Success);
        assert!(move_result.discarded_cards.is_none());
        assert!(move_result.drawn_cards.is_some());

        assert_eq!(initial_hand, 5);
        assert!(hand_len(&gs, 0) >= 6);
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
        gs.current_turn = PlayerIdx(0);

        while gs.card_dealer.remaining() > 0 {
            gs.card_dealer.draw_one();
        }
        let result = gs.apply_move(PlayerIdx(0), Action::Draw).unwrap();

        assert_eq!(result.success, MoveSuccess::Success);
        assert!(result.discarded_cards.is_none());
        assert_eq!(gs.turn_phase, TurnPhase::Play);
    }

    // ── OpenScale action ─────────────────────────────────────
    #[test]
    fn open_scale_with_ace() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        gs.players[0].hand[0] = Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red };
        let hand_size_before = hand_len(&gs, 0);
        let result = gs.apply_move(PlayerIdx(0), Action::OpenScale { hand_idx: HandIdx(0) });
        assert_eq!(
            result,
            Ok(MoveResult {
                success: MoveSuccess::ScaleOpened {
                    scale_id: ScaleIdx(0),
                    placed_card: card(1),
                },
                drawn_cards: Some(vec![]),
                discarded_cards: None,
            })
        );
        assert_eq!(gs.scale_manager.scales.len(), 8);
        assert_eq!(hand_len(&gs, 0), hand_size_before - 1);
    }

    #[test]
    fn open_scale_with_non_ace_rejected() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        gs.players[0].hand[0] = Card { suit: Suit::Hearts, value: 5, deck: DeckColor::Red };
        let result = gs.apply_move(PlayerIdx(0), Action::OpenScale { hand_idx: HandIdx(0) });
        assert_eq!(result, Err(MoveError::DoesNotFit));
        assert_eq!(hand_len(&gs, 0), 6);
    }

    // ─── Scale (borrow a Scale by id) ─────────────────────────────────────
    #[test]
    fn scale_returns_reference_to_existing_scale() {
        let mut gs = empty_game_for_lobby();
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(0));
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(1));
        let mut scale = Scale::new(ScaleIdx(0));
        scale.push(Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red }).unwrap();
        gs.scale_manager.scales[0] = Some(scale);
        let retrieved = gs.scale(ScaleIdx(0));
        assert_eq!(retrieved.as_ref().unwrap().scale_idx, ScaleIdx(0));
    }

    // ── can_place_on_scale ──────────────────────────────────────
    #[test]
    fn can_place_on_scale_delegates_to_scale_manager() {
        let mut gs = empty_game_for_lobby();
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(0));
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(1));

        // Create a scale and place it directly into slot 0
        let mut scale = Scale::new(ScaleIdx(0));
        scale.push(Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red }).unwrap();
        gs.scale_manager.scales[0] = Some(scale); // assign to index 0

        let card = Card { suit: Suit::Hearts, value: 2, deck: DeckColor::Red };
        assert!(gs.can_place_on_scale(ScaleIdx(0), &card));

        let bad_card = Card { suit: Suit::Spades, value: 3, deck: DeckColor::Blue };
        assert!(!gs.can_place_on_scale(ScaleIdx(0), &bad_card));
    }

    #[test]
    fn can_place_on_scale_returns_false_for_invalid_scale_id() {
        let gs = empty_game_for_lobby();
        let card = Card { suit: Suit::Clubs, value: 1, deck: DeckColor::Red };
        assert!(!gs.can_place_on_scale(ScaleIdx(99), &card));
    }

    // ── PlayHand action ──────────────────────────────────────
    #[test]
    fn play_hand_valid() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);

        gs.players[0].hand[0] = card(1);
        let _ = gs.apply_move(PlayerIdx(0), Action::OpenScale {
            hand_idx: HandIdx(0),
        });

        gs.players[0].hand[0] = card(2);

        let result = gs
            .apply_move(
                PlayerIdx(0),
                Action::PlayHand {
                    hand_idx: HandIdx(0),
                    scale_idx: ScaleIdx(0),
                },
            )
            .unwrap();

        assert_eq!(
            result.success,
            MoveSuccess::ScalePlaced {
                scale_id: ScaleIdx(0),
                completed: false,
                placed_card: card(2),
            }
        );

        assert_eq!(result.discarded_cards, None);
        assert_eq!(hand_len(&gs, 0), 4);
    }

    #[test]
    fn play_hand_invalid_scale() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        let result = gs.apply_move(PlayerIdx(0), Action::PlayHand { hand_idx: HandIdx(0), scale_idx: ScaleIdx(99) });
        assert_eq!(result, Err(MoveError::DoesNotFit));
    }

    // ───── Player (getter by player_idx) ──────────────────────────────────
    #[test]
    fn player_returns_some_for_existing_id() {
        let mut gs = empty_game_for_lobby();
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(7));
        gs.add_player(PlayerId(Uuid::nil()), PlayerIdx(8));
        let p = gs.player(PlayerIdx(7));
        assert!(p.is_some());
        assert_eq!(p.unwrap().player_idx, PlayerIdx(7));
    }

    #[test]
    fn player_returns_none_for_missing_id() {
        let gs = empty_game_for_lobby();
        assert!(gs.player(PlayerIdx(99)).is_none());
    }

    // ── PlayPersonal action ──────────────────────────────────
    #[test]
    fn play_personal_to_scale() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        gs.players[0].personal[12] = Card {
            suit: Suit::Hearts,
            value: 2,
            deck: DeckColor::Red,
        };
        let mut scale = Scale::new(ScaleIdx(0));
        scale.push(Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red }).unwrap();
        gs.scale_manager.scales[0] = Some(scale);
        let result = gs.apply_move(PlayerIdx(0), Action::PlayPersonal { scale_idx: ScaleIdx(0) });
        let move_result = result.unwrap();

        assert_eq!(
            move_result.success,
            MoveSuccess::ScalePlaced {
                scale_id: ScaleIdx(0),
                completed: false,
                placed_card: card(2),
            }
        );

        assert!(move_result.discarded_cards.is_none());
        assert_eq!(personal_len(&gs, 0), 12);
    }

    // ── PlaySide action ──────────────────────────────────────
    #[test]
    fn play_side_to_scale() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        gs.players[0].side[0].push(card(2));
        let mut scale = Scale::new(ScaleIdx(0));
        scale.push(Card { suit: Suit::Hearts, value: 1, deck: DeckColor::Red }).unwrap();
        gs.scale_manager.scales[0] = Some(scale);
        let result = gs.apply_move(PlayerIdx(0), Action::PlaySide { stack_idx: StackIdx(0), scale_idx: ScaleIdx(0) });
        assert_eq!(
            result,
            Ok(MoveResult {
                success: MoveSuccess::ScalePlaced {
                    scale_id: ScaleIdx(0),
                    completed: false,
                    placed_card: card(2),
                },
                drawn_cards: Some(vec![]),
                discarded_cards: None,
            })
        );
        assert!(gs.players[0].side[0].is_empty());
    }

    // ── MoveToSide action ────────────────────────────────────
    #[test]
    fn move_to_side_ends_turn() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let result = gs.apply_move(PlayerIdx(0), Action::Draw).unwrap();

        assert_eq!(result.success, MoveSuccess::Success);
        assert_eq!(hand_len(&gs, 0), 6);

        let initial_turn = gs.current_turn;

        let result = gs
            .apply_move(
                PlayerIdx(0),
                Action::MoveToSide {
                    hand_idx: HandIdx(0),
                    stack_idx: StackIdx(0),
                },
            )
            .unwrap();

        assert_eq!(result.success, MoveSuccess::TurnEnded);
        assert!(result.drawn_cards.is_none());
        assert!(result.discarded_cards.is_none());

        assert_eq!(gs.current_turn, PlayerIdx((initial_turn.0 + 1) % 2));
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(gs.players[0].side[0].len(), 1);
        assert_eq!(hand_len(&gs, 0), 5);
    }

    // ── MovePersonalToSide ───────────────────────────────────
    #[test]
    fn move_king_from_personal_to_side() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let result = gs.apply_move(PlayerIdx(0), Action::Draw).unwrap();
        assert_eq!(result.success, MoveSuccess::Success);

        let king = Card {
            suit: Suit::Spades,
            value: 13,
            deck: DeckColor::Blue,
        };

        gs.players[0].personal.clear();
        gs.players[0].personal.push(king);

        let result = gs
            .apply_move(
                PlayerIdx(0),
                Action::MovePersonalToSide {
                    stack_idx: StackIdx(0),
                },
            )
            .unwrap();

        assert_eq!(result.success, MoveSuccess::Success);
        assert!(result.discarded_cards.is_none());

        assert!(gs.players[0].personal.is_empty());
        assert_eq!(gs.players[0].side[0].len(), 1);
    }

    #[test]
    fn move_non_king_from_personal_rejected() {
        let mut gs = make_game();

        gs.current_turn = PlayerIdx(0);
        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        let card = Card { value: 5, ..gs.players[0].personal[0] };
        gs.players[0].personal.clear();
        gs.players[0].personal.push(card);
        let result = gs.apply_move(PlayerIdx(0), Action::MovePersonalToSide { stack_idx: StackIdx(0) });
        assert_eq!(result, Err(MoveError::NotAllowed));
    }

    // ── Win condition ────────────────────────────────────────
    #[test]
    fn win_on_move_to_side_when_both_piles_empty() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let _ = gs.apply_move(PlayerIdx(0), Action::Draw);
        gs.players[0].personal.clear();
        gs.players[0].hand = vec![Card { value: 2, ..gs.players[0].hand[0] }];
        let result = gs.apply_move(
            PlayerIdx(0),
            Action::MoveToSide {
                hand_idx: HandIdx(0),
                stack_idx: StackIdx(0),
            },
        );

        assert_eq!(
            result,
            Ok(MoveResult {
                success: MoveSuccess::GameWon {
                    winner_idx: PlayerIdx(0),
                },
                drawn_cards: None,
                discarded_cards: None,
            })
        );

        assert_eq!(gs.phase, GamePhase::Finished);
    }

    // ── Refill hand ──────────────────────────────────────────

    #[test]
    fn refill_hand() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        let MoveResult {
            success: res,
            drawn_cards: drawn,
            discarded_cards: _,
        } = gs.apply_move(PlayerIdx(0), Action::Draw).unwrap();

        assert_eq!(res, MoveSuccess::Success);
        assert!(drawn.as_ref().unwrap().len() >= 1);
        assert!(!drawn.as_ref().unwrap().is_empty());

        gs.players[0].hand = vec![Card {
            value: 1,
            suit: Suit::Hearts,
            deck: DeckColor::Red,
        }];

        assert_eq!(gs.players[0].hand.len(), 1);

        let MoveResult {
            success: res,
            drawn_cards: refill_cards,
            discarded_cards: _,
        } = gs
            .apply_move(
                PlayerIdx(0),
                Action::OpenScale {
                    hand_idx: HandIdx(0),
                },
            )
            .unwrap();

        assert_eq!(
            res,
            MoveSuccess::ScaleOpened {
                scale_id: ScaleIdx(0),
                placed_card: card(1),
            }
        );

        assert_eq!(gs.players[0].hand.len(), 5);

        let refill_cards = refill_cards.unwrap();
        assert_eq!(refill_cards.len(), 5);

        for (i, card) in refill_cards.iter().enumerate() {
            assert_eq!(gs.players[0].hand[i], *card);
        }
    }

    // ── Simulated partial game ───────────────────────────────
    #[test]
    fn simulated_game_partial() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        // Turn 1: Player 0
        let result = gs.apply_move(PlayerIdx(0), Action::Draw).unwrap();
        assert_eq!(result.success, MoveSuccess::Success);
        assert!(result.drawn_cards.is_some());
        assert_eq!(hand_len(&gs, 0), 6);

        gs.players[0].hand[0] = Card { value: 1, suit: Suit::Hearts, deck: DeckColor::Red };

        let result = gs
            .apply_move(PlayerIdx(0), Action::OpenScale {
                hand_idx: HandIdx(0),
            })
            .unwrap();

        assert_eq!(
            result.success,
            MoveSuccess::ScaleOpened {
                scale_id: ScaleIdx(0),
                placed_card: card(1),
            }
        );

        assert_eq!(result.drawn_cards, Some(vec![]));
        assert_eq!(result.discarded_cards, None);

        assert_eq!(hand_len(&gs, 0), 5);
        assert_eq!(gs.scale_manager.scales.len(), 8);

        gs.players[0].hand[0] = Card {
            value: 2,
            suit: Suit::Hearts,
            deck: DeckColor::Red,
        };

        let result = gs
            .apply_move(
                PlayerIdx(0),
                Action::PlayHand {
                    hand_idx: HandIdx(0),
                    scale_idx: ScaleIdx(0),
                },
            )
            .unwrap();

        assert_eq!(
            result.success,
            MoveSuccess::ScalePlaced {
                scale_id: ScaleIdx(0),
                completed: false,
                placed_card: card(2),
            }
        );

        assert_eq!(result.drawn_cards, Some(vec![]));
        assert_eq!(result.discarded_cards, None);

        assert_eq!(hand_len(&gs, 0), 4);

        let result = gs
            .apply_move(
                PlayerIdx(0),
                Action::MoveToSide {
                    hand_idx: HandIdx(0),
                    stack_idx: StackIdx(0),
                },
            )
            .unwrap();

        assert_eq!(result.success, MoveSuccess::TurnEnded);
        assert!(result.drawn_cards.is_none());
        assert!(result.discarded_cards.is_none());

        assert_eq!(gs.current_turn, PlayerIdx(1));
        assert_eq!(gs.turn_phase, TurnPhase::Draw);
        assert_eq!(gs.players[0].side[0].len(), 1);
        assert_eq!(hand_len(&gs, 0), 3);

        // Turn 2: Player 1
        let result = gs.apply_move(PlayerIdx(1), Action::Draw).unwrap();
        assert_eq!(result.success, MoveSuccess::Success);
        assert!(result.drawn_cards.is_some());
        assert_eq!(hand_len(&gs, 1), 6);

        gs.players[1].hand[0] = Card {
            value: 1,
            suit: Suit::Hearts,
            deck: DeckColor::Red,
        };

        assert_eq!(
            gs.apply_move(
                PlayerIdx(1),
                Action::OpenScale {
                    hand_idx: HandIdx(0),
                },
            ),
            Ok(MoveResult {
                success: MoveSuccess::ScaleOpened {
                    scale_id: ScaleIdx(1),
                    placed_card: card(1),
                },
                drawn_cards: Some(vec![]),
                discarded_cards: None,
            })
        );

        assert_eq!(hand_len(&gs, 1), 5);

        gs.players[1].hand[0] = Card {
            value: 2,
            suit: Suit::Hearts,
            deck: DeckColor::Red,
        };

        assert_eq!(
            gs.apply_move(
                PlayerIdx(1),
                Action::PlayHand {
                    hand_idx: HandIdx(0),
                    scale_idx: ScaleIdx(1),
                },
            ),
            Ok(MoveResult {
                success: MoveSuccess::ScalePlaced {
                    scale_id: ScaleIdx(1),
                    completed: false,
                    placed_card: card(2),
                },
                drawn_cards: Some(vec![]),
                discarded_cards: None,
            })
        );

        assert_eq!(hand_len(&gs, 1), 4);
    }

    // ── Utility methods ──────────────────────────────────────
    #[test]
    fn current_player_idx_returns_current_turn() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);

        assert_eq!(gs.current_player_idx(), Some(PlayerIdx(0)));
        gs.apply_move(PlayerIdx(0), Action::Draw).unwrap();
        gs.apply_move(PlayerIdx(0), Action::MoveToSide { hand_idx: HandIdx(0), stack_idx: StackIdx(0) }).unwrap();
        assert_eq!(gs.current_player_idx(), Some(PlayerIdx(1)));
    }

    #[test]
    fn current_player_id_returns_correct_id() {
        let mut gs = make_game();
        gs.current_turn = PlayerIdx(0);
        // This method now returns Option<PlayerIdx>
        assert_eq!(gs.current_player_idx(), Some(PlayerIdx(0)));
    }

    #[test]
    fn current_player_id_none_if_no_players() {
        let mut gs = empty_game_for_lobby();
        gs.remove_player(PlayerIdx(1));
        gs.remove_player(PlayerIdx(0));
        assert!(gs.current_player_idx().is_none());
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
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        assert!(gs.players[0].has_won());
        assert_eq!(gs.winner().unwrap().player_idx, PlayerIdx(0));
    }

    #[test]
    fn check_win_condition_sets_finished() {
        let mut gs = make_game();
        gs.players[0].personal.clear();
        gs.players[0].hand.clear();
        assert!(!gs.check_win_condition(PlayerIdx(1)));
        assert_eq!(gs.phase, GamePhase::Playing);
        assert!(gs.check_win_condition(PlayerIdx(0)));
        assert_eq!(gs.phase, GamePhase::Finished);
        assert!(gs.check_win_condition(PlayerIdx(0)));
    }
}
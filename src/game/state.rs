use rand::RngExt;
use serde::{Deserialize, Serialize};
use crate::game::action::Action;
use crate::game::card::{Card, Deck};
use crate::game::board::PlayerBoard;
use crate::game::scale::Scale;
use crate::game::result::PlayResult;
use crate::game::turn_phase::TurnPhase;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GamePhase { Waiting, Playing, Finished }

#[derive(Debug)]
pub struct GameState {
    pub room_id: String,
    pub phase: GamePhase,
    pub players: Vec<PlayerBoard>,

    pub draw_pile: Deck,
    pub discard_pile: Vec<Card>,

    pub scales: Vec<Scale>,

    pub current_turn: usize,
    pub turn_phase: TurnPhase,
    pub turn_seconds: u64,
    pub seed: u64
}

impl GameState {
    pub fn new(room_id: String, turn_seconds: u64) -> Self {
        let seed = rand::rng().random();

        Self {
            room_id,
            phase: GamePhase::Waiting,
            players: vec![],
            draw_pile: Deck::double(),
            discard_pile: vec![],
            scales: vec![],
            current_turn: 0,
            turn_phase: TurnPhase::Draw,
            turn_seconds,
            seed
        }
    }

    // ── Player management ─────────────────────────────────────────────────────

    pub fn add_player(&mut self, player_id: String, username: String) -> bool {
        if self.players.iter().any(|p| p.player_id == player_id) { return false; }
        if self.players.len() >= 2 { return false; }

        self.players.push(PlayerBoard::new(player_id, username));

        if self.players.len() == 2 { self.start_game(); }

        true
    }

    pub fn remove_player(&mut self, player_id: &str) {
        let before = self.players.len();
        self.players.retain(|p| p.player_id != player_id);

        if self.players.len() != before && self.phase == GamePhase::Playing {
            self.phase = GamePhase::Finished;
        }

        if self.current_turn >= self.players.len() {
            self.current_turn = 0;
        }
    }

    fn start_game(&mut self) {
        // Reset piles
        self.draw_pile = Deck::double();
        self.discard_pile.clear();

        self.draw_pile.shuffle_with_seed(self.seed);

        // Deal cards
        for board in &mut self.players {
            board.personal = self.draw_pile.deal(13);
            board.hand = self.draw_pile.deal(5);

            // Optional: clear side stacks to be safe
            board.side = [vec![], vec![], vec![], vec![]];
        }

        // Reset scales
        self.scales.clear();

        // Reset turn state
        self.phase = GamePhase::Playing;
        self.turn_phase = TurnPhase::Draw;
        self.current_turn = 0;
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn current_player_id(&self) -> Option<&str> {
        self.players.get(self.current_turn).map(|p| p.player_id.as_str())
    }

    pub fn winner(&self) -> Option<&PlayerBoard> {
        self.players.iter().find(|p| p.has_won())
    }

    pub fn state_sync_for(&self, player_id: &str)
        -> Option<(PlayerBoard, usize, Option<Card>, [Vec<Card>; 4], String)>
    {
        let me  = self.players.iter().find(|p| p.player_id == player_id)?;
        let opp = self.players.iter().find(|p| p.player_id != player_id)?;
        Some((
            me.clone(),
            opp.personal.len(),
            opp.personal_top().cloned(),
            opp.side.clone(),
            opp.username.clone(),
        ))
    }


    // ── Scale logic ───────────────────────────────────────────────────────────

    fn find_or_open_scale(&mut self, card: &Card) -> Option<usize> {
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

    fn place_on_scale(&mut self, card: Card) -> PlayResult {
        let Some(scale_idx) = self.find_or_open_scale(&card) else {
            return PlayResult::DoesNotFit;
        };
        
        self.scales[scale_idx].push(card);

        let completed = self.scales[scale_idx].is_complete();

        if completed {
            let cards = self.scales[scale_idx].cards.drain(..);
            self.discard_pile.extend(cards);
        }

        PlayResult::Ok {
            scale_id: scale_idx,
            completed 
        }
    }

    // ── Play actions ──────────────────────────────────────────────────────────

    fn play_hand_to_scale(&mut self, hand_index: usize) -> PlayResult {
        let idx = self.current_idx();

        if let Some(err) = self.ensure_turn(idx) {
            return err;
        }

        let Some(card) = self.players[idx].take_from_hand(hand_index) else {
            return PlayResult::InvalidIndex;
        };

        let result = self.place_on_scale(card);

        if matches!(result, PlayResult::DoesNotFit) {
            self.players[idx].hand.push(card);
        }

        self.refill_hand_if_empty(idx);

        result
    }

    fn play_personal_to_scale(&mut self) -> PlayResult {
        let idx = self.current_idx();

        if let Some(err) = self.ensure_turn(idx) {
            return err;
        }

        let Some(card) = self.players[idx].pop_personal() else {
            return PlayResult::InvalidIndex;
        };

        let result = self.place_on_scale(card);

        if matches!(result, PlayResult::DoesNotFit) {
            self.players[idx].hand.push(card);
        }

        self.refill_hand_if_empty(idx);

        result
    }

    fn play_personal_to_side(&mut self, stack: usize) -> PlayResult {
        let idx = self.current_idx();

        if let Some(err) = self.ensure_turn(idx) {
            return err;
        }

        match self.players[idx].personal_top() {
            None => PlayResult::InvalidIndex,

            Some(card) if card.value != 13 => PlayResult::NotAllowed,

            Some(_) => {
                let card = self.players[idx].pop_personal().unwrap();

                let success = self.players[idx].place_on_side(stack, card);

                if !success {
                    // put card back
                    self.players[idx].personal.push(card);
                    return PlayResult::InvalidIndex;
                }

                self.refill_hand_if_empty(idx);

                PlayResult::Moved
            }
        }
    }

    fn play_side_to_scale(&mut self, stack: usize) -> PlayResult {
        let idx = self.current_idx();

        if let Some(err) = self.ensure_turn(idx) {
            return err;
        }

        let Some(card) = self.players[idx].take_from_side(stack) else {
            return PlayResult::InvalidIndex;
        };

        let result = self.place_on_scale(card);

        if matches!(result, PlayResult::DoesNotFit) {
            self.players[idx].hand.push(card);
        }

        self.refill_hand_if_empty(idx);

        result
    }

    // ── Helper play ───────────────────────────────────────────────────────────

    fn refill_hand_if_empty(&mut self, player_idx: usize) {
        if self.players[player_idx].hand.is_empty()
            && !self.players[player_idx].personal.is_empty()
            && self.phase == GamePhase::Playing
        {
            for _ in 0..5 {
                let Some(card) = self.draw_and_check() else { break };
                self.players[player_idx].hand.push(card);
            }
        }
    }

    fn draw_and_check(&mut self) -> Option<Card> {
        if self.draw_pile.remaining() == 5 && !self.discard_pile.is_empty() {
            let cards = self.discard_pile.drain(..).collect();
            self.draw_pile.add_bottom(cards);
        }

        let card = self.draw_pile.draw_one()?;
        Some(card)
    }

    fn ensure_turn(&self, player_idx: usize) -> Option<PlayResult> {
        if self.phase != GamePhase::Playing {
            return Some(PlayResult::NotAllowed);
        }

        if player_idx != self.current_turn {
            return Some(PlayResult::NotYourTurn);
        }

        None
    }

    // ── End-turn withdrawal ───────────────────────────────────────────────────

    fn withdraw_hand_to_side(&mut self, hand_index: usize, stack: usize) -> PlayResult {
        let idx = self.current_idx();
        let Some(card) = self.players[idx].take_from_hand(hand_index) else {
            return PlayResult::InvalidIndex;
        };
        if self.players[idx].hand.is_empty() && self.players[idx].personal.is_empty() {
            self.phase = GamePhase::Finished;
            return PlayResult::Won;
        }
        self.players[idx].place_on_side(stack, card);

        if self.check_win_condition(idx) {
            return PlayResult::Won;
        }

        self.advance_turn();
        PlayResult::TurnEnded
    }

    // ── Win condition ─────────────────────────────────────────────────────────

    fn check_win_condition(&mut self, player_idx: usize) -> bool {
        if self.players[player_idx].has_won() {
            self.phase = GamePhase::Finished;
            return true;
        }
        false
    }

    pub fn winner(&self) -> Option<&PlayerBoard> {
        self.players.iter().find(|p| p.has_won())
    }

    pub fn apply_move(&mut self, player_id: &str, action: Action) -> PlayResult {
        let Some(idx) = self.players.iter().position(|p| p.player_id == player_id) else {
            return PlayResult::NotAllowed;
        };

        // turn validation
        if self.phase != GamePhase::Playing {
            return PlayResult::NotAllowed;
        }

        if idx != self.current_idx() {
            return PlayResult::NotYourTurn;
        }

        match action {
            Action::PlayHand { index } => self.play_hand_to_scale(index),

            Action::PlayPersonal => self.play_personal_to_scale(),

            Action::PlaySide { stack } => self.play_side_to_scale(stack),

            Action::MoveToSide { hand_index, stack } => {
                self.withdraw_hand_to_side(hand_index, stack)
            }

            Action::Draw => {
                if self.turn_phase != TurnPhase::Draw {
                    return PlayResult::NotAllowed;
                }

                let cards = self.draw_for_current();

                if cards.is_empty() {
                    return PlayResult::NotAllowed;
                }

                self.turn_phase = TurnPhase::Play;

                PlayResult::Moved
            }
        }
    }
}
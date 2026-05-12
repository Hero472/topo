use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::{Arc, atomic::AtomicBool};
use std::collections::HashMap;
use std::time::Duration;
use rand::RngExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    core::game::{
        actions::{Action, PlayResult},
        state::GameState,
    },
    infrastructure::{
        full_state::build_full_state,
        message::GameMessage,
        server_event::ServerEvent,
    },
};

// ── Shared types ──────────────────────────────────────────────────────────────

pub type SharedRoom    = Arc<Mutex<GameState>>;
pub type RoomRegistry  = Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>;

// ── Room handle ───────────────────────────────────────────────────────────────

pub struct RoomHandle {
    /// The actual game state (cards, scales, turn, etc.)
    pub state: SharedRoom,

    /// Per‑player message senders – used to push events to connected WebSocket tasks.
    player_senders: Mutex<HashMap<usize, mpsc::UnboundedSender<GameMessage>>>,

    /// player_id - username
    pub players: Mutex<HashMap<usize, String>>,

    /// True after the game has been started (second player joined).
    game_started: AtomicBool,

    /// Seconds before a player’s turn times out automatically.
    turn_seconds: u64,

    /// Handle to the currently running turn‑timeout task (if any).
    cancel_timer: Mutex<Option<CancellationToken>>,
}

impl RoomHandle {
    pub fn new_arc(room_id: String, turn_seconds: u64, player_ids: Vec<usize>) -> Arc<Self> {
        let mut rng = rand::rng();
        let seed = rng.random::<u64>();
        Arc::new(Self::new(room_id, turn_seconds, player_ids, seed))
    }

    pub fn new_arc_with_seed(
        room_id: String,
        turn_seconds: u64,
        player_ids: Vec<usize>,
        seed: u64,
    ) -> Arc<Self> {
        Arc::new(Self::new(room_id, turn_seconds, player_ids, seed))
    }

    pub fn new(
        room_id: String,
        turn_seconds: u64,
        player_ids: Vec<usize>,
        seed: u64,
    ) -> Self {
        let state = GameState::new(room_id, player_ids, seed, 13, 5);
        Self {
            state: Arc::new(Mutex::new(state)),
            player_senders: Mutex::new(HashMap::new()),
            players: Mutex::new(HashMap::new()),
            game_started: AtomicBool::new(false),
            turn_seconds,
            cancel_timer: Mutex::new(None),
        }
    }

    // ── Messaging infrastructure ─────────────────────────────────────────────

    /// Create a per‑player message channel and return the receiver.
    pub async fn subscribe_player(&self, player_id: usize) -> mpsc::UnboundedReceiver<GameMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.player_senders.lock().unwrap().insert(player_id, tx);
        rx
    }

    /// Push an event to a single player (private).
    fn send_to(&self, player_id: usize, event: ServerEvent) {
        let msg = GameMessage {
            to: Some(player_id),
            event,
        };
        let senders = self.player_senders.lock().unwrap();
        if let Some(tx) = senders.get(&player_id) {
            let _ = tx.send(msg);
        }
    }

    /// Push an event to all connected players (public).
    fn broadcast(&self, event: ServerEvent) {
        let msg = GameMessage { to: None, event };
        let senders = self.player_senders.lock().unwrap();
        for tx in senders.values() {
            let _ = tx.send(msg.clone());
        }
    }

    // ── Player management ────────────────────────────────────────────────────

    pub async fn add_player(self: &Arc<Self>, player_id: usize, username: String) {
        // 1. Register the player’s username
        self.players.lock().unwrap().insert(player_id, username.clone());
        self.broadcast(ServerEvent::PlayerJoined { player_id, username });

        // 2. If this is the second player, start the game once
        let should_start = {
            let players = self.players.lock().unwrap();
            players.len() == 2 && !self.game_started.swap(true, Ordering::SeqCst)
        };

        if should_start {
            self.start_game().await;
        }
    }

    /// Called exactly once when the second player joins.
    async fn start_game(self: &Arc<Self>) {
        let mut state = self.state.lock().unwrap();
        state.start_game();

        // Randomly pick who plays first
        let starter_idx = rand::rng().random_range(0..2);
        state.current_turn = starter_idx;
        let starter_id = state.players[starter_idx].player_idx;

        // Get usernames for FullState generation
        let usernames = self.players.lock().unwrap().clone();

        // Build private FullState for each player
        for p in &state.players {
            if let Some(opp) = state.players.iter().find(|o| o.player_idx != p.player_idx) {
                let opp_name = usernames.get(&opp.player_idx).cloned().unwrap_or_default();
                if let Some(full_state) = build_full_state(&state, p.player_idx, opp_name) {
                    self.send_to(p.player_idx, full_state);
                }
            }
        }

        drop(state); // early release

        println!("[ROOM] Game started. Starter: player {}", starter_id);

        // Public game‑started announcement
        self.broadcast(ServerEvent::GameStarted {
            current_player_id: starter_id,
            turn_seconds: self.turn_seconds,
        });

        // Kick off the turn timer for the first player
        self.set_turn_timer(starter_id).await;
    }

    /// Removes a player from the room.
    /// Returns `true` if the room is now **empty** (no players left).
    pub async fn remove_player(&self, player_id: usize) -> bool {
        // Cancel any running turn timer
        self.cancel_timer();

        // Remove the player from both maps, collect remaining player IDs
        let remaining_ids = {
            let mut players = self.players.lock().unwrap();
            players.remove(&player_id);
            players.keys().cloned().collect::<Vec<usize>>()
        };
        self.player_senders.lock().unwrap().remove(&player_id);

        // If exactly one player remains, they win by forfeit
        if remaining_ids.len() == 1 {
            let winner_id = remaining_ids[0];
            self.broadcast(ServerEvent::GameOver {
                winner_id,
                reason: "Opponent disconnected".into(),
            });
        }

        // Return true if the room is now completely empty
        remaining_ids.is_empty()
    }

    // ── Action handling ───────────────────────────────────────────

    pub fn apply_action(
        self: &Arc<Self>,
        player_id: usize,
        action: Action,
        is_timeout: bool
    ) {
        println!("[ROOM] Apply action {:?} from player {}", action, player_id);

        // 1. Apply the move and collect resulting events
        let (events, result, next_player_id) = {
            let mut state = self.state.lock().unwrap();
            let result = state.apply_move(player_id, action.clone());
            let events = self.generate_events(&state, &action, &result, player_id, is_timeout);
            let next = result.next_player(&state);
            drop(state);
            (events, result, next)
        };

        // 2. Dispatch events with correct visibility
        for event in &events {
            match event {
                ServerEvent::CardDrawn { .. } => {
                    self.send_to(player_id, event.clone());
                }
                ServerEvent::OpponentUpdate { .. } => {
                    if let Some(other) = self.other_player_id(player_id) {
                        self.send_to(other, event.clone());
                    }
                }
                _ => {
                    self.broadcast(event.clone());
                }
            }
        }

        // 3. If the turn ended, cancel the timer and start a new one for the next player
        if result.turn_ended() {
            println!("[ROOM] Turn ended. Next: {:?}", next_player_id);
            self.cancel_timer();
            if let Some(next) = next_player_id {
                self.set_turn_timer(next);
            }
        }

        // If the game ended, the timer is already cancelled (turn_ended includes GameWon)
    }

    // ── Timer management ─────────────────────────────────────────────────────

    /// Cancel any active turn timer.
    fn cancel_timer(&self) {
        let mut guard = self.cancel_timer.lock().unwrap();
        if let Some(token) = guard.take() {
            println!("[TIMER] Cancelling existing timer.");
            token.cancel(); // signals the loop to exit
        }
    }

    /// Cancel any previous timer and start a new one‑shot timeout for `player_id`.
    async fn set_turn_timer(self: &Arc<Self>, player_id: usize) {
        println!("[TIMER] Setting timer for player {}", player_id);
        // Cancel any existing timer
        self.cancel_timer();

        let token = CancellationToken::new();
        let cancel = token.clone();
        {
            let mut guard = self.cancel_timer.lock().unwrap();
            *guard = Some(cancel);
        }

        let room = self.clone();
        let turn_secs = self.turn_seconds;

        tokio::spawn(async move {
            println!("[TIMER] Timer loop started for player {}", player_id);
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(turn_secs)) => {
                        println!("[TIMER] Timeout reached for player {}", player_id);
                        // Timeout – advance the turn

                        // send TimeoutPacket
                        let next_id = room.other_player_id(player_id).unwrap();
                        room.broadcast(ServerEvent::TurnEnded {
                            next_player_id: next_id,
                            turn_seconds: room.turn_seconds,
                            timed_out_player_id: Some(player_id),
                        });

                        let random_stack = rand::rng().random_range(0..4);
                        println!("[TIMER] Auto-playing MoveToSide({}) for player {}", random_stack, player_id);

                        room.apply_action(player_id, Action::MoveToSide { stack: random_stack, hand_idx: 0 }, true);
                    }
                    _ = token.cancelled() => {
                        // Turn was ended manually, loop exits
                        println!("[TIMER] Timer cancelled for player {}", player_id);
                        break;
                    }   
                }
            }
            println!("[TIMER] Timer loop exiting for player {}", player_id);
        });
    }

    // ── Event generation ─────────────────────────────────────────────────────

    fn generate_events(
        &self,
        state: &GameState,
        action: &Action,
        result: &PlayResult,
        player_id: usize,
        is_timeout: bool
    ) -> Vec<ServerEvent> {
        use Action::*;
        let mut events = Vec::new();

        match action {
            Draw => {
                let card = state.player(player_id)
                    .and_then(|p| p.hand.last().cloned());
                events.push(ServerEvent::CardDrawn {
                    player_id,
                    card,
                });
            }

            OpenScale { .. } => {
                if let PlayResult::ScaleOpened { scale_id } = result {
                    if let Some(card) = state.scale(scale_id).cards.last().cloned() {
                        events.push(ServerEvent::CardPlayedOnScale {
                            player_id,
                            card,
                            scale_id: *scale_id,
                            completed: false,
                        });
                    }
                    events.push(self.opponent_update(state, player_id));
                }
            }

            PlayHand { .. } | PlayPersonal { .. } | PlaySide { .. }=> {
                if let PlayResult::ScalePlaced { scale_id, completed } = result {
                    if let Some(card) = state.scale(scale_id).cards.last().cloned() {
                        events.push(ServerEvent::CardPlayedOnScale {
                            player_id,
                            card,
                            scale_id: *scale_id,
                            completed: *completed,
                        });
                    }
                    events.push(self.opponent_update(state, player_id));
                }
            }

            MoveToSide { stack, .. } => {
                if result.is_turn_ended() {
                    if let Some(card) = state.player(player_id)
                        .and_then(|p| p.side.get(*stack).and_then(|s| s.last().cloned()))
                    {
                        events.push(ServerEvent::CardPlacedOnSide {
                            player_id,
                            card,
                            stack: *stack,
                        });
                    }
                    events.push(self.opponent_update(state, player_id));
                    // Turn end announcement
                    if !is_timeout {
                        if let Some(next) = result.next_player(state) {
                            events.push(ServerEvent::TurnEnded {
                                next_player_id: next,
                                turn_seconds: self.turn_seconds,
                                timed_out_player_id: None,   // normal move
                            });
                        }
                    }
                }
            }

            MovePersonalToSide { stack_idx } => {
                if result.is_success() {
                    if let Some(card) = state.player(player_id)
                        .and_then(|p| p.side.get(*stack_idx).and_then(|s| s.last().cloned()))
                    {
                        events.push(ServerEvent::CardPlacedOnSide {
                            player_id,
                            card,
                            stack: *stack_idx,
                        });
                    }
                    events.push(self.opponent_update(state, player_id));
                }
            }
        }

        if let PlayResult::GameWon { winner_id } = result {
            events.push(ServerEvent::GameOver {
                winner_id: *winner_id,
                reason: "All cards cleared".into(),
            });
        }

        events
    }

    fn opponent_update(&self, state: &GameState, player_id: usize) -> ServerEvent {
        let opponent = state.players.iter().find(|p| p.player_idx != player_id);
        match opponent {
            Some(o) => ServerEvent::OpponentUpdate {
                player_idx: o.player_idx,
                personal_count: o.personal.len(),
                personal_top: o.personal_top().cloned(),
                side: o.side.clone(),
            },
            None => ServerEvent::OpponentUpdate {
                player_idx: 0,
                personal_count: 0,
                personal_top: None,
                side: [
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    ],
            },
        }
    }

    fn other_player_id(&self, current: usize) -> Option<usize> {
        let players = self.players.lock().unwrap();
        players.keys().copied().find(|&id| id != current)
    }
}
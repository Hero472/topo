use std::sync::Weak;
use std::sync::atomic::Ordering;
use std::sync::{Arc, atomic::AtomicBool};
use std::collections::HashMap;
use std::time::Duration;
use rand::RngExt;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::{core::game::{actions::{Action, PlayResult}, state::GameState}, infrastructure::{full_state::build_full_state, message::GameMessage, server_event::ServerEvent}};

// ── Shared types ──────────────────────────────────────────────────────────────

pub type SharedRoom    = Arc<Mutex<GameState>>;
pub type RoomRegistry  = Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>;

// ── Room handle ───────────────────────────────────────────────────────────────

pub struct RoomHandle {
    pub state: SharedRoom,
    player_senders: Mutex<HashMap<usize, mpsc::UnboundedSender<GameMessage>>>,
    pub players:    Mutex<HashMap<usize, String>>,
    game_started:   AtomicBool,

    turn_seconds:   u64,
    timeout_handle: Mutex<Option<JoinHandle<()>>>,
}

impl RoomHandle {
    pub fn new_arc(room_id: String, turn_seconds: u64, player_ids: Vec<usize>) -> Arc<Self> {
        let mut rng = rand::rng();
        let seed = rng.random::<u64>();
        Arc::new(Self::new(room_id, turn_seconds, player_ids, seed))
    }

    /// Same as `new_arc` but with a fixed seed (useful for tests).
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
            timeout_handle: Mutex::new(None),
        }
    }

    /// Returns a **per‑player receiver** that the WebSocket task should `await` on.
    /// This replaces the old broadcast & subscription model.
    pub async fn subscribe_player(&self, player_id: usize) -> mpsc::UnboundedReceiver<GameMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.player_senders.lock().await.insert(player_id, tx);
        rx
    }

    /// Send a message to a specific player (if connected).
    async fn send_to_player(&self, player_id: usize, msg: GameMessage) {
        let senders = self.player_senders.lock().await;
        if let Some(tx) = senders.get(&player_id) {
            // Unbounded send never fails; we ignore error if receiver dropped.
            let _ = tx.send(msg);
        }
    }

    /// Send a message to **all** connected players (for `to: None` events).
    async fn broadcast_public(&self, event: ServerEvent) {
        let msg = GameMessage { to: None, event };
        let senders = self.player_senders.lock().await;
        for tx in senders.values() {
            let _ = tx.send(msg.clone());
        }
    }

    /// Add a player to the room. Call when WebSocket connects.
    pub async fn add_player(self: &Arc<Self>, player_id: usize, username: String) {
        {
            let mut players = self.players.lock().await;
            players.insert(player_id, username.clone());
        }

        self.broadcast_public(ServerEvent::PlayerJoined { player_id, username }).await;

        {
            let players = self.players.lock().await;
            if players.len() == 2 && !self.game_started.swap(true, Ordering::SeqCst) {
                drop(players);

                let mut state = self.state.lock().await;
                state.start_game();

                let starter_idx = rand::rng().random_range(0..2);
                state.current_turn = starter_idx;
                let starter_id = state.players[starter_idx].player_idx;

                let usernames: HashMap<usize, String> = {
                    let pmap = self.players.lock().await;
                    pmap.clone()
                };

                let events: Vec<GameMessage> = state
                    .players
                    .iter()
                    .filter_map(|p| {
                        let opp = state.players.iter().find(|o| o.player_idx != p.player_idx)?;
                        let opp_username = usernames.get(&opp.player_idx).cloned().unwrap_or_default();
                        let full_state = build_full_state(&state, p.player_idx, opp_username)?;
                        Some(GameMessage { to: Some(p.player_idx), event: full_state })
                    })
                    .collect();

                drop(state);

                self.broadcast_public(ServerEvent::GameStarted {
                    current_player_id: starter_id,
                    turn_seconds: self.turn_seconds,
                }).await;

                for gm in events {
                    if let Some(to) = gm.to {
                        self.send_to_player(to, gm).await;
                    }
                }

                // ⏱️ Start the turn timer for the first player
                start_turn_timer(self.clone(), starter_id).await;
            }
        }
    }

    /// Remove a player (on disconnect).
    pub async fn remove_player(&self, player_id: usize) {
        self.cancel_timer().await;

        self.players.lock().await.remove(&player_id);
        self.player_senders.lock().await.remove(&player_id);

        self.broadcast_public(ServerEvent::GameOver {
            winner_id: 0,
            reason: "Opponent disconnected".into(),
        }).await;
    }

    // ── Action handling ───────────────────────────────────────────
    pub async fn apply_action(self: &Arc<Self>, player_id: usize, action: Action) {
        let mut state = self.state.lock().await;
        let result = state.apply_move(player_id, action.clone());
        let events = self.generate_events(&state, &action, &result, player_id);

        // Determine if the turn has ended (or game won)
        let turn_ended = matches!(&result, PlayResult::TurnEnded | PlayResult::GameWon { .. });
        let next_player_id = if let PlayResult::TurnEnded = &result {
            state.players.get(state.current_turn).map(|p| p.player_idx)
        } else {
            None
        };
        drop(state);

        // Dispatch all events
        for event in &events {
            match event {
                ServerEvent::CardDrawn { .. } => {
                    self.send_to_player(player_id, GameMessage {
                        to: Some(player_id),
                        event: event.clone(),
                    }).await;
                }
                ServerEvent::OpponentUpdate { .. } => {
                    if let Some(other) = self.other_player_id(player_id).await {
                        self.send_to_player(other, GameMessage {
                            to: Some(other),
                            event: event.clone(),
                        }).await;
                    }
                }
                _ => {
                    self.broadcast_public(event.clone()).await;
                }
            }
        }

        // Handle turn transition / timer
        if turn_ended {
            self.cancel_timer().await;
            if let Some(next_id) = next_player_id {
                start_turn_timer(self.clone(), next_id).await;
            }
        }
    }

    // keep cancel_timer (as &self), generate_events, opponent_update, other_player_id unchanged
    async fn cancel_timer(&self) {
        if let Some(handle) = self.timeout_handle.lock().await.take() {
            handle.abort();
        }
    }

    /// Build the list of server events from the action, result, and final state.
    /// Now panic‑free: all `unwrap()` calls replaced with safe alternatives.
    fn generate_events(
        &self,
        state: &GameState,
        action: &Action,
        result: &PlayResult,
        player_id: usize,
    ) -> Vec<ServerEvent> {
        use Action::*;
        let mut events = Vec::new();

        match action {
            Draw => {
                if matches!(result, PlayResult::Success) {
                    if let Some(card) = state.players.iter()
                        .find(|p| p.player_idx == player_id)
                        .and_then(|p| p.hand.last().cloned())
                    {
                        events.push(ServerEvent::CardDrawn { player_id, card: Some(card) });
                    } else {
                        // Unexpected empty hand after draw – skip the event or send CardDrawn{card:None}
                        events.push(ServerEvent::CardDrawn { player_id, card: None });
                    }
                }
            }
            OpenScale { .. } => {
                if let PlayResult::ScaleOpened { scale_id } = result {
                    if let Some(card) = state.scale_manager.scales.get(*scale_id)
                        .and_then(|s| s.cards.last().cloned())
                    {
                        events.push(ServerEvent::CardPlayedOnScale {
                            player_id,
                            card,
                            scale_id: *scale_id,
                            completed: false,
                        });
                    } else {
                        eprintln!("Scale {} not found after OpenScale", scale_id);
                    }
                    events.push(self.opponent_update(state, player_id));
                }
            }
            PlayHand { scale_idx, .. }
            | PlayPersonal { scale_idx }
            | PlaySide { scale_idx, .. } => {
                if let PlayResult::ScalePlaced { scale_id, completed } = result {
                    if let Some(card) = state.scale_manager.scales.get(*scale_id)
                        .and_then(|s| s.cards.last().cloned())
                    {
                        events.push(ServerEvent::CardPlayedOnScale {
                            player_id,
                            card,
                            scale_id: *scale_id,
                            completed: *completed,
                        });
                    } else {
                        eprintln!("Scale {} missing after placement", scale_id);
                    }
                    events.push(self.opponent_update(state, player_id));
                }
            }
            MoveToSide { stack, .. } => {
                if matches!(result, PlayResult::TurnEnded) {
                    // Find card on side stack; if missing for any reason, omit event.
                    if let Some(card) = state.players.iter()
                        .find(|p| p.player_idx == player_id)
                        .and_then(|p| p.side.get(*stack).and_then(|s| s.last().cloned()))
                    {
                        events.push(ServerEvent::CardPlacedOnSide {
                            player_id,
                            card,
                            stack: *stack,
                        });
                    }
                    events.push(self.opponent_update(state, player_id));
                    // Get next player's ID; if the array is empty (shouldn't happen), skip the event.
                    if let Some(next_player) = state.players.get(state.current_turn) {
                        events.push(ServerEvent::TurnEnded {
                            next_player_id: next_player.player_idx,
                            turn_seconds: 60,
                        });
                    }
                }
            }
            MovePersonalToSide { stack_idx } => {
                if matches!(result, PlayResult::Success) {
                    if let Some(card) = state.players.iter()
                        .find(|p| p.player_idx == player_id)
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

        // Check for game over
        if let PlayResult::GameWon { winner_id } = result {
            events.push(ServerEvent::GameOver {
                winner_id: *winner_id,
                reason: "All cards cleared".into(),
            });
        }

        events
    }

    /// Create OpponentUpdate for the opponent of `player_id`.
    /// Now never panics – if the opponent is not found, returns a dummy event (or we can skip later).
    fn opponent_update(&self, state: &GameState, player_id: usize) -> ServerEvent {
        if let Some(opponent) = state.players.iter().find(|p| p.player_idx != player_id) {
            ServerEvent::OpponentUpdate {
                player_id: opponent.player_idx,
                personal_count: opponent.personal.len(),
                personal_top: opponent.personal_top().cloned(),
                side: opponent.side.clone(),
            }
        } else {
            // In case of corrupted state, return a placeholder. The caller will handle it later.
            ServerEvent::OpponentUpdate {
                player_id: 0,
                personal_count: 0,
                personal_top: None,
                side: [
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                ],
            }
        }
    }

    async fn other_player_id(&self, current: usize) -> Option<usize> {
        let players = self.players.lock().await;
        players.keys().copied().find(|&id| id != current)
    }
}

async fn start_turn_timer(room: Arc<RoomHandle>, player_id: usize) {
    // Cancel any running timer first
    room.cancel_timer().await;

    let turn_secs = room.turn_seconds;

    // Clone the Arc so that we can move it into the spawned task,
    // leaving `room` available for the rest of this function.
    let room_clone = room.clone();

    let handle = tokio::spawn(async move {
        let mut current_player = player_id;

        loop {
            tokio::time::sleep(Duration::from_secs(turn_secs)).await;

            let mut state = room_clone.state.lock().await;

            // If the room is empty, stop the timer loop.
            if state.players.is_empty() {
                return;
            }

            let next_idx = (state.current_turn + 1) % state.players.len();
            state.current_turn = next_idx;
            let next_player_id = state.players[next_idx].player_idx;
            drop(state);

            room_clone
                .broadcast_public(ServerEvent::TurnTimedOut {
                    player_id: current_player,
                    next_player_id,
                    turn_seconds: room_clone.turn_seconds,
                })
                .await;

            room_clone
                .broadcast_public(ServerEvent::TurnEnded {
                    next_player_id,
                    turn_seconds: room_clone.turn_seconds,
                })
                .await;

            current_player = next_player_id;
        }
    });

    // `room` is still valid here because we moved `room_clone`, not `room`.
    room.timeout_handle.lock().await.replace(handle);
}
use std::sync::Arc;
use std::collections::HashMap;
use rand::RngExt;
use tokio::sync::{Mutex, broadcast};

use crate::{core::game::{actions::{Action, PlayResult}, state::GameState}, infrastructure::{message::GameMessage, server_event::ServerEvent}};

// ── Shared types ──────────────────────────────────────────────────────────────

pub type SharedRoom    = Arc<Mutex<GameState>>;
pub type RoomRegistry  = Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>;

// ── Room handle ───────────────────────────────────────────────────────────────

pub struct RoomHandle {
    pub state: SharedRoom,
    tx:        broadcast::Sender<GameMessage>,
    // player_id -> username (for events that include a username)
    players:   Mutex<HashMap<usize, String>>,
}

impl RoomHandle {
    pub fn new(room_id: String, turn_seconds: u64, player_ids: Vec<usize>) -> Self {
        let mut rng  = rand::rng();
        Self::new_with_seed(room_id, turn_seconds, player_ids, rng.random::<u64>())
    }

    pub fn new_with_seed(
        room_id: String,
        _turn_seconds: u64,
        player_ids: Vec<usize>,
        seed: u64,
    ) -> Self {
        let state = GameState::new(room_id, player_ids, seed, 13, 5);
        let (tx, _) = broadcast::channel(32);
        Self {
            state: Arc::new(Mutex::new(state)),
            tx,
            players: Mutex::new(HashMap::new()),
        }
    }

    /// Subscribe to all messages (public + private). The client filters by `to`.
    pub fn subscribe(&self) -> broadcast::Receiver<GameMessage> {
        self.tx.subscribe()
    }

    /// Add a player to the room. Call when WebSocket connects.
    pub async fn add_player(&self, player_id: usize, username: String) {
        self.players.lock().await.insert(player_id, username.clone());
        // Broadcast public join event
        let _ = self.tx.send(GameMessage {
            to: None,
            event: ServerEvent::PlayerJoined { player_id, username },
        });
        // If both players are present, you could start the game etc.
    }

    /// Remove a player (on disconnect).
    pub async fn remove_player(&self, player_id: usize) {
        self.players.lock().await.remove(&player_id);
        let _ = self.tx.send(GameMessage {
            to: None,
            event: ServerEvent::PlayerLeft { player_id },
        });
    }

    pub async fn apply_action(&self, player_id: usize, action: Action) {
        // 1. Lock game state and apply the move
        let mut state = self.state.lock().await;
        let result = state.apply_move(player_id, action.clone());
        let events = self.generate_events(&state, &action, &result, player_id);
        drop(state);

        for event in events {
            let to = match &event {
                ServerEvent::CardDrawn { .. } => Some(player_id),
                ServerEvent::OpponentUpdate { player_id: pid, .. } => {
                    let other = self.other_player_id(player_id).await;
                    Some(other.unwrap())
                },
                _ => None,   // public
            };
            let _ = self.tx.send(GameMessage { to, event });
        }
    }

    /// Build the list of server events from the action, result, and final state.
    fn generate_events(
        &self,
        state: &GameState,
        action: &Action,
        result: &PlayResult,
        player_id: usize,
    ) -> Vec<ServerEvent> {
        use crate::core::game::actions::Action;
        let mut events = Vec::new();

        match action {
            Action::Draw => {
                // After a successful draw, the player's hand has one extra card (last added).
                if matches!(result, PlayResult::Success) {
                    let card: Option<crate::core::game::card::Card> = state.players.iter()
                        .find(|p| p.player_idx == player_id)
                        .and_then(|p| p.hand.last().cloned());
                    events.push(ServerEvent::CardDrawn { player_id, card });
                }
            }

            Action::OpenScale { hand_idx: _ } => {
                if let PlayResult::ScaleOpened { scale_id } = result {
                    // The card that was used is the ace on the new scale's top.
                    let card = state.scale_manager.scales[*scale_id].cards.last().cloned().unwrap();
                    events.push(ServerEvent::CardPlayedOnScale {
                        player_id,
                        card,
                        scale_id: *scale_id,
                        completed: false,
                    });
                    // Opponent needs updated hand count / side (if any). Here the hand size changed.
                    events.push(self.opponent_update(state, player_id));
                }
            }

            Action::PlayHand { hand_idx: _, scale_idx } |
            Action::PlayPersonal { scale_idx } |
            Action::PlaySide { stack: _, scale_idx } => {
                if let PlayResult::ScalePlaced { scale_id, completed } = result {
                    let card = state.scale_manager.scales[*scale_id].cards.last().cloned().unwrap();
                    events.push(ServerEvent::CardPlayedOnScale {
                        player_id,
                        card,
                        scale_id: *scale_id,
                        completed: *completed,
                    });
                    // If the scale completed, the cards are drained and discarded – still visible.
                    // Opponent sees personal count / side changes.
                    events.push(self.opponent_update(state, player_id));
                }
            }

            Action::MoveToSide { hand_idx: _, stack } => {
                if matches!(result, PlayResult::TurnEnded) {
                    // The card now on the side stack is the last one placed.
                    let card = state.players.iter()
                        .find(|p| p.player_idx == player_id)
                        .and_then(|p| p.side[*stack].last().cloned())
                        .unwrap();
                    events.push(ServerEvent::CardPlacedOnSide {
                        player_id,
                        card,
                        stack: *stack,
                    });
                    events.push(self.opponent_update(state, player_id));
                    events.push(ServerEvent::TurnEnded {
                        next_player_id: state.players[state.current_turn].player_idx,
                        turn_seconds: 60, // or from config
                    });
                }
            }

            Action::MovePersonalToSide { stack_idx } => {
                if matches!(result, PlayResult::Success) {
                    let card = state.players.iter()
                        .find(|p| p.player_idx == player_id)
                        .and_then(|p| p.side[*stack_idx].last().cloned())
                        .unwrap();
                    events.push(ServerEvent::CardPlacedOnSide {
                        player_id,
                        card,
                        stack: *stack_idx,
                    });
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
    fn opponent_update(&self, state: &GameState, player_id: usize) -> ServerEvent {
        let opponent = state.players.iter()
            .find(|p| p.player_idx != player_id)
            .unwrap();
        ServerEvent::OpponentUpdate {
            player_id: opponent.player_idx,
            personal_count: opponent.personal.len(),
            personal_top: opponent.personal_top().cloned(),
            side: opponent.side.clone(),
        }
    }

    async fn other_player_id(&self, current: usize) -> Option<usize> {
        let players = self.players.lock().await;
        players.keys().find(|&&id| id != current).copied()
    }
}
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tokio::task::JoinHandle;

use async_trait::async_trait;
use log::{debug, warn};
use rand::RngExt;

use crate::{
    core::{
        game::state::{state_types::Seed, GameState, Seconds},
        game_id::GameId,
        player::{PlayerId, PlayerIdx},
    },
    infrastructure::{
        error::ErrorCode,
        full_state::build_full_state,
        room::utils::{broadcast, send_to, start_timer},
        server_event::ServerEvent,
    },
};

use super::*;

const BOARD_SIZE: usize = 13;
const HAND_SIZE: usize = 5;
const DISCONNECT_GRACE_SECONDS: u64 = 30;

pub struct OverPhase {
    pub game_id: GameId,
    pub turn_seconds: Seconds,

    pub winner_id: PlayerId,
    pub winner_idx: PlayerIdx,
    pub reason: String,

    /// Players that participated in the finished game.
    /// This survives disconnections.
    pub participants: HashMap<PlayerId, PlayerIdx>,

    /// Players that have requested a rematch.
    pub play_again: HashSet<PlayerId>,

    /// Tracks active grace period timers for disconnected players.
    pub pending_disconnects: HashMap<PlayerId, JoinHandle<()>>,
}

impl OverPhase {
    pub fn new(
        game_id: GameId,
        players: &HashMap<PlayerId, PlayerInfo>,
        participants: HashMap<PlayerId, PlayerIdx>,
        turn_seconds: Seconds,
        winner_id: PlayerId,
        winner_idx: PlayerIdx,
        reason: String,
    ) -> Self {
        // Announce the result to currently connected players.
        for player_id in players.keys() {
            send_to(
                players,
                *player_id,
                ServerEvent::GameOver {
                    winner_id,
                    winner_idx,
                    reason: reason.clone(),
                },
            );
        }

        Self {
            game_id,
            turn_seconds,
            winner_id,
            winner_idx,
            reason,
            participants,
            play_again: HashSet::new(),
            pending_disconnects: HashMap::new(),
        }
    }

    /// Builds a fresh GameState + player mappings from `self.participants` and
    /// returns the new PlayingPhase to transition into.
    fn start_rematch(
        &mut self,
        players: &mut HashMap<PlayerId, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Box<dyn RoomPhase + Send> {
        let seed = Seed(rand::rng().random::<u64>());

        let mut fresh_state = GameState::new(
            self.game_id.clone(),
            seed,
            BOARD_SIZE,
            HAND_SIZE,
            self.turn_seconds,
        );

        fresh_state.start_game();

        // Restore the PlayerId -> PlayerIdx relationship into the new GameState.
        for (&player_id, &player_idx) in &self.participants {
            if let Some(board) = fresh_state
                .players
                .iter_mut()
                .find(|board| board.player_idx == player_idx)
            {
                board.player_id = Some(player_id);
            }
        }

        let starter_idx = fresh_state.current_turn;

        let starter_id = self
            .participants
            .iter()
            .find(|(_, player_idx)| **player_idx == starter_idx)
            .map(|(&player_id, _)| player_id)
            .expect("Starter must exist in participants");

        *state = Some(fresh_state);

        let game_state = state.as_ref().expect("fresh state was just inserted");

        let id_to_idx = self.participants.clone();

        let idx_to_id: HashMap<PlayerIdx, PlayerId> = self
            .participants
            .iter()
            .map(|(&player_id, &player_idx)| (player_idx, player_id))
            .collect();

        self.play_again.clear();
        self.pending_disconnects.clear();

        // Send full state to all connected players
        for &player_id in players.keys() {
            if let Some(idx) = self.participants.get(&player_id) {
                if let Some(event) = build_full_state(game_state, *idx, String::from("Opponent")) {
                    send_to(players, player_id, event);
                }
            }
        }

        broadcast(
            players,
            &ServerEvent::GameStarted {
                current_player_id: starter_id,
                current_player_idx: starter_idx,
                turn_seconds: self.turn_seconds,
            },
        );

        start_timer(starter_id, self.turn_seconds, timer, cmd_tx);

        Box::new(PlayingPhase {
            game_id: self.game_id.clone(),
            turn_seconds: self.turn_seconds,
            pending_disconnects: HashMap::new(), // ✅ Updated to match new PlayingPhase
            current_player: starter_idx,
            id_to_idx,
            idx_to_id,
            turn_started_at: Instant::now(),
        })
    }
}

#[async_trait]
impl RoomPhase for OverPhase {
    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<PlayerId, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase + Send>> {
        debug!("HANDLE_OVER_CMD: {:?}", cmd);

        match cmd {
            RoomCommand::SubscribePlayer { player_id, sender } => {
                let player_idx = match self.participants.get(&player_id) {
                    Some(&idx) => idx,
                    None => return None,
                };

                players.insert(
                    player_id,
                    PlayerInfo {
                        username: String::new(),
                        tx: sender,
                        player_idx,
                        connected: true,
                    },
                );

                if let Some(game_state) = state.as_ref() {
                    if let Some(event) = build_full_state(game_state, player_idx, String::from("Opponent")) {
                        send_to(players, player_id, event);
                    }
                }

                send_to(
                    players,
                    player_id,
                    ServerEvent::GameOver {
                        winner_id: self.winner_id,
                        winner_idx: self.winner_idx,
                        reason: self.reason.clone(),
                    },
                );

                broadcast(
                    players,
                    &ServerEvent::PlayerReconnected {
                        player_id,
                        player_idx,
                        turn_seconds_remaining: Seconds::from(0),
                    },
                );
                None
            }

            RoomCommand::UnsubscribePlayer { player_id } => {
                // Only remove if they never successfully joined (player_idx is still MAX)
                // If they did join, we rely on NetworkDisconnect or PlayerLeft.
                if let Some(info) = players.get(&player_id) {
                    if info.player_idx == PlayerIdx(usize::MAX) {
                        players.remove(&player_id);
                    }
                }
                None
            }

            // Network drop (starts grace period)
            RoomCommand::NetworkDisconnect { player_id } => {
                let player_idx = if let Some(info) = players.get_mut(&player_id) {
                    info.connected = false;
                    Some(info.player_idx)
                } else {
                    None
                };

                if let Some(idx) = player_idx {
                    broadcast(
                        players,
                        &ServerEvent::PlayerDisconnected {
                            player_id,
                            player_idx: idx,
                            grace_period_seconds: DISCONNECT_GRACE_SECONDS,
                        },
                    );

                    let tx = cmd_tx.clone();
                    let handle = tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(DISCONNECT_GRACE_SECONDS)).await;
                        let _ = tx.send(RoomCommand::DisconnectTimeout { player_id });
                    });

                    self.pending_disconnects.insert(player_id, handle);
                }
                None
            }

            // ✅ NEW: Reconnect cancels grace period and restores state
            RoomCommand::PlayerReconnected { player_id } => {
                if let Some(handle) = self.pending_disconnects.remove(&player_id) {
                    handle.abort();
                }

                let player_idx = if let Some(info) = players.get_mut(&player_id) {
                    info.connected = true;
                    Some(info.player_idx)
                } else {
                    None
                };

                if let Some(idx) = player_idx {
                    if let Some(game_state) = state.as_ref() {
                        if let Some(event) = build_full_state(game_state, idx, String::from("Opponent")) {
                            send_to(players, player_id, event);
                        }
                    }

                    send_to(
                        players,
                        player_id,
                        ServerEvent::GameOver {
                            winner_id: self.winner_id,
                            winner_idx: self.winner_idx,
                            reason: self.reason.clone(),
                        },
                    );

                    broadcast(
                        players,
                        &ServerEvent::PlayerReconnected {
                            player_id,
                            player_idx: idx,
                            turn_seconds_remaining: Seconds::from(0),
                        },
                    );
                }
                None
            }

            // Intentional leave (bypasses grace period, removes immediately)
            RoomCommand::PlayerLeft { player_id } => {
                if let Some(handle) = self.pending_disconnects.remove(&player_id) {
                    handle.abort();
                }

                let idx = self.participants.get(&player_id).copied().unwrap_or(PlayerIdx(usize::MAX));

                players.remove(&player_id);
                self.play_again.remove(&player_id);
                self.participants.remove(&player_id);

                if players.is_empty() {
                    let _ = cmd_tx.send(RoomCommand::Shutdown);
                    return None;
                }

                broadcast(
                    players,
                    &ServerEvent::PlayerLeft {
                        player_id,
                        player_idx: idx,
                    },
                );
                None
            }

            // ✅ NEW: Grace period expired, actually remove the player
            RoomCommand::DisconnectTimeout { player_id } => {
                self.pending_disconnects.remove(&player_id);

                if let Some(info) = players.get(&player_id) {
                    if info.connected {
                        return None; // Reconnected just in time
                    }
                } else {
                    return None; // Already removed
                }

                let idx = self.participants.get(&player_id).copied().unwrap_or(PlayerIdx(usize::MAX));

                players.remove(&player_id);
                self.play_again.remove(&player_id);
                self.participants.remove(&player_id);

                if players.is_empty() {
                    let _ = cmd_tx.send(RoomCommand::Shutdown);
                    return None;
                }

                broadcast(
                    players,
                    &ServerEvent::PlayerLeft {
                        player_id,
                        player_idx: idx,
                    },
                );
                None
            }

            RoomCommand::PlayAgain { player_id } => {
                // Only original participants may request a rematch.
                if !self.participants.contains_key(&player_id) {
                    return None;
                }

                // Player must currently be connected.
                if !players.contains_key(&player_id) {
                    return None;
                }

                if self.participants.len() < 2 {
                    warn!(
                        "PlayAgain from {:?} but only {} participant(s) remain - ignoring",
                        player_id,
                        self.participants.len()
                    );
                    return None;
                }

                let newly_ready = self.play_again.insert(player_id);

                if newly_ready {
                    broadcast(players, &ServerEvent::PlayAgainRequested { player_id });
                }

                let all_present = self.participants.keys().all(|id| players.contains_key(id));
                let all_agreed = self.participants.keys().all(|id| self.play_again.contains(id));

                // Wait until both players are connected and both have pressed "Play Again".
                if !all_present || !all_agreed {
                    return None;
                }

                return Some(self.start_rematch(players, state, timer, cmd_tx));
            }

            RoomCommand::PlayerAction { player_id, .. } => {
                send_to(
                    players,
                    player_id,
                    ServerEvent::Error {
                        code: ErrorCode::GameOver,
                        message: Some("Game is already over".into()),
                        details: None,
                    },
                );
                None
            }

            RoomCommand::IsPlayerKnown { player_id, reply } => {
                let known = self.participants.contains_key(&player_id);
                let _ = reply.send(known);
                None
            }

            _ => None,
        }
    }
}
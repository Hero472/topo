use async_trait::async_trait;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

use crate::core::game::actions::move_result::MoveResult;
use crate::core::game::actions::{MoveSuccess, MoveError};
use crate::core::game::state::{GameState, Seconds};
use crate::core::game_id::GameId;
use crate::core::player::{PlayerId, PlayerIdx};
use crate::infrastructure::error::{ErrorCode, ErrorDetails};
use crate::infrastructure::room::player_info::PlayerInfo;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::server_event::ServerEvent;
use crate::infrastructure::room::utils::*;
use crate::infrastructure::full_state::build_full_state;

use super::*;

/// How long a disconnected player has to reconnect before they forfeit.
const DISCONNECT_GRACE_SECONDS: u64 = 30;

pub struct PlayingPhase {
    pub game_id: GameId,
    pub turn_seconds: Seconds,
    pub turn_started_at: Instant,
    pub pending_disconnects: HashMap<PlayerId, JoinHandle<()>>, // ✅ Updated to match LobbyPhase
    pub current_player: PlayerIdx,
    pub id_to_idx: HashMap<PlayerId, PlayerIdx>,
    pub idx_to_id: HashMap<PlayerIdx, PlayerId>,
}

impl PlayingPhase {
    pub fn turn_seconds_remaining(&self) -> Seconds {
        let elapsed = Seconds::from(self.turn_started_at.elapsed().as_secs());
        self.turn_seconds.saturating_sub(elapsed)
    }
}

#[async_trait]
impl RoomPhase for PlayingPhase {
    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<PlayerId, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase + Send>> {
        let game_state = state.as_mut().expect("PlayingPhase requires a game state");

        debug!("HANDLE_PLAYING_CMD: {:?}", cmd);

        match cmd {
            RoomCommand::SubscribePlayer { player_id, sender } => {
                if let Some(info) = players.get_mut(&player_id) {
                    info.tx = sender;
                } else {
                    warn!("SubscribePlayer for unknown player {:?}", player_id);
                }
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

            RoomCommand::PlayerAction { player_id, action } => {
                let player_idx = match self.id_to_idx.get(&player_id) {
                    Some(&idx) => idx,
                    None => {
                        warn!("Unknown player {:?} sent action", player_id);
                        return None;
                    }
                };

                if player_idx != self.current_player {
                    send_to(
                        players,
                        player_id,
                        ServerEvent::Error {
                            code: ErrorCode::NotYourTurn,
                            message: Some("It's not your turn".into()),
                            details: Some(ErrorDetails {
                                player_id: Some(player_id),
                                action: Some(format!("{:?}", action)),
                                card_id: None,
                            }),
                        },
                    );
                    return None;
                }

                let MoveResult {
                    success,
                    drawn_cards,
                    discarded_cards: _,
                } = match game_state.apply_move(player_idx, action.clone()) {
                    Ok(result) => result,
                    Err(move_err) => {
                        let code = match move_err {
                            MoveError::DoesNotFit => ErrorCode::InvalidMove,
                            MoveError::NotAllowed => ErrorCode::InvalidMove,
                            MoveError::InvalidIndex { .. } => ErrorCode::CardNotFound,
                            MoveError::NotYourTurn => ErrorCode::NotYourTurn,
                        };
                        warn!(
                            "Invalid move by player {:?} ({:?}): {:?} -> {:?}",
                            player_id, player_idx, action, move_err
                        );
                        send_to(
                            players,
                            player_id,
                            ServerEvent::Error {
                                code,
                                message: None,
                                details: Some(ErrorDetails {
                                    player_id: Some(player_id),
                                    action: Some(format!("{:?}", action)),
                                    card_id: None,
                                }),
                            },
                        );
                        return None;
                    }
                };

                if let Some(token) = timer.take() {
                    token.cancel();
                }

                if let Some(drawn) = drawn_cards {
                    send_to(
                        players,
                        player_id,
                        ServerEvent::HandRefill {
                            player_id,
                            player_idx,
                            cards: drawn,
                            turn_seconds_remaining: self.turn_seconds_remaining(),
                        },
                    );
                }

                process_action(
                    players,
                    game_state,
                    &action,
                    &success,
                    player_id,
                    self.turn_seconds_remaining(),
                );

                if let MoveSuccess::GameWon { winner_idx } = success {
                    let winner_id = self.idx_to_id[&winner_idx];
                    info!(
                        "Game won by player {:?} ({:?}) in room {}",
                        winner_id, winner_idx, self.game_id
                    );

                    let participants = self.id_to_idx.clone();
                    return Some(Box::new(OverPhase::new(
                        self.game_id.clone(),
                        players,
                        participants,
                        self.turn_seconds,
                        winner_id,
                        winner_idx,
                        "All cards cleared".into(),
                    )));
                }

                if success.turn_ended() {
                    let next_idx = game_state.current_turn;
                    let next_id = self.idx_to_id[&next_idx];

                    info!("Turn ended. Next player: {:?} ({:?})", next_id, next_idx);

                    self.current_player = next_idx;
                    self.turn_started_at = Instant::now();

                    broadcast(
                        players,
                        &ServerEvent::TurnEnded {
                            next_player_id: next_id,
                            next_player_idx: next_idx,
                            turn_seconds: self.turn_seconds,
                            timed_out_player_id: None,
                            timed_out_player_idx: None,
                        },
                    );
                    send_full_state(players, game_state);

                    start_timer(next_id, self.turn_seconds, timer, cmd_tx);
                }

                None
            }

            RoomCommand::TurnTimeout { player_id } => {
                if Some(player_id) != self.idx_to_id.get(&self.current_player).copied() {
                    warn!(
                        "TurnTimeout for {:?}, but current player is {:?}",
                        player_id, self.current_player
                    );
                    return None;
                }

                let timed_out_idx = self.current_player;

                if let Some(token) = timer.take() {
                    token.cancel();
                }

                let next_idx = game_state.advance_turn();
                let next_id = self.idx_to_id[&next_idx];
                self.current_player = next_idx;

                broadcast(
                    players,
                    &ServerEvent::TurnEnded {
                        next_player_id: next_id,
                        next_player_idx: next_idx,
                        turn_seconds: self.turn_seconds,
                        timed_out_player_id: Some(player_id),
                        timed_out_player_idx: Some(timed_out_idx),
                    },
                );

                send_full_state(players, game_state);
                start_timer(next_id, self.turn_seconds, timer, cmd_tx);

                None
            }

            // Network drop (starts grace period, pauses turn if needed)
            RoomCommand::NetworkDisconnect { player_id } => {
                let player_idx = if let Some(info) = players.get_mut(&player_id) {
                    info.connected = false;
                    Some(info.player_idx)
                } else {
                    None
                };

                if let Some(idx) = player_idx {
                    if idx == self.current_player {
                        if let Some(token) = timer.take() {
                            token.cancel();
                        }
                    }

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

            // Reconnect cancels grace period and restores state
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
                    if let Some(event) = build_full_state(game_state, idx, String::from("Opponent")) {
                        send_to(players, player_id, event);
                    }

                    broadcast(
                        players,
                        &ServerEvent::PlayerReconnected {
                            player_id,
                            player_idx: idx,
                            turn_seconds_remaining: self.turn_seconds_remaining(),
                        },
                    );

                    if idx == self.current_player {
                        start_timer(player_id, self.turn_seconds, timer, cmd_tx);
                    }
                }
                None
            }

            // Intentional leave (bypasses grace period, ends game immediately)
            RoomCommand::PlayerLeft { player_id } => {
                if let Some(handle) = self.pending_disconnects.remove(&player_id) {
                    handle.abort();
                }

                let info = match players.get(&player_id) {
                    Some(info) => info,
                    None => {
                        warn!("PlayerLeft for unknown player {:?}", player_id);
                        return None;
                    }
                };

                let leaving_idx = info.player_idx;

                if let Some(token) = timer.take() {
                    token.cancel();
                }

                let winner = self
                    .id_to_idx
                    .iter()
                    .find(|(pid, _)| **pid != player_id)
                    .map(|(&pid, &idx)| (pid, idx));

                let Some((winner_id, winner_idx)) = winner else {
                    warn!("PlayerLeft but no opponent exists");
                    players.remove(&player_id);
                    return None;
                };

                let participants = self.id_to_idx.clone();
                players.remove(&player_id);

                broadcast(
                    players,
                    &ServerEvent::PlayerLeft {
                        player_id,
                        player_idx: leaving_idx,
                    },
                );

                Some(Box::new(OverPhase::new(
                    self.game_id.clone(),
                    players,
                    participants,
                    self.turn_seconds,
                    winner_id,
                    winner_idx,
                    "Opponent left the game".into(),
                )))
            }

            // ✅ NEW: Grace period expired, actually remove the player and end game
            RoomCommand::DisconnectTimeout { player_id } => {
                self.pending_disconnects.remove(&player_id);

                let info = match players.get(&player_id) {
                    Some(info) => info,
                    None => return None,
                };

                if info.connected {
                    return None; // Reconnected just in time
                }

                if let Some(token) = timer.take() {
                    token.cancel();
                }

                let winner = self
                    .id_to_idx
                    .iter()
                    .find(|(pid, _)| **pid != player_id)
                    .map(|(&pid, &idx)| (pid, idx));

                if let Some((winner_id, winner_idx)) = winner {
                    let participants = self.id_to_idx.clone();
                    players.remove(&player_id);

                    return Some(Box::new(OverPhase::new(
                        self.game_id.clone(),
                        players,
                        participants,
                        self.turn_seconds,
                        winner_id,
                        winner_idx,
                        "Opponent did not reconnect in time".into(),
                    )));
                }

                None
            }

            RoomCommand::IsPlayerKnown { player_id, reply } => {
                let known = players.contains_key(&player_id);
                let _ = reply.send(known);
                None
            }

            _ => None,
        }
    }
}
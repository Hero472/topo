use async_trait::async_trait;
use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::core::game::actions::move_result::MoveResult;
use crate::core::game::actions::{MoveSuccess, MoveError};
use crate::core::game::state::{GameState, Seconds};
use crate::core::game_id::GameId;
use crate::core::player::PlayerIdx;
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
    pub disconnect_tokens: HashMap<PlayerId, CancellationToken>,
    pub current_player: PlayerIdx,
    pub id_to_idx: HashMap<PlayerId, PlayerIdx>,
    pub idx_to_id: HashMap<PlayerIdx, PlayerId>,
}

impl PlayingPhase {
    pub fn turn_seconds_remaining(&self) -> Seconds {
        let elapsed = Seconds::from(
            self.turn_started_at.elapsed().as_secs()
        );
 
        self.turn_seconds.saturating_sub(elapsed)
    }
 
    /// Spawns the grace-period watchdog for a disconnected player. If it isn't
    /// cancelled (via reconnect) before it elapses, sends DisconnectTimeout.
    fn spawn_disconnect_grace_timer(
        player_id: PlayerId,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> CancellationToken {
        let token = CancellationToken::new();
        let cancel_clone = token.clone();
        let tx = cmd_tx.clone();
 
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_clone.cancelled() => {},
                _ = tokio::time::sleep(Duration::from_secs(DISCONNECT_GRACE_SECONDS)) => {
                    let _ = tx.send(RoomCommand::DisconnectTimeout { player_id });
                }
            }
        });
 
        token
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

        println!("HANDLE_PLAYING_CMD: {:?}", cmd);

        match cmd {

            RoomCommand::SubscribePlayer { player_id, sender } => {
                if let Some(info) = players.get_mut(&player_id) {
                    info.tx = sender;
                } else {
                    warn!("SubscribePlayer for unknown player {:?}", player_id);
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
 
                debug!("Received action from player {:?} ({:?}): {:?}", player_id, player_idx, action);
 
                if player_idx != self.current_player {
                    send_to(players, player_id, ServerEvent::Error {
                        code: ErrorCode::NotYourTurn,
                        message: Some("It's not your turn".into()),
                        details: Some(ErrorDetails {
                            player_id: Some(player_id),
                            action: Some(format!("{:?}", action)),
                            card_id: None,
                        }),
                    });
                    return None;
                }
 
                let MoveResult {
                    success,
                    drawn_cards,
                    discarded_cards
                } = match game_state.apply_move(player_idx, action.clone()) {
                    Ok(success) => success,
                    Err(move_err) => {
                        let code = match move_err {
                            MoveError::DoesNotFit => ErrorCode::InvalidMove,
                            MoveError::NotAllowed => ErrorCode::InvalidMove,
                            MoveError::InvalidIndex{..} => ErrorCode::CardNotFound,
                            MoveError::NotYourTurn => ErrorCode::NotYourTurn,
                        };
                        warn!("Invalid move by player {:?} ({:?}): {:?} -> {:?}", player_id, player_idx, action, move_err);
                        send_to(players, player_id, ServerEvent::Error {
                            code,
                            message: None,
                            details: Some(ErrorDetails {
                                player_id: Some(player_id),
                                action: Some(format!("{:?}", action)),
                                card_id: None,
                            }),
                        });
                        return None;
                    }
                };
 
                if let Some(token) = timer.take() {
                    token.cancel();
                }
 
                if let Some(drawn_cards) = drawn_cards {
                    send_to(players, player_id, ServerEvent::HandRefill {
                        player_id,
                        player_idx,
                        cards: drawn_cards,
                        turn_seconds_remaining: self.turn_seconds_remaining()
                    });
                }
 
                process_action(players, game_state, &action, &success, player_id, self.turn_seconds_remaining());
 
                if let MoveSuccess::GameWon { winner_idx } = success {
                    let winner_id = self.idx_to_id[&winner_idx];
 
                    info!(
                        "Game won by player {:?} ({:?}) in room {}",
                        winner_id,
                        winner_idx,
                        self.game_id
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
 
                    broadcast(players, &ServerEvent::TurnEnded {
                        next_player_id: next_id,
                        next_player_idx: next_idx,
                        turn_seconds: self.turn_seconds,
                        timed_out_player_id: None,
                        timed_out_player_idx: None,
                    });
                    send_full_state(players, game_state);
 
                    start_timer(next_id, self.turn_seconds, timer, cmd_tx);
                }
 
                None
            },

            RoomCommand::TurnTimeout { player_id } => {
                // Sanity check: timeout should only happen for the current player
                if Some(player_id) != self.idx_to_id.get(&self.current_player).copied() {
                    warn!(
                        "TurnTimeout for {:?}, but current player is {:?}",
                        player_id, self.current_player
                    );
                    return None;
                }
 
                let timed_out_idx = self.current_player;
 
                // Cancel any existing timer (it already fired, but this keeps the state clean)
                if let Some(token) = timer.take() {
                    token.cancel();
                }
 
                // Advance the game's turn
                let next_idx = game_state.advance_turn();
                let next_id = self.idx_to_id[&next_idx];
                self.current_player = next_idx;
 
                // Notify all players that the turn ended due to timeout
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
 
                // Send updated game state to everyone
                send_full_state(players, game_state);
 
                // Start the timer for the new current player
                start_timer(next_id, self.turn_seconds, timer, cmd_tx);
 
                None
            },

            RoomCommand::PlayerLeft { player_id } => {
                trace!("PlayerLeft for {:?}", player_id);

                let info = match players.get(&player_id) {
                    Some(info) => info,
                    None => {
                        warn!("PlayerLeft for unknown player {:?}", player_id);
                        return None;
                    }
                };

                let leaving_idx = info.player_idx;

                // Cancel this player's pending disconnect grace timer, if any.
                if let Some(token) = self.disconnect_tokens.remove(&player_id) {
                    token.cancel();
                }

                // Cancel the normal turn timer.
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

                // Explicit leave is permanent.
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

            RoomCommand::UnsubscribePlayer { player_id } => {
                trace!("UnsubscribePlayer for {:?}", player_id);

                let info = match players.get_mut(&player_id) {
                    Some(info) => info,
                    None => {
                        warn!("UnsubscribePlayer for unknown player {:?}", player_id);
                        return None;
                    }
                };

                // Already disconnected; don't create another grace timer.
                if !info.connected {
                    return None;
                }

                info.connected = false;
                let idx = info.player_idx;

                // If this player is currently taking their turn, pause their
                // turn timer. It will be restarted if they reconnect.
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

                let disconnect_token =
                    Self::spawn_disconnect_grace_timer(player_id, cmd_tx);

                self.disconnect_tokens
                    .insert(player_id, disconnect_token);

                // IMPORTANT:
                // Do not remove the player and do not leave PlayingPhase.
                None
            }

            RoomCommand::PlayerReconnected { player_id } => {
                let info = match players.get_mut(&player_id) {
                    Some(info) => info,
                    None => {
                        warn!("Reconnect from unknown player {:?}", player_id);
                        return None;
                    }
                };
 
                if info.connected {
                    warn!("Player {:?} is already connected", player_id);
                    return None;
                }
 
                // Restore connection
                info.connected = true;
 
                // Cancel *this player's* disconnect grace timer specifically -
                // never touch another player's entry.
                if let Some(token) = self.disconnect_tokens.remove(&player_id) {
                    token.cancel();
                }
 
                let idx = info.player_idx;
 
                // for now just opponent name
                if let Some(event) = build_full_state(game_state, info.player_idx, String::from("Opponent name")) {
                    send_to(players, player_id, event);
                }
 
                // Notify opponent
                broadcast(players, &ServerEvent::PlayerReconnected {
                    player_id,
                    player_idx: idx,
                    turn_seconds_remaining: self.turn_seconds_remaining()
                });
 
                // If it's currently this player's turn, restart the turn timer -
                // it was paused specifically because they were the current player.
                if idx == self.current_player {
                    start_timer(player_id, self.turn_seconds, timer, cmd_tx);
                }
 
                None
            },

            RoomCommand::DisconnectTimeout { player_id } => {
                let info = match players.get(&player_id) {
                    Some(info) => info,
                    None => return None,
                };
 
                if info.connected {
                    return None;
                }
 
                // Clean up this player's own grace-timer entry only.
                if let Some(token) = self.disconnect_tokens.remove(&player_id) {
                    token.cancel();
                }
 
                let winner = self
                    .id_to_idx
                    .iter()
                    .find(|(pid, _)| **pid != player_id)
                    .map(|(&pid, &idx)| (pid, idx));
 
                if let Some((winner_id, winner_idx)) = winner {
                    // Keep both players as participants.
                    let participants = self.id_to_idx.clone();
 
                    // Remove the disconnected player from currently connected players.
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
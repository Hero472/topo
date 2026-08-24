use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use async_trait::async_trait;
use rand::RngExt;

use crate::{
    core::{
        game::state::{state_types::Seed, GameState, Seconds},
        game_id::GameId,
        player::PlayerIdx,
    },
    infrastructure::{
        error::ErrorCode,
        room::utils::{broadcast, send_full_state, send_to, start_timer},
        server_event::ServerEvent,
    },
};

use super::*;

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
            send_to(players, *player_id, ServerEvent::GameOver {
                winner_id,
                winner_idx,
                reason: reason.clone(),
            });
        }

        Self {
            game_id,
            turn_seconds,
            winner_id,
            winner_idx,
            reason,
            participants,
            play_again: HashSet::new(),
        }
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
        match cmd {
            RoomCommand::SubscribePlayer { player_id, sender } => {
                let player_idx = match self.participants.get(&player_id) {
                    Some(&idx) => idx,
                    None => return None,
                };

                players.insert(player_id, PlayerInfo {
                    username: String::new(),
                    tx: sender,
                    player_idx,
                    connected: true,
                });

                if let Some(game_state) = state.as_ref() {
                    send_full_state(players, game_state); // or send_to just this player if that helper exists
                }

                send_to(players, player_id, ServerEvent::GameOver {
                    winner_id: self.winner_id,
                    winner_idx: self.winner_idx,
                    reason: self.reason.clone(),
                });

                broadcast(players, &ServerEvent::PlayerReconnected {
                    player_id,
                    player_idx,
                    turn_seconds_remaining: Seconds::from(0), // or omit if this field doesn't make sense post-game
                });
            }

            // ---------------------------------------------------------
            // PLAYER LEFT
            // ---------------------------------------------------------
            RoomCommand::PlayerLeft { player_id } => {
                players.remove(&player_id);
                self.play_again.remove(&player_id);
                self.participants.remove(&player_id); // they're not coming back — don't count them for rematch

                if players.is_empty() {
                    let _ = cmd_tx.send(RoomCommand::Shutdown);
                    return None;
                }

                broadcast(players, &ServerEvent::OpponentLeft);
            }

            RoomCommand::UnsubscribePlayer { player_id } => {
                players.remove(&player_id);
                // keep in self.participants — this might just be a socket blip, allow reconnect

                if players.is_empty() {
                    let _ = cmd_tx.send(RoomCommand::Shutdown);
                    return None;
                }

                broadcast(players, &ServerEvent::OpponentLeft);
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

                self.play_again.insert(player_id);

                let all_present = self
                    .participants
                    .keys()
                    .all(|id| players.contains_key(id));

                let all_agreed = self
                    .participants
                    .keys()
                    .all(|id| self.play_again.contains(id));

                // Wait until both players are connected and both
                // have pressed "Play Again".
                if !all_present || !all_agreed {
                    return None;
                }

                // -----------------------------------------------------
                // CREATE NEW GAME
                // -----------------------------------------------------

                let seed = Seed(rand::rng().random::<u64>());

                let mut fresh_state = GameState::new(
                    self.game_id.clone(),
                    seed,
                    13,
                    5,
                    self.turn_seconds,
                );

                fresh_state.start_game();

                // Restore the relationship:
                //
                // PlayerId -> PlayerIdx
                //
                // into the new GameState.
                for (&player_id, &player_idx) in &self.participants {
                    if let Some(board) = fresh_state
                        .players
                        .iter_mut()
                        .find(|board| board.player_idx == player_idx)
                    {
                        board.player_id = Some(player_id);
                    }
                }

                // -----------------------------------------------------
                // DETERMINE STARTER
                // -----------------------------------------------------

                let starter_idx = fresh_state.current_turn;

                let starter_id = self
                    .participants
                    .iter()
                    .find(|(_, player_idx)| **player_idx == starter_idx)
                    .map(|(&player_id, _)| player_id)
                    .expect("Starter must exist in participants");

                // -----------------------------------------------------
                // REPLACE STATE
                // -----------------------------------------------------

                *state = Some(fresh_state);

                let game_state = state
                    .as_ref()
                    .expect("fresh state was just inserted");

                // -----------------------------------------------------
                // BUILD PLAYER MAPPINGS
                // -----------------------------------------------------

                let id_to_idx = self.participants.clone();

                let idx_to_id: HashMap<PlayerIdx, PlayerId> = self
                    .participants
                    .iter()
                    .map(|(&player_id, &player_idx)| {
                        (player_idx, player_id)
                    })
                    .collect();

                // -----------------------------------------------------
                // RESET REMATCH STATE
                // -----------------------------------------------------

                self.play_again.clear();

                // -----------------------------------------------------
                // NOTIFY CLIENTS
                // -----------------------------------------------------

                send_full_state(players, game_state);

                broadcast(
                    players,
                    &ServerEvent::GameStarted {
                        current_player_id: starter_id,
                        current_player_idx: starter_idx,
                        turn_seconds: self.turn_seconds,
                    },
                );

                // -----------------------------------------------------
                // START TIMER
                // -----------------------------------------------------

                start_timer(
                    starter_id,
                    self.turn_seconds,
                    timer,
                    cmd_tx,
                );

                // -----------------------------------------------------
                // ENTER PLAYING PHASE
                // -----------------------------------------------------

                return Some(Box::new(PlayingPhase {
                    game_id: self.game_id.clone(),
                    turn_seconds: self.turn_seconds,
                    disconnect_token: None,
                    current_player: starter_idx,
                    id_to_idx,
                    idx_to_id,
                    turn_started_at: Instant::now(),
                }));
            }

            // ---------------------------------------------------------
            // ACTION AFTER GAME OVER
            // ---------------------------------------------------------
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
            }

            // ---------------------------------------------------------
            // PLAYER KNOWN
            // ---------------------------------------------------------
            RoomCommand::IsPlayerKnown { player_id, reply } => {
                let known = self.participants.contains_key(&player_id);
                let _ = reply.send(known);
            }

            _ => {}
        }

        None
    }
}
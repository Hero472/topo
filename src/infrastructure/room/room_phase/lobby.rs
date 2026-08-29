use std::{collections::HashSet, time::Instant};

use async_trait::async_trait;
use log::{debug, warn};
use rand::RngExt;
use tokio::task::JoinHandle;
use crate::{
    core::{game::state::{Seconds, state_types::Seed}, game_id::GameId, player::{PlayerId, PlayerIdx}}, infrastructure::{
        message::GameMessage, room::utils::{broadcast, send_full_state, start_timer}, server_event::ServerEvent
    }
};

use super::*;

const BOARD_SIZE: usize = 13;
const HAND_SIZE: usize = 5;
const START_COUNTDOWN_SECONDS: u64 = 3;
const GRACE_PERIOD_SECONDS: u64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LobbyState {
    Waiting,
    Starting,
}

pub struct LobbyPhase {
    pub game_id: GameId,
    pub turn_seconds: Seconds,
    seed: Option<Seed>,
    state: LobbyState,
    ready: HashSet<PlayerId>,
    pending_disconnects: HashMap<PlayerId, JoinHandle<()>>
}

impl LobbyPhase {
    pub fn new(game_id: GameId, turn_seconds: Seconds) -> Self {
        Self {
            game_id,
            turn_seconds,
            seed: None,
            state: LobbyState::Waiting,
            ready: HashSet::new(),
            pending_disconnects: HashMap::new()
        }
    }
 
    /// Number of players that have actually joined (i.e. have a real player_idx).
    fn joined_count(players: &HashMap<PlayerId, PlayerInfo>) -> usize {
        players
            .values()
            .filter(|info| info.player_idx != PlayerIdx(usize::MAX))
            .count()
    }
 
    /// First free slot among {0, 1}. Only ever called when joined_count < 2.
    fn next_available_idx(players: &HashMap<PlayerId, PlayerInfo>) -> PlayerIdx {
        for i in 0..2 {
            let idx = PlayerIdx(i);
            if !players.values().any(|info| info.player_idx == idx) {
                return idx;
            }
        }
        // Should be unreachable given the joined_count guard in PlayerJoined.
        warn!("next_available_idx called with no free slot");
        PlayerIdx(usize::MAX)
    }
 
    fn all_players_ready(&self, players: &HashMap<PlayerId, PlayerInfo>) -> bool {
        Self::joined_count(players) == 2 && players.keys().all(|pid| self.ready.contains(pid))
    }
 
    fn start_game(
        &mut self,
        players: &mut HashMap<PlayerId, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Box<dyn RoomPhase + Send> {
        let seed = self.seed.unwrap_or_else(|| Seed(rand::rng().random::<u64>()));
 
        let mut new_state = GameState::new(
            self.game_id.clone(),
            seed,
            BOARD_SIZE,
            HAND_SIZE,
            self.turn_seconds,
        );
        new_state.start_game(); // Starter idx can be either 0 or 1
 
        for (&pid, pinfo) in players.iter() {
            if let Some(board) = new_state
                .players
                .iter_mut()
                .find(|b| b.player_idx == pinfo.player_idx)
            {
                board.player_id = Some(pid);
            }
        }
 
        let starter_idx = new_state.current_turn;
        let starter_id = players
            .iter()
            .find(|(_, pinfo)| pinfo.player_idx == starter_idx)
            .map(|(&pid, _)| pid)
            .expect("Starter not found in players map");
 
        send_full_state(players, &new_state);
 
        broadcast(
            players,
            &ServerEvent::GameStarted {
                current_player_id: starter_id,
                current_player_idx: starter_idx,
                turn_seconds: self.turn_seconds,
            },
        );
 
        *state = Some(new_state);
 
        start_timer(starter_id, self.turn_seconds, timer, cmd_tx);
 
        let id_to_idx: HashMap<PlayerId, PlayerIdx> = players
            .iter()
            .map(|(&pid, info)| (pid, info.player_idx))
            .collect();
        let idx_to_id: HashMap<PlayerIdx, PlayerId> = id_to_idx
            .iter()
            .map(|(&pid, &idx)| (idx, pid))
            .collect();
 
        Box::new(PlayingPhase {
            game_id: self.game_id.clone(),
            turn_seconds: self.turn_seconds,
            pending_disconnects: HashMap::new(),
            current_player: starter_idx,
            id_to_idx,
            idx_to_id,
            turn_started_at: Instant::now(),
        })
    }
}

#[async_trait]
impl RoomPhase for LobbyPhase {
    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<PlayerId, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase + Send>> {
        debug!("HANDLE_LOBBY_CMD: {:?}", cmd);

        if self.state == LobbyState::Starting && !matches!(cmd, RoomCommand::StartGame){
            return None;
        }

        match cmd {

            RoomCommand::SubscribePlayer { player_id, sender } => {
                players.entry(player_id)
                    .and_modify(|info| info.tx = sender.clone())
                    .or_insert(PlayerInfo {
                        username: String::new(),
                        tx: sender,
                        player_idx: PlayerIdx(usize::MAX),
                        connected: false,
                    });
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
            
            RoomCommand::PlayerJoined { player_id, username } => {
                let joined_before = Self::joined_count(players);

                if joined_before >= 2 {
                    warn!("PlayerJoined for {:?} but lobby is already full - ignored", player_id);
                    return None;
                }

                let idx = Self::next_available_idx(players);

                let info = match players.get_mut(&player_id) {
                    Some(info) => info,
                    None => {
                        warn!("PlayerJoined for unsubscribed player {:?}", player_id);
                        return None;
                    }
                };

                if info.player_idx != PlayerIdx(usize::MAX) {
                    warn!("Duplicate PlayerJoined for {:?} - ignored", player_id);
                    return None;
                }

                info.player_idx = idx;
                info.connected = true;
                info.username = username.clone();

                let new_player_tx = info.tx.clone(); 
                for (existing_id, existing_info) in players.iter() {
                    if *existing_id != player_id && existing_info.player_idx != PlayerIdx(usize::MAX) {
                        let _ = new_player_tx.send(GameMessage {
                            to: None,
                            event: ServerEvent::PlayerJoined {
                                player_id: *existing_id,
                                player_idx: existing_info.player_idx,
                                username: existing_info.username.clone(),
                            },
                        });
                    }
                }

                // Then broadcast the NEW player's arrival to EVERYONE (including the new player)
                broadcast(
                    players,
                    &ServerEvent::PlayerJoined {
                        player_id,
                        player_idx: idx,
                        username: username.clone(),
                    },
                );

                if joined_before == 0 {
                    broadcast(
                        players,
                        &ServerEvent::WaitingForPlayer {
                            game_id: self.game_id.clone(),
                        },
                    );
                }

                None
            }

            RoomCommand::PlayerReady { player_id } => {
                if !players.contains_key(&player_id) {
                    warn!("PlayerReady for unknown player {:?}", player_id);
                    return None;
                }

                if self.ready.insert(player_id) {
                    broadcast(
                        players,
                        &ServerEvent::PlayerReady { player_id },
                    );
                }

                if self.all_players_ready(players) {
                    self.state = LobbyState::Starting;

                    let tx = cmd_tx.clone();

                    tokio::spawn(async move {
                        for seconds in (1..=3).rev() {
                            // Tell the clients how long remains.
                            let _ = tx.send(RoomCommand::GameStartingTick {
                                seconds_remaining: seconds,
                            });

                            tokio::time::sleep(
                                std::time::Duration::from_secs(1)
                            ).await;
                        }

                        let _ = tx.send(RoomCommand::StartGame);
                    });
                }

                None
            }

            RoomCommand::StartGame => {
                if self.state != LobbyState::Starting {
                    return None;
                }

                if !self.all_players_ready(players) {
                    // Something changed during the countdown.
                    self.state = LobbyState::Waiting;
                    return None;
                }

                Some(self.start_game(players, state, timer, cmd_tx))
            }

            RoomCommand::NetworkDisconnect { player_id } => {
                let player_idx = if let Some(info) = players.get_mut(&player_id) {
                    info.connected = false;
                    Some(info.player_idx)
                } else {
                    None
                };

                if let Some(player_idx) = player_idx {
                    broadcast(
                        players,
                        &ServerEvent::PlayerDisconnected {
                            player_id,
                            player_idx,
                            grace_period_seconds: GRACE_PERIOD_SECONDS,
                        },
                    );

                    let tx = cmd_tx.clone();
                    let handle = tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(GRACE_PERIOD_SECONDS)).await;
                        let _ = tx.send(RoomCommand::DisconnectTimeout { player_id });
                    });

                    self.pending_disconnects.insert(player_id, handle);
                }
                None
            }

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

                if let Some(player_idx) = player_idx {
                    // ✅ THE FIX: Send existing players to the reconnecting player.
                    // This ensures that if the client lost state or incorrectly 
                    // triggered a reconnect instead of a join, it still gets the full roster.
                    let reconnecting_tx = players[&player_id].tx.clone();
                    for (existing_id, existing_info) in players.iter() {
                        if *existing_id != player_id && existing_info.player_idx != PlayerIdx(usize::MAX) {
                            let _ = reconnecting_tx.send(GameMessage {
                                to: None,
                                event: ServerEvent::PlayerJoined {
                                    player_id: *existing_id,
                                    player_idx: existing_info.player_idx,
                                    username: existing_info.username.clone(),
                                },
                            });
                        }
                    }

                    broadcast(
                        players,
                        &ServerEvent::PlayerReconnected {
                            player_id,
                            player_idx,
                            turn_seconds_remaining: Seconds(0),
                        },
                    );
                } else {
                    warn!("PlayerReconnected for unknown player {:?}, ignoring", player_id);
                }
                None
            }

            RoomCommand::PlayerLeft { player_id } => {
                if let Some(handle) = self.pending_disconnects.remove(&player_id) {
                    handle.abort();
                }
                if let Some(info) = players.remove(&player_id) {
                    self.ready.remove(&player_id);
                    broadcast(
                        players,
                        &ServerEvent::PlayerLeft {
                            player_id,
                            player_idx: info.player_idx,
                        },
                    );
                }

                if players.is_empty() {
                    let _ = cmd_tx.send(RoomCommand::Shutdown);
                }
                None
            }

            RoomCommand::DisconnectTimeout { player_id } => {
                self.pending_disconnects.remove(&player_id);
                if let Some(info) = players.remove(&player_id) {
                    self.ready.remove(&player_id);
                    broadcast(
                        players,
                        &ServerEvent::PlayerLeft {
                            player_id,
                            player_idx: info.player_idx,
                        },
                    );
                }

                if players.is_empty() {
                    let _ = cmd_tx.send(RoomCommand::Shutdown);
                }
                None
            }

            RoomCommand::GameStartingTick { seconds_remaining } => {
                if self.state != LobbyState::Starting {
                    return None;
                }

                broadcast(
                    players,
                    &ServerEvent::GameStarting {
                        seconds_remaining,
                    },
                );

                None
            }

            RoomCommand::IsPlayerKnown { player_id, reply } => {
                let known = players.contains_key(&player_id);
                let _ = reply.send(known);
                None
            }

            RoomCommand::SetSeed(s) => {
                self.seed = Some(s);
                None
            }

            _ => {
                // Silently ignore other commands while in lobby
                None
            }
        }
    }
}
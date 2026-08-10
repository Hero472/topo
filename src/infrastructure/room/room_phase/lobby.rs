use std::time::Instant;

use async_trait::async_trait;
use log::warn;
use rand::RngExt;
use crate::{
    core::{game::state::{Seconds, state_types::Seed}, player::{PlayerId, PlayerIdx}},
    infrastructure::{
        room::utils::{broadcast, send_full_state, start_timer},
        server_event::ServerEvent
    }
};

use super::*;

pub struct LobbyPhase {
    pub room_id: String,
    pub turn_seconds: Seconds,
    next_player_idx: PlayerIdx,
    seed: Option<Seed>
}

impl LobbyPhase {
    pub fn new(room_id: String, turn_seconds: Seconds) -> Self {
        Self {
            room_id,
            turn_seconds,
            next_player_idx: PlayerIdx(0),
            seed: None
        }
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
            
            RoomCommand::PlayerJoined { player_id, username } => {
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

                let idx = self.next_player_idx;
                info.player_idx = idx;
                info.connected = true;
                self.next_player_idx.0 += 1;

                info.username = username.clone();

                broadcast(
                    players,
                    &ServerEvent::PlayerJoined {
                        player_id,
                        player_idx: idx,
                        username: username.clone(),
                    },
                );

                if self.next_player_idx == PlayerIdx(2) {
                    let seed = self.seed.unwrap_or_else(|| Seed(rand::rng().random::<u64>()));

                    let mut new_state = GameState::new(
                        self.room_id.clone(),
                        seed,
                        13,
                        5,
                        self.turn_seconds
                    );
                    new_state.start_game(); // Player IDx starter can be either 0 or 1

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

                    return Some(Box::new(PlayingPhase {
                        room_id: self.room_id.clone(),
                        turn_seconds: self.turn_seconds,
                        disconnect_token: None,
                        current_player: starter_idx,
                        id_to_idx,
                        idx_to_id,
                        turn_started_at: Instant::now()
                    }));
                }

                None
            }

            RoomCommand::PlayerLeft { player_id } => {
                if let Some(info) = players.remove(&player_id) {
                        broadcast(
                        players,
                        &ServerEvent::PlayerLeft {
                            player_id,
                            player_idx: info.player_idx,
                        },
                    );
                }
                if players.is_empty() {
                    return Some(Box::new(OverPhase::new(self.room_id.clone(), cmd_tx.clone())));
                }
                None
            },

            RoomCommand::IsPlayerKnown { player_id, reply } => {
                let known = players.contains_key(&player_id);
                let _ = reply.send(known);
                None
            },

            RoomCommand::PlayerReconnected { player_id } => {
                if let Some(info) = players.get_mut(&player_id) {
                    info.connected = true;
                }
                None
            },

            RoomCommand::UnsubscribePlayer { player_id } => {
                players.remove(&player_id);
                None
            },

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
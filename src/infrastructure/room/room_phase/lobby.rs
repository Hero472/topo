use async_trait::async_trait;
use log::{info, warn};
use rand::RngExt;
use crate::{core::game::state::state_types::Seed, infrastructure::{room::utils::{broadcast, send_full_state, start_timer}, server_event::ServerEvent}};

use super::*;

pub struct LobbyPhase {
    pub room_id: String,
    pub turn_seconds: u64
}

#[async_trait]
impl RoomPhase for LobbyPhase {

    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<usize, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase>> {

        match cmd {
            RoomCommand::SubscribePlayer { player_id, sender } => {
                players.entry(player_id).or_insert(
                    PlayerInfo { 
                        username: String::new(),
                        tx: sender 
                });
                None
            },
            RoomCommand::PlayerJoined { player_id, username } => {

                let info = match players.get_mut(&player_id) {
                    Some(info) => info,
                    None => {
                        warn!("PlayerJoined for unsubcribed player {}", player_id);
                        return None
                    }
                };

                info.username = username.clone();

                broadcast(players, &ServerEvent::PlayerJoined {
                    player_id,
                    username: username.clone(),
                });

                if players.len() == 2 {
                    let player_ids = players.keys().copied().collect();
                    info!(
                        "Both players present, starting game in room {}. Players: {:?}",
                        self.room_id, player_ids
                    );

                    let mut new_state = GameState::new(
                        self.room_id.clone(),
                        Seed(rand::rng().random::<u64>()),
                        13,
                        5,
                    );
                    new_state.start_game();

                    let starter_id = new_state
                        .current_player_idx()
                        .expect("GameState should have a current player after start_game");

                    send_full_state(players, &new_state);

                    let turn_seconds = self.turn_seconds;

                    broadcast(players, &ServerEvent::GameStarted {
                        current_player_id: starter_idx,
                        turn_seconds,
                    });

                    *state = Some(new_state);

                    start_timer(starter_id, turn_seconds, timer, cmd_tx);

                    return Some(Box::new(PlayingPhase {
                        room_id: self.room_id.clone(),
                        turn_seconds,
                        current_player: starter_id,
                    }));
                }
                
                None
            },

            RoomCommand::PlayerLeft { player_id } => {
                info!("Player {} left the lobby", player_id);
                players.remove(&player_id);
                broadcast(players, &ServerEvent::PlayerLeft { player_id });
                None
            },

            _ => {
                // Silently ignore other commands while in lobby
                None
            }
        }

    }

}
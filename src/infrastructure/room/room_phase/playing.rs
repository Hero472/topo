use async_trait::async_trait;
use log::{info, debug, warn};
use std::collections::HashMap;

use crate::core::game::actions::{MoveSuccess, MoveError};
use crate::core::game::state::{GameState, Seconds};
use crate::core::player::PlayerIdx;
use crate::infrastructure::error::{ErrorCode, ErrorDetails};
use crate::infrastructure::room::player_info::PlayerInfo;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::server_event::ServerEvent;
use crate::infrastructure::room::utils::*;


use super::*; 

pub struct PlayingPhase {
    pub room_id: String,
    pub turn_seconds: Seconds,
    pub current_player: PlayerIdx,
    pub id_to_idx: HashMap<PlayerId, PlayerIdx>,
    pub idx_to_id: HashMap<PlayerIdx, PlayerId>,
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

        match cmd {

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

                let result = match game_state.apply_move(player_idx, action.clone()) {
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

                process_action(players, game_state, &action, &result, player_id);

                if let MoveSuccess::GameWon { winner_idx } = result {
                    let winner_external = self.idx_to_id[&winner_idx];
                    info!("Game won by player {:?} ({:?}) in room {}",
                          winner_external, winner_idx, self.room_id);
                    broadcast(players, &ServerEvent::GameOver {
                        winner_id: winner_external,
                        winner_idx,
                        reason: "All cards cleared".into(),
                    });
                    return Some(Box::new(OverPhase { room_id: self.room_id.clone() }));
                }

                if result.turn_ended() {
                    let next_idx = game_state.advance_turn();
                    let next_id = self.idx_to_id[&next_idx];
                    info!("Turn ended. Next player: {:?} ({:?})", next_id, next_idx);
                    broadcast(players, &ServerEvent::TurnEnded {
                        next_player_id: next_id,
                        next_player_idx: next_idx,
                        turn_seconds: self.turn_seconds,
                        timed_out_player_id: None,
                        timed_out_player_idx: None,
                    });
                    send_full_state(players, game_state);
                    start_timer(next_id, self.turn_seconds, timer, cmd_tx);
                    self.current_player = next_idx;
                } else {
                    // Turn continues
                    start_timer(player_id, self.turn_seconds, timer, cmd_tx);
                }

                None
            }
            _ => None,
        }

    }
}
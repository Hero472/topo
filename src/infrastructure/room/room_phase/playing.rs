use async_trait::async_trait;
use log::{info, debug, warn};
use std::collections::HashMap;

use crate::core::game::actions::{MoveSuccess, MoveError};
use crate::core::game::state::GameState;
use crate::infrastructure::error::{ErrorCode, ErrorDetails};
use crate::infrastructure::room::player_info::PlayerInfo;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::server_event::ServerEvent;
use crate::infrastructure::room::utils::*;


use super::*; 

pub struct PlayingPhase {
    pub room_id: String,
    pub turn_seconds: u64,
    pub current_player: usize,
}

#[async_trait]
impl RoomPhase for PlayingPhase {
    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<usize, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase>> {

        let game_state = state.as_mut().expect("PlayingPhase requires a game state");

        match cmd {

            RoomCommand::PlayerAction { player_id, action } => {
                debug!("Received action from player {}: {:?}", player_id, action);

                if player_id != self.current_player {

                    send_to(players, player_id, ServerEvent::Error {
                        code: ErrorCode::NotYourTurn,
                        message: Some("It's not your turn".into()), 
                        details: Some(ErrorDetails {
                            player_id: Some(player_id),
                            action: Some(format!("{:?}", action)),
                            card_id: None,
                        }),
                    });
                    
                    return None
                }

                let result = match game_state.apply_move(player_id, action.clone()) {
                    Ok(success) => success,
                    Err(move_err) => {
                        
                        let code = match move_err {
                            MoveError::DoesNotFit => ErrorCode::InvalidMove,
                            MoveError::NotAllowed => ErrorCode::InvalidMove,
                            MoveError::InvalidIndex{..} => ErrorCode::CardNotFound,
                            MoveError::NotYourTurn => ErrorCode::NotYourTurn,
                        };

                        warn!("Invalid move by player {}: {:?} -> {:?}", player_id, action, move_err);

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

                if let MoveSuccess::GameWon { winner_id } = result {
                    info!("Game won by player {} in room {}", winner_id, self.room_id);
                    broadcast(players, &ServerEvent::GameOver {
                        winner_id,
                        reason: "All cards cleared".into(),
                    });
                    return Some(Box::new(OverPhase { room_id: self.room_id.clone()}));
                }

                if result.turn_ended() {
                    let next = game_state.advance_turn();
                    info!("Turn ended. Next player: {}", next);
                    broadcast(players, &ServerEvent::TurnEnded {
                        next_player_id: next,
                        turn_seconds: self.turn_seconds,
                        timed_out_player_id: None,
                    });
                    send_full_state(players, game_state);
                    start_timer(next, self.turn_seconds, timer, cmd_tx);
                    self.current_player = next;
                } else {
                    // Turn continues
                    start_timer(self.current_player, self.turn_seconds, timer, cmd_tx);
                }

                None
            },
            _ => None,
        }

    }
}
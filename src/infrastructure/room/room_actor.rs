use rand::RngExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;
use log::{info, debug, warn, error};

use crate::core::game::actions::{Action, MoveSuccess, MoveError};
use crate::core::game::state::GameState;
use crate::infrastructure::error::{ErrorCode, ErrorDetails};
use crate::infrastructure::full_state::build_full_state;
use crate::infrastructure::message::GameMessage;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::server_event::ServerEvent;

struct PlayerInfo {
    username: String,
    tx: mpsc::UnboundedSender<GameMessage>
}


fn broadcast(players: &HashMap<usize, PlayerInfo>, event: &ServerEvent) {
    debug!("Broadcasting event: {:?}", event);
    for (&pid, info) in players.iter() {
        if let Err(e) = info.tx.send(GameMessage {
            to: None,
            event: event.clone(),
        }
        ) {
            warn!("Failed to send broadcast to player {}: {}", pid, e);
        }
    }
}

fn send_to(players: &HashMap<usize, PlayerInfo>, player_id: usize, event: ServerEvent) {
    debug!("Sending to player {}: {:?}", player_id, event);
    if let Some(info) = players.get(&player_id) {
        if let Err(e) = info.tx.send(GameMessage {
            to: Some(player_id),
            event,
        }) {
            warn!("Failed to send to player {}: {}", player_id, e);
        }
    } else {
        warn!("Cannot send to unknown player {}", player_id);
    }
}

fn send_full_state(players: &HashMap<usize, PlayerInfo>, state: &GameState) {
    for (&pid, _info) in players {
        let opponent = players.iter().find(|(id, _)| **id != pid);
        let opp_name = opponent
            .map(|(_, o)| o.username.clone())
            .unwrap_or_default();
        if let Some(event) = build_full_state(state, pid, opp_name) {
            debug!("Sending FullState to player {}", pid);
            send_to(players, pid, event);
        } else {
            warn!("build_full_state returned None for player {} – no full state sent", pid);
        }
    }
}

fn process_action(
    players: &HashMap<usize, PlayerInfo>,
    state: &GameState,
    action: &Action,
    result: &MoveSuccess,
    acting_player: usize
) {
    debug!("Generating events for player {} action {:?} result {:?}", acting_player, action, result);
    let events = generate_events(state, action, result, acting_player, false);

    for event in &events {
        match event {
            ServerEvent::CardDrawn { .. } => {
                send_to(players, acting_player, event.clone())
            }
            ServerEvent::OpponentUpdate { player_idx, .. } => {
                if let Some((&other_id, _)) = players.iter().find(|(id, _)| **id != *player_idx) {
                    send_to(players, other_id, event.clone());
                }
            }
            _ => broadcast(players, event),
        }
    }

}

pub async fn room_actor(
    room_id: String,
    turn_seconds: u64,
    mut cmd_rx: mpsc::UnboundedReceiver<RoomCommand>,
    cmd_tx: mpsc::UnboundedSender<RoomCommand>,
) {
    info!("🎮 room_actor started for room {}", room_id);

    let mut state: Option<GameState> = None;
    let mut players: HashMap<usize, PlayerInfo> = HashMap::new();
    let mut current_timer_cancel: Option<CancellationToken> = None;

    // ── State machine phases ──────────────────────────────────────────────
    enum Phase {
        Lobby,
        Playing { current_player: usize },
        Over,
    }

    let mut phase = Phase::Lobby;

    fn start_timer(
        player_id: usize,
        seconds: u64,
        current_cancel: &mut Option<CancellationToken>,
        tx: &mpsc::UnboundedSender<RoomCommand>,
    ) {
        if let Some(token) = current_cancel.take() {
            debug!("Cancelling previous timer for player {}", player_id);
            token.cancel();
        }
        let token = CancellationToken::new();
        let cancel = token.clone();
        *current_cancel = Some(cancel);

        debug!("Starting turn timer for player {}: {} seconds", player_id, seconds);
        let tx = tx.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = sleep(Duration::from_secs(seconds)) => {
                    debug!("Turn timeout for player {}", player_id);
                    let _ = tx.send(RoomCommand::TurnTimeout { player_id });
                }
                _ = token.cancelled() => {
                    debug!("Timer cancelled for player {}", player_id);
                }
            }
        });
    }

    loop {
        match &mut phase {
            Phase::Lobby => loop {
                match cmd_rx.recv().await {
                    Some(RoomCommand::SubscribePlayer { player_id, sender }) => {
                        debug!("Player {} subscribed (sender registered)", player_id);
                        players.entry(player_id).or_insert(PlayerInfo {
                            username: String::new(),
                            tx: sender,
                        });
                    }

                    Some(RoomCommand::PlayerJoined { player_id, username }) => {
                        if let Some(info) = players.get_mut(&player_id) {
                            info!("Player {} ({}) joined room {}", player_id, username, room_id);
                            info.username = username.clone();
                        } else {
                            warn!("PlayerJoined for unsubscribed player {}", player_id);
                            continue;
                        }

                        broadcast(&players, &ServerEvent::PlayerJoined {
                            player_id,
                            username: username.clone(),
                        });

                        // When we have exactly two players, start the game
                        if players.len() == 2 {
                            let player_ids: Vec<usize> = players.keys().copied().collect();
                            info!("Both players present, starting game in room {}. Players: {:?}", room_id, player_ids);
                            let mut new_state = GameState::new(
                                room_id.clone(),
                                player_ids,
                                rand::rng().random::<u64>(),
                                13,
                                5,
                            );
                            new_state.start_game();

                            let starter_id = new_state.current_player_id().unwrap();

                            info!("Game started. Current player: {}", starter_id);
                            send_full_state(&players, &new_state);
                            broadcast(&players, &ServerEvent::GameStarted {
                                current_player_id: starter_id,
                                turn_seconds,
                            });

                            state = Some(new_state);

                            // Set the first turn timer
                            start_timer(starter_id, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                            
                            phase = Phase::Playing {
                                current_player: starter_id
                            };
                            break; // exit the inner lobby loop, re‑enter main loop as Playing
                        }
                    }

                    // Player leaves before game starts – remove them
                    Some(RoomCommand::PlayerLeft { player_id }) => {
                        info!("Player {} left room {} (lobby)", player_id, room_id);
                        players.remove(&player_id);
                        broadcast(&players, &ServerEvent::PlayerLeft { player_id });
                    }

                    None => {
                        info!("Command channel closed, terminating room_actor for room {}", room_id);

                        return
                    } // channel closed, terminate actor
                    _ => {}
                }
            },

            Phase::Playing { current_player } => {
                match cmd_rx.recv().await {
                    Some(RoomCommand::PlayerAction { player_id, action }) => {
                        debug!("Received action from player {}: {:?}", player_id, action);

                        if player_id != *current_player {
                            debug!("Not player {}'s turn (current: {})", player_id, *current_player);

                            send_to(&players, player_id, ServerEvent::Error {
                                code: ErrorCode::NotYourTurn,
                                message: Some("It's not your turn.".into()),
                                details: Some(ErrorDetails {
                                    player_id: Some(player_id),
                                    action: Some(format!("{:?}", action)),
                                    card_id: None,
                                }),
                            });
                            continue;
                        }

                        // Apply move directly on the real game state
                        let result = match state.as_mut().unwrap().apply_move(player_id, action.clone()) {
                            Ok(success) => success,
                            Err(move_err) => {
                                let code = match move_err {
                                    MoveError::DoesNotFit      => ErrorCode::InvalidMove,
                                    MoveError::NotAllowed      => ErrorCode::InvalidMove,
                                    MoveError::InvalidIndex{..} => ErrorCode::CardNotFound,
                                    MoveError::NotYourTurn     => ErrorCode::NotYourTurn, // won't happen here
                                };
                                warn!("Invalid move by player {}: {:?} -> error: {:?}", player_id, action, move_err);
                                send_to(&players, player_id, ServerEvent::Error {
                                    code,
                                    message: None,
                                    details: Some(ErrorDetails {
                                        player_id: Some(player_id),
                                        action: Some(format!("{:?}", action)),
                                        card_id: None,
                                    }),
                                });
                                continue;
                            }
                        };

                        // Move accepted – cancel timer
                        if let Some(token) = current_timer_cancel.take() {
                            debug!("Cancelling timer for accepted move by player {}", player_id);
                            token.cancel();
                        }

                        let state_ref = state.as_ref().unwrap();
                        process_action(&players, state_ref, &action, &result, player_id);

                        if let MoveSuccess::GameWon { winner_id } = result {
                            info!("Game won by player {} in room {}", winner_id, room_id);
                            broadcast(&players, &ServerEvent::GameOver {
                                winner_id,
                                reason: "All cards cleared".into(),
                            });
                            phase = Phase::Over;
                        } else if result.turn_ended() {
                            let next = {
                                let state_mut = state.as_mut().unwrap();
                                state_mut.advance_turn()
                            };
                            info!("Turn ended. Next player: {}", next);
                            broadcast(&players, &ServerEvent::TurnEnded {
                                next_player_id: next,
                                turn_seconds,
                                timed_out_player_id: None,
                            });
                            send_full_state(&players, state.as_ref().unwrap());
                            start_timer(next, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                            *current_player = next;
                        } else {
                            debug!("Turn continues for player {}", *current_player);
                            start_timer(*current_player, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                        }
                    }

                    Some(RoomCommand::TurnTimeout { player_id }) => {
                        debug!("Turn timeout received for player {}", player_id);
                        if player_id != *current_player {
                            warn!("Timeout for wrong player: {} (current: {})", player_id, *current_player);
                            continue;
                        }

                        let action = Action::MoveToSide {
                            stack: rand::rng().random_range(0..4),
                            hand_idx: 0,
                        };

                        warn!("Player {} timed out, forcing action: {:?}", player_id, action);

                        // Apply the forced move (may fail if invalid)
                        let result = state
                            .as_mut()
                            .unwrap()
                            .apply_move(player_id, action.clone());

                        if let Ok(ref success) = result {
                            process_action(&players, state.as_ref().unwrap(), &action, success, player_id);
                        } else if let Err(ref err) = result {
                            error!("Forced move after timeout failed for player {}: {:?}", player_id, err);
                        }

                        let next = {
                            let state_mut = state.as_mut().unwrap();
                            state_mut.advance_turn()
                        };

                        info!("Turn ended after timeout. Next player: {}", next);
                        broadcast(&players, &ServerEvent::TurnEnded {
                            next_player_id: next,
                            turn_seconds,
                            timed_out_player_id: Some(player_id),
                        });
                        send_full_state(&players, state.as_ref().unwrap());

                        start_timer(next, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                        *current_player = next;
                    }

                    Some(RoomCommand::PlayerLeft { player_id }) => {
                        info!("Player {} disconnected during game in room {}", player_id, room_id);

                        let winner = players.keys().find(|&&id| id != player_id);
                        if let Some(&winner_id) = winner {
                            info!("Opponent disconnected, declaring player {} winner", winner_id);
                            broadcast(&players, &ServerEvent::GameOver {
                                winner_id,
                                reason: "Opponent disconnected".into(),
                            });
                        } else {
                            warn!("No remaining player to win after disconnect?");
                        }
                        phase = Phase::Over;
                    }

                    Some(RoomCommand::PlayerJoined { player_id, username }) => {
                        info!("Player {} ({}) rejoined during game in room {}", player_id, username, room_id);
                        if let Some(info) = players.get_mut(&player_id) {
                            info.username = username;
                        } else {
                            warn!("PlayerJoined for unknown player {} during game", player_id);
                            continue;
                        }

                        if let Some(ref game_state) = state {
                            let opponent_name = players.iter()
                                .find(|(id, _)| **id != player_id)
                                .map(|(_, p)| p.username.clone())
                                .unwrap_or_default();
                            if let Some(event) = build_full_state(game_state, player_id, opponent_name) {
                                debug!("Sending full state to re-joined player {}", player_id);
                                send_to(&players, player_id, event);
                            }
                        }
                    }

                    None => {
                        info!("Command channel closed, terminating room_actor for room {}", room_id);
                        return;
                    }
                    _ => {}
                }
            }

            Phase::Over => {
                info!("Room {} game over, actor shutting down", room_id);
                while let Some(_) = cmd_rx.recv().await {}
                return;
            }
        }
    }
}

fn generate_events(
    state: &GameState,
    action: &Action,
    result: &MoveSuccess,
    player_id: usize,
    _is_timeout: bool
) -> Vec<ServerEvent> {
    use Action::*;
    let mut events = Vec::new();

    match action {
        Draw => {
            let card = state.player(player_id)
                .and_then(|p| p.hand.last().cloned());
            events.push(ServerEvent::CardDrawn {
                player_id,
                card,
            });
        }

        OpenScale { .. } => {
            if let MoveSuccess::ScaleOpened { scale_id } = result {
                if let Some(card) = state.scale(scale_id).cards.last().cloned() {
                    events.push(ServerEvent::CardPlayedOnScale {
                        player_id,
                        card,
                        scale_id: *scale_id,
                        completed: false,
                    });
                }
                events.push(opponent_update(state, player_id));
            }
        }

        PlayHand { .. } | PlayPersonal { .. } | PlaySide { .. } => {
            if let MoveSuccess::ScalePlaced { scale_id, completed } = result {
                if let Some(card) = state.scale(scale_id).cards.last().cloned() {
                    events.push(ServerEvent::CardPlayedOnScale {
                        player_id,
                        card,
                        scale_id: *scale_id,
                        completed: *completed,
                    });
                }
                events.push(opponent_update(state, player_id));
            }
        }

        MoveToSide { stack, .. } => {
            if let Some(card) = state.player(player_id)
                .and_then(|p| p.side.get(*stack).and_then(|s| s.last().cloned()))
            {
                events.push(ServerEvent::CardPlacedOnSide {
                    player_id,
                    card,
                    stack: *stack,
                });
            }

            events.push(opponent_update(state, player_id));
        }

        MovePersonalToSide { stack_idx } => {
            if matches!(result, MoveSuccess::Success) {
                if let Some(card) = state.player(player_id)
                    .and_then(|p| p.side.get(*stack_idx).and_then(|s| s.last().cloned()))
                {
                    events.push(ServerEvent::CardPlacedOnSide {
                        player_id,
                        card,
                        stack: *stack_idx,
                    });
                }
                events.push(opponent_update(state, player_id));
            }
        }
    }

    if let MoveSuccess::GameWon { winner_id } = result {
        events.push(ServerEvent::GameOver {
            winner_id: *winner_id,
            reason: "All cards cleared".into(),
        });
    }

    debug!("Generated {} events for player {} action {:?}", events.len(), player_id, action);

    events
}

fn opponent_update(state: &GameState, player_id: usize) -> ServerEvent {
    let opponent = state.players.iter().find(|p| p.player_idx != player_id);
    match opponent {
        Some(o) => ServerEvent::OpponentUpdate {
            player_idx: o.player_idx,
            personal_count: o.personal.len(),
            personal_top: o.personal_top().cloned(),
            side: o.side.clone(),
        },
        None => ServerEvent::OpponentUpdate {
            player_idx: 0,
            personal_count: 0,
            personal_top: None,
            side: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        },
    }
}
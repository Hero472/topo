use rand::RngExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;
use std::pin::Pin;

use crate::core::game::actions::{Action, PlayResult};
use crate::core::game::state::GameState;
use crate::infrastructure::full_state::build_full_state;
use crate::infrastructure::message::GameMessage;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::server_event::ServerEvent;

struct PlayerInfo {
    username: String,
    tx: mpsc::UnboundedSender<GameMessage>
}


fn broadcast(players: &HashMap<usize, PlayerInfo>, event: &ServerEvent) {
    for info in players.values() {
        let _ = info.tx.send(GameMessage {
            to: None,
            event: event.clone(),
        });
    }
}

fn send_to(players: &HashMap<usize, PlayerInfo>, player_id: usize, event: ServerEvent) {
    if let Some(info) = players.get(&player_id) {
        let _ = info.tx.send(GameMessage {
            to: Some(player_id),
            event,
        });
    }
}

fn send_full_state(players: &HashMap<usize, PlayerInfo>, state: &GameState) {
    for (&pid, _info) in players {
        let opponent = players.iter().find(|(id, _)| **id != pid);
        let opp_name = opponent
            .map(|(_, o)| o.username.clone())
            .unwrap_or_default();
        if let Some(event) = build_full_state(state, pid, opp_name) {
            send_to(players, pid, event);
        }
    }
}

fn process_action(
    players: &HashMap<usize, PlayerInfo>,
    state: &GameState,
    action: &Action,
    result: &PlayResult,
    acting_player: usize
) {
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
    println!("🎮 room_actor started for room {room_id}");
    // ── Actor‑owned state ─────────────────────────────────────────────────

    let mut state: Option<GameState> = None;
    let mut players: HashMap<usize, PlayerInfo> = HashMap::new();
    let mut current_timer_cancel: Option<CancellationToken> = None;

    // ── State machine phases ──────────────────────────────────────────────
    enum Phase {
        Lobby,
        Playing {
            current_player: usize
        },
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
            token.cancel();
        }
        let token = CancellationToken::new();
        let cancel = token.clone();
        *current_cancel = Some(cancel);

        let tx = tx.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = sleep(Duration::from_secs(seconds)) => {
                    let _ = tx.send(RoomCommand::TurnTimeout { player_id });
                }
                _ = token.cancelled() => {}
            }
        });
    }

    loop {
        match &mut phase {
            Phase::Lobby => loop {
                match cmd_rx.recv().await {
                    Some(RoomCommand::SubscribePlayer { player_id, sender }) => {
                        players.entry(player_id).or_insert(PlayerInfo {
                            username: String::new(),
                            tx: sender,
                        });
                    }

                    Some(RoomCommand::PlayerJoined { player_id, username }) => {
                        if let Some(info) = players.get_mut(&player_id) {
                            info.username = username.clone();
                        } else {
                            // Should not happen normally, but handle gracefully
                            continue;
                        }

                        broadcast(&players, &ServerEvent::PlayerJoined {
                            player_id,
                            username: username.clone(),
                        });

                        // When we have exactly two players, start the game
                        if players.len() == 2 {
                            let player_ids: Vec<usize> = players.keys().copied().collect();
                            let mut new_state = GameState::new(
                                room_id.clone(),
                                player_ids,
                                rand::rng().random::<u64>(),
                                13,
                                5,
                            );
                            new_state.start_game();

                            // Randomly choose who goes first
                            let starter_id = new_state.current_player_id().unwrap();

                            // Send initial full state to both
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
                        players.remove(&player_id);
                        // If a player subscribed but never joined, we might also drop them
                        broadcast(&players, &ServerEvent::PlayerLeft { player_id });
                    }

                    None => return, // channel closed, terminate actor
                    _ => {}
                }
            },

            Phase::Playing { current_player } => {
                // Now we only await commands (no timer select)
                match cmd_rx.recv().await {
                    Some(RoomCommand::PlayerAction { player_id, action }) => {
                        if player_id != *current_player {
                            continue;
                        }

                        // Cancel the current timer because the player acted
                        if let Some(token) = current_timer_cancel.take() {
                            token.cancel();
                        }

                        let result = {
                            let state_mut = state.as_mut().unwrap();
                            state_mut.apply_move(player_id, action.clone())
                        };
                        let state_ref = state.as_ref().unwrap();
                        process_action(&players, state_ref, &action, &result, player_id);

                        if let PlayResult::GameWon { winner_id } = result {
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
                            let state_ref = state.as_ref().unwrap();
                            broadcast(&players, &ServerEvent::TurnEnded {
                                next_player_id: next,
                                turn_seconds,
                                timed_out_player_id: None,
                            });
                            send_full_state(&players, state_ref);
                            // Start timer for next player
                            start_timer(next, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                            *current_player = next;
                        } else {
                            // Turn didn't end (e.g., after Draw), restart timer for same player
                            start_timer(*current_player, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                        }
                    }

                    Some(RoomCommand::TurnTimeout { player_id }) => {
                        // Only process if it's still the same player's turn
                        if player_id != *current_player {
                            continue;
                        }

                        // Cancel token is already consumed when the timer fired.
                        // Force a move
                        let action = Action::MoveToSide {
                            stack: rand::rng().random_range(0..4),
                            hand_idx: 0,
                        };
                        let result = {
                            let state_mut = state.as_mut().unwrap();
                            state_mut.apply_move(player_id, action.clone())
                        };
                        let state_ref = state.as_ref().unwrap();
                        process_action(&players, state_ref, &action, &result, player_id);

                        // Advance turn
                        let next = {
                            let state_mut = state.as_mut().unwrap();
                            state_mut.advance_turn()
                        };
                        let state_ref = state.as_ref().unwrap();
                        broadcast(&players, &ServerEvent::TurnEnded {
                            next_player_id: next,
                            turn_seconds,
                            timed_out_player_id: Some(player_id),
                        });
                        send_full_state(&players, state_ref);
                        start_timer(next, turn_seconds, &mut current_timer_cancel, &cmd_tx);
                        *current_player = next;
                    }

                    Some(RoomCommand::PlayerLeft { player_id }) => {
                        let winner = players.keys().find(|&&id| id != player_id);
                        if let Some(&winner_id) = winner {
                            broadcast(&players, &ServerEvent::GameOver {
                                winner_id,
                                reason: "Opponent disconnected".into(),
                            });
                        }
                        phase = Phase::Over;
                    }

                    None => return,
                    _ => {}
                }
            }

            Phase::Over => {
                // Drain remaining commands (clean disconnect) until channel closes
                while let Some(_) = cmd_rx.recv().await {}
                return;
            }
        }
    }
}

fn generate_events(
    state: &GameState,
    action: &Action,
    result: &PlayResult,
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
            if let PlayResult::ScaleOpened { scale_id } = result {
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
            if let PlayResult::ScalePlaced { scale_id, completed } = result {
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
            if result.is_success() {
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

    if let PlayResult::GameWon { winner_id } = result {
        events.push(ServerEvent::GameOver {
            winner_id: *winner_id,
            reason: "All cards cleared".into(),
        });
    }

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
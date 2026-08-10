use std::{collections::HashMap, time::Duration};

use log::{debug, warn};
use tokio::{sync::mpsc, time::sleep};
use tokio_util::sync::CancellationToken;

use crate::{
    core::{
        game::{actions::{Action, MoveSuccess}, board::PlayerBoard, card::Card, state::{GameState, Seconds}}, player::{PlayerId, PlayerIdx}
    }, infrastructure::{
        full_state::build_full_state, message::GameMessage, room::{player_info::PlayerInfo, room_command::RoomCommand}, server_event::ServerEvent, views::PersonalPileView
    }
};

// repeated const in full_state.rs
const PERSONAL_PREVIEW_SIZE: usize = 7;

fn personal_pile_view(board: &PlayerBoard) -> PersonalPileView {
    PersonalPileView {
        count: board.personal.len(),
        top: board.personal_top().cloned(),
        colors: board
            .personal
            .iter()
            .rev()
            .skip(1)
            .take(PERSONAL_PREVIEW_SIZE)
            .map(|card| card.deck)
            .collect(),
    }
}

pub fn broadcast(players: &HashMap<PlayerId, PlayerInfo>, event: &ServerEvent) {
    debug!("Broadcasting event: {:?}", event);
    for (pid, info) in players.iter() {
        if let Err(e) = info.tx.send(GameMessage {
            to: None,
            event: event.clone(),
        }) {
            warn!("Failed to broadcast to player {:?}: {}", pid, e);
        }
    }
}

pub fn send_to(players: &HashMap<PlayerId, PlayerInfo>, player_id: PlayerId, event: ServerEvent) {
    debug!("Sending to player {:?}: {:?}", player_id, event);
    if let Some(info) = players.get(&player_id) {
        if let Err(e) = info.tx.send(GameMessage {
            to: Some(player_id),
            event,
        }) {
            warn!("Failed to send to player {:?}: {}", player_id, e);
        }
    } else {
        warn!("Cannot send to unknown player {:?}", player_id);
    }
}

pub fn send_full_state(players: &HashMap<PlayerId, PlayerInfo>, state: &GameState) {
    for (pid, info) in players {
        let opp_name = players
            .iter()
            .find(|(id, _)| *id != pid)
            .map(|(_, opp_info)| opp_info.username.clone())
            .unwrap_or_default();

        if let Some(event) = build_full_state(state, info.player_idx, opp_name) {
            send_to(players, *pid, event);
        } else {
            warn!("build_full_state returned None for player {:?}", pid);
        }
    }
}

pub fn process_action(
    players: &HashMap<PlayerId, PlayerInfo>,
    state: &GameState,
    action: &Action,
    result: &MoveSuccess,
    acting_player_id: PlayerId,
    turn_seconds_remaining: Seconds
) {
    let acting_player_idx = players[&acting_player_id].player_idx;

    let events = generate_events(
        players,
        state,
        action,
        result,
        acting_player_id,
        acting_player_idx,
        turn_seconds_remaining,
        false,
    );

    for event in &events {
        match event {
            ServerEvent::OpponentUpdate { player_id, .. } => {
                send_to(players, *player_id, event.clone());
            }
            _ => broadcast(players, event),
        }
    }
}

pub fn generate_events(
    players: &HashMap<PlayerId, PlayerInfo>,
    state: &GameState,
    action: &Action,
    result: &MoveSuccess,
    player_id: PlayerId,
    player_idx: PlayerIdx,
    turn_seconds_remaining: Seconds,
    _is_timeout: bool,
) -> Vec<ServerEvent> {
    use Action::*;
    let mut events = Vec::new();

    match action {
        Draw => {
            if let Some(event) = opponent_update(players, state, player_idx, turn_seconds_remaining) {
                events.push(event);
            }
        }
        OpenScale { .. } => {
            if let MoveSuccess::ScaleOpened { scale_id, placed_card } = result {
                events.push(ServerEvent::CardPlayedOnScale {
                    player_id,
                    player_idx,
                    card: *placed_card,
                    scale_idx: *scale_id,
                    completed: false,
                    turn_seconds_remaining
                });

                if let Some(event) = opponent_update(players, state, player_idx, turn_seconds_remaining) {
                    events.push(event);
                }
            }
        }
        PlayHand { .. } | PlayPersonal { .. } | PlaySide { .. } => {
            match result {
                MoveSuccess::ScalePlaced {
                    scale_id,
                    completed,
                    placed_card,
                } => {
                    events.push(ServerEvent::CardPlayedOnScale {
                        player_id,
                        player_idx,
                        card: *placed_card,
                        scale_idx: *scale_id,
                        completed: *completed,
                        turn_seconds_remaining
                    });

                    if *completed {
                        events.push(ServerEvent::ScaleCompleted {
                            scale_idx: *scale_id,
                            by_player_id: player_id,
                            by_player_idx: player_idx,
                        });
                    }
                }

                MoveSuccess::ScaleOpened {
                    scale_id,
                    placed_card,
                } => {
                    events.push(ServerEvent::CardPlayedOnScale {
                        player_id,
                        player_idx,
                        card: *placed_card,
                        scale_idx: *scale_id,
                        completed: false,
                        turn_seconds_remaining
                    });
                }
                _ => {}
            }

            if matches!(action, PlayPersonal { .. }) {
                if let Some(board) = state.player(player_idx) {
                    events.push(ServerEvent::PersonalPileUpdated {
                        player_id,
                        player_idx,
                        personal_view: personal_pile_view(board),
                    });
                }
            }

            if let Some(event) = opponent_update(players, state, player_idx, turn_seconds_remaining) {
                events.push(event);
            }
        }
        MoveToSide { stack_idx, .. } => {
            let card = state
                .player(player_idx)
                .and_then(|p| p.side.get(stack_idx.as_usize()).and_then(|s| s.last().cloned()));
            if let Some(card) = card {
                events.push(ServerEvent::CardPlacedOnSide {
                    player_id,
                    player_idx,
                    card,
                    stack_idx: *stack_idx,
                    turn_seconds_remaining
                });
            }

            if let Some(event) = opponent_update(players, state, player_idx, turn_seconds_remaining) {
                events.push(event);
            }
        }
        MovePersonalToSide { stack_idx } => {
            if matches!(result, MoveSuccess::Success) {
                let card = state
                    .player(player_idx)
                    .and_then(|p| p.side.get(stack_idx.0).and_then(|s| s.last().cloned()));
                if let Some(card) = card {
                    events.push(ServerEvent::CardPlacedOnSide {
                        player_id,
                        player_idx,
                        card,
                        stack_idx: *stack_idx,
                        turn_seconds_remaining
                    });
                }

                if let Some(board) = state.player(player_idx) {

                    events.push(ServerEvent::PersonalPileUpdated {
                        player_id,
                        player_idx,
                        personal_view: personal_pile_view(board),
                    });
                }

                if let Some(event) = opponent_update(players, state, player_idx, turn_seconds_remaining) {
                    events.push(event);
                }
            }
        }
    }

    if let MoveSuccess::GameWon { winner_idx } = result {
        events.push(ServerEvent::GameOver {
            winner_id: player_id,
            winner_idx: *winner_idx,
            reason: "All cards cleared".into(),
        });
    }

    debug!(
        "Generated {} events for player {:?} action {:?}",
        events.len(),
        player_idx,
        action
    );
    events
}

fn opponent_update(
    players: &HashMap<PlayerId, PlayerInfo>,
    state: &GameState,
    acting_player_idx: PlayerIdx,
    turn_seconds_remaining: Seconds
) -> Option<ServerEvent> {
    let acting_board = state.player(acting_player_idx)?;

    let opponent_id = players
        .iter()
        .find(|(_, info)| info.player_idx != acting_player_idx)
        .map(|(id, _)| *id)?;

    Some(ServerEvent::OpponentUpdate {
        player_id: opponent_id,
        player_idx: acting_player_idx,
        personal_count: acting_board.personal.len(),
        hand: acting_board.hand.iter().map(|card| {card.dummy_card()}).collect(),
        personal_top: acting_board.personal_top().cloned(),
        side: acting_board.side.clone(),
        turn_seconds_remaining
    })
}

pub fn start_timer(
    player_id: PlayerId,
    seconds: Seconds,
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
            _ = sleep(Duration::from_secs(seconds.0)) => {
                let _ = tx.send(RoomCommand::TurnTimeout { player_id });
            }
            _ = token.cancelled() => {}
        }
    });
}
use crate::game::{result::PlayResult, room::ServerEvent, state::GamePhase, timer::start_turn_timer};

pub async fn handle_action(
    text:  &str,
    pid:   &str,
    state: &crate::game::room::SharedRoom,
    tx:    &tokio::sync::broadcast::Sender<ServerEvent>,
) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(text) else { return };
    let action  = val.get("action").and_then(|v| v.as_str()).unwrap_or("");

    let mut gs = state.lock().await;

    if gs.phase != GamePhase::Playing { return; }
    if gs.current_player_id() != Some(pid) { return; } // not your turn

    let result = match action {
        // ── Free actions (can do multiple before ending turn) ─────────────
        "play_hand_to_scale" => {
            let index = val.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            gs.play_hand_to_scale(index)
        }
        "play_personal_to_scale" => {
            gs.play_personal_to_scale()
        }
        "play_personal_to_side" => {
            let index = val.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            gs.play_personal_to_side(index)
        }
        "play_side_to_scale" => {
            let stack = val.get("stack").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            gs.play_side_to_scale(stack)
        }

        // ── End-turn actions (mandatory to end the turn) ──────────────────
        "withdraw_to_side" => {
            let index = val.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let stack = val.get("stack").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            gs.withdraw_hand_to_side(index, stack)
        }

        // ── Pass (end turn without playing) ──────────────────────────────
        "pass" => {
            gs.advance_turn();
            PlayResult::TurnEnded
        }

        _ => return,
    };

    // Dispatch result
    match result {
        PlayResult::Won => {
            let winner = gs.winner()
                .map(|p| p.username.clone())
                .unwrap_or_default();
            gs.phase = crate::game::state::GamePhase::Finished;
            drop(gs);
            let _ = tx.send(ServerEvent::GameOver {
                winner,
                reason: "personal stack emptied".to_string(),
            });
        }

        PlayResult::TurnEnded => {
            let next = gs.current_player_id().map(|s| s.to_string()).unwrap_or_default();
            let secs = gs.turn_seconds;
            let turn = gs.current_turn;
            drop(gs);

            // Draw a card for the next player automatically
            {
                let mut gs = state.lock().await;
                if let Some(card) = gs.draw_for_current() {
                    let _ = tx.send(ServerEvent::CardDrawn {
                        player_id: next.clone(),
                        card,
                    });
                }
            }

            let _ = tx.send(ServerEvent::TurnChanged {
                current_player_id: next,
                seconds: secs,
            });
            start_turn_timer(state.clone(), tx.clone(), secs, turn);
        }

        PlayResult::Ok { scale_id, completed } => {
            let _ = tx.send(ServerEvent::ScaleUpdated {
                scale_id,
                completed,
                player_id: pid.to_string(),
            });
            // Turn not ended yet — player can keep playing
        }

        PlayResult::DoesNotFit => { /* card was returned to hand, no broadcast needed */ }
        PlayResult::InvalidIndex => {},
        PlayResult::NotAllowed => {},
        PlayResult::NotYourTurn => {}
        PlayResult::Moved => {}
    }

    broadcast_state_sync(state, tx).await;
}

pub async fn broadcast_state_sync(
    state: &crate::game::room::SharedRoom,
    tx:    &tokio::sync::broadcast::Sender<ServerEvent>,
) {
    let gs = state.lock().await;
    for p in &gs.players {
        if let Some((board, opp_count, opp_top, opp_side, opp_name)) = gs.state_sync_for(&p.player_id) {
            let _ = tx.send(ServerEvent::StateSync {
                your_board: board,
                opponent_personal_count: opp_count,
                opponent_personal_top: opp_top,
                opponent_side: opp_side,
                opponent_username: opp_name,
                scales: gs.scales.clone(),
            });
        }
    }
}
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{sleep, Duration};

use crate::game::state::{GameState, GamePhase};
use crate::game::room::ServerEvent;

pub fn start_turn_timer(
    state:         Arc<Mutex<GameState>>,
    tx:            broadcast::Sender<ServerEvent>,
    seconds:       u64,
    expected_turn: usize,
) {
    actix_web::rt::spawn(async move {
        sleep(Duration::from_secs(seconds)).await;

        let mut gs = state.lock().await;

        // Game already ended or turn already advanced — do nothing
        if gs.phase != GamePhase::Playing    { return; }
        if gs.current_turn != expected_turn  { return; }

        let timed_out_pid = gs.current_player_id()
            .map(|s| s.to_string())
            .unwrap_or_default();

        // Force advance the turn
        gs.advance_turn();

        let next_pid = gs.current_player_id()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let secs     = gs.turn_seconds;
        let new_turn = gs.current_turn;

        // Draw for the next player
        if let Some(card) = gs.draw_for_current() {
            let _ = tx.send(ServerEvent::CardDrawn {
                player_id: next_pid.clone(),
                card,
            });
        }
        drop(gs);

        let _ = tx.send(ServerEvent::TurnTimeout {
            player_id: timed_out_pid,
        });
        let _ = tx.send(ServerEvent::TurnChanged {
            current_player_id: next_pid,
            seconds:           secs,
        });

        // Re-arm for the next player
        start_turn_timer(state, tx, secs, new_turn);
    });
}
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;

use crate::core::game::state::GameState;
use crate::infrastructure::room::player_info::PlayerInfo;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::server_event::ServerEvent;

pub async fn room_actor(
    room_id: String,
    turn_seconds: u64,
    mut cmd_rx: mpsc::UnboundedReceiver<RoomCommand>,
    cmd_tx: mpsc::UnboundedSender<RoomCommand>,
) {
    let mut state: Option<GameState> = None;
    let mut players: HashMap<usize, PlayerInfo> = HashMap::new();
    let mut current_timer_cancel: Option<CancellationToken> = None;

    let game_start = run_lobby(&room_id, &mut cmd_rx, &mut players).await;

    if game_start.is_none() {
        return; // channel closed
    }

    let starter_id = game_start.unwrap();

    let mut new_state = GameState::new(room_id.clone(), player_ids, seed, personal_count, hand_count);
    new_state.start_game();
    state = Some(new_state);
    broadcast(&players, &ServerEvent::GameStarted { current_player_id: (), turn_seconds });

    let outcome = run_game(
        &room_id,
        turn_seconds,
        &mut cmd_rx,
        &cmd_tx,
        &mut players,
        state.as_mut().unwrap(),
        starter_id,
        &mut current_timer_cancel,
    ).await;

    while let Some(_) = cmd_rx.recv().await {}
}
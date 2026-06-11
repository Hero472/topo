use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;

use crate::core::game::state::{GameState, Seconds};
use crate::core::player::PlayerId;
use crate::infrastructure::room::player_info::PlayerInfo;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::infrastructure::room::room_phase::{LobbyPhase, RoomPhase};

pub async fn room_actor(
    room_id: String,
    turn_seconds: Seconds,
    mut cmd_rx: mpsc::UnboundedReceiver<RoomCommand>,
    cmd_tx: mpsc::UnboundedSender<RoomCommand>,
) {
    let mut players: HashMap<PlayerId, PlayerInfo> = HashMap::new();
    let mut state: Option<GameState> = None;
    let mut timer: Option<CancellationToken> = None;

    let mut phase: Box<dyn RoomPhase + Send> = Box::new(LobbyPhase::new(room_id, turn_seconds));

    while let Some(cmd) = cmd_rx.recv().await {
        if let Some(new_phase) = phase
            .handle_command(cmd, &mut players, &mut state, &mut timer, &cmd_tx)
            .await
        {
            phase = new_phase;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::game::actions::Action;
    use crate::infrastructure::error::ErrorCode;
    use crate::core::player::PlayerId;
    use crate::infrastructure::message::GameMessage;
    use crate::infrastructure::server_event::ServerEvent;
    use tokio::sync::mpsc;
    use tokio::time::{sleep, Duration};
    use uuid::Uuid;

    fn id(val: u128) -> PlayerId {
        PlayerId(Uuid::from_u128(val))
    }

    async fn setup_actor(
        room_id: &str,
        turn_seconds: Seconds,
        player1: PlayerId,
        player2: PlayerId,
    ) -> (
        mpsc::UnboundedSender<RoomCommand>,
        mpsc::UnboundedReceiver<GameMessage>,
        mpsc::UnboundedReceiver<GameMessage>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let actor_tx = cmd_tx.clone();
        tokio::spawn(room_actor(
            room_id.to_string(),
            turn_seconds,
            cmd_rx,
            actor_tx,
        ));

        let (tx1, rx1) = mpsc::unbounded_channel();
        cmd_tx
            .send(RoomCommand::SubscribePlayer {
                player_id: player1,
                sender: tx1,
            })
            .unwrap();
        let (tx2, rx2) = mpsc::unbounded_channel();
        cmd_tx
            .send(RoomCommand::SubscribePlayer {
                player_id: player2,
                sender: tx2,
            })
            .unwrap();

        tokio::task::yield_now().await;

        (cmd_tx, rx1, rx2)
    }

    async fn drain<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Vec<T> {
        let mut msgs = vec![];
        while let Ok(msg) = rx.try_recv() {
            msgs.push(msg);
        }
        msgs
    }

    #[tokio::test]
    async fn two_players_join_and_game_starts() {
        let player1 = id(1);
        let player2 = id(2);
        let (cmd_tx, mut rx1, mut rx2) =
            setup_actor("test_room", Seconds(60), player1, player2).await;

        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player1,
                username: "Alice".into(),
            })
            .unwrap();
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player2,
                username: "Bob".into(),
            })
            .unwrap();

        sleep(Duration::from_millis(10)).await;

        let msgs1 = drain(&mut rx1).await;
        let msgs2 = drain(&mut rx2).await;

        println!("--- Player 1 received {} messages ---", msgs1.len());
        for (i, msg) in msgs1.iter().enumerate() {
            println!("  msg1[{}]: {:?}", i, msg.event);
        }
        println!("--- Player 2 received {} messages ---", msgs2.len());
        for (i, msg) in msgs2.iter().enumerate() {
            println!("  msg2[{}]: {:?}", i, msg.event);
        }

        assert_eq!(msgs1.len(), 4);
        assert_eq!(msgs2.len(), 4);

        // Check that each received a FullState and GameStarted
        let has_full_state = |msgs: &[GameMessage]| msgs.iter().any(|m| matches!(m.event, ServerEvent::FullState { .. }));
        let has_game_started = |msgs: &[GameMessage]| msgs.iter().any(|m| matches!(m.event, ServerEvent::GameStarted { .. }));
        assert!(has_full_state(&msgs1));
        assert!(has_game_started(&msgs1));
        assert!(has_full_state(&msgs2));
        assert!(has_game_started(&msgs2));
    }

    #[tokio::test]
    async fn valid_draw_action_sends_card_drawn() {
        let player1 = id(1);
        let player2 = id(2);
        let (cmd_tx, mut rx1, _rx2) =
            setup_actor("test_room", Seconds(60), player1, player2).await;

        // Both join to start game
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player1,
                username: "Alice".into(),
            })
            .unwrap();
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player2,
                username: "Bob".into(),
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;
        // drain previous events
        drain(&mut rx1).await;

        // Player1 draws (assuming they are the first player)
        cmd_tx
            .send(RoomCommand::PlayerAction {
                player_id: player1,
                action: Action::Draw,
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;

        let msgs = drain(&mut rx1).await;
        // Player1 should receive a CardDrawn event (private)
        assert!(msgs.iter().any(|m| matches!(m.event, ServerEvent::CardDrawn { .. })));
    }

    #[tokio::test]
    async fn action_when_not_your_turn_returns_error() {
        let player1 = id(1);
        let player2 = id(2);
        let (cmd_tx, mut rx1, mut rx2) =
            setup_actor("test_room", Seconds(60), player1, player2).await;

        // Join both players
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player1,
                username: "Alice".into(),
            })
            .unwrap();
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player2,
                username: "Bob".into(),
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;
        drain(&mut rx1).await;
        drain(&mut rx2).await;

        // Player2 tries to draw (but it's Player1's turn)
        cmd_tx
            .send(RoomCommand::PlayerAction {
                player_id: player2,
                action: Action::Draw,
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;

        let msgs2 = drain(&mut rx2).await;
        assert!(msgs2.iter().any(|m| matches!(m.event, ServerEvent::Error { code: ErrorCode::NotYourTurn, .. })));
    }

    #[tokio::test]
    async fn timeout_forces_move_and_ends_turn() {
        let player1 = id(1);
        let player2 = id(2);
        let (cmd_tx, mut rx1, mut rx2) =
            setup_actor("test_room", Seconds(1), player1, player2).await; // 1-second turns

        // Join and start game
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player1,
                username: "Alice".into(),
            })
            .unwrap();
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player2,
                username: "Bob".into(),
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;
        drain(&mut rx1).await;
        drain(&mut rx2).await;

        // Wait for the timeout to fire (turn_seconds = 1)
        sleep(Duration::from_secs(2)).await;

        let msgs1 = drain(&mut rx1).await;
        let msgs2 = drain(&mut rx2).await;

        // Both should see a TurnEnded event with timed_out_player_idx set
        let has_timeout_turn_end = |msgs: &[GameMessage]| {
            msgs.iter().any(|m| matches!(&m.event,
                ServerEvent::TurnEnded { timed_out_player_idx: Some(_), .. }))
        };
        assert!(has_timeout_turn_end(&msgs1));
        assert!(has_timeout_turn_end(&msgs2));
    }

    #[tokio::test]
    async fn player_disconnect_ends_game() {
        let player1 = id(1);
        let player2 = id(2);
        let (cmd_tx, mut rx1, mut rx2) =
            setup_actor("test_room", Seconds(60), player1, player2).await;

        // Join both players
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player1,
                username: "Alice".into(),
            })
            .unwrap();
        cmd_tx
            .send(RoomCommand::PlayerJoined {
                player_id: player2,
                username: "Bob".into(),
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;
        drain(&mut rx1).await;
        drain(&mut rx2).await;

        // Player2 disconnects
        cmd_tx
            .send(RoomCommand::PlayerLeft {
                player_id: player2,
            })
            .unwrap();
        sleep(Duration::from_millis(10)).await;

        let msgs1 = drain(&mut rx1).await;
        let msgs2 = drain(&mut rx2).await; // may be empty if channel closed

        // Player1 should see GameOver
        assert!(msgs1.iter().any(|m| matches!(m.event, ServerEvent::GameOver { .. })));
        // Player2 may or may not get the GameOver depending on timing,
        // but the room should be removed.
    }
}
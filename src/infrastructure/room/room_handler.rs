use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::{
    core::game::actions::Action,
    infrastructure::{
        message::GameMessage,
        room::room_actor::room_actor
    },
};

pub struct RoomHandle {
    cmd_tx: mpsc::UnboundedSender<RoomCommand>
}

impl RoomHandle {
    pub fn new_arc(room_id: String, turn_seconds: u64) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let actor_tx = cmd_tx.clone();
        let handle = Arc::new(Self { cmd_tx: cmd_tx.clone() });
        tokio::spawn(room_actor(room_id, turn_seconds, cmd_rx, actor_tx));
        handle
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_with_tx(cmd_tx: mpsc::UnboundedSender<RoomCommand>) -> Self {
        Self { cmd_tx }
    }

    pub fn subscribe_player(&self, player_id: usize) -> mpsc::UnboundedReceiver<GameMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = self.cmd_tx.send(RoomCommand::SubscribePlayer { player_id, sender: tx });
        rx
    }

    pub fn add_player(&self, player_id: usize, username: String) -> Result<(), SendError<RoomCommand>> {
        self.cmd_tx.send(RoomCommand::PlayerJoined { player_id, username })
    }


    pub fn remove_player(&self, player_id: usize) -> Result<(), SendError<RoomCommand>> {
        self.cmd_tx.send(RoomCommand::PlayerLeft { player_id })
    }

    pub fn apply_action(&self, player_id: usize, action: Action) -> Result<(), SendError<RoomCommand>> {
        self.cmd_tx.send(RoomCommand::PlayerAction { player_id, action })
    }
}

#[cfg(test)]
mod unit_tests {
    use crate::{core::game::card::Card, infrastructure::server_event::ServerEvent};

use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn add_player_sends_player_joined_command() {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let handle = RoomHandle::new_with_tx(cmd_tx);

        handle.add_player(1, "Alice".to_string()).unwrap();

        match cmd_rx.recv().await.unwrap() {
            RoomCommand::PlayerJoined { player_id, username } => {
                assert_eq!(player_id, 1);
                assert_eq!(username, "Alice");
            }
            other => panic!("Expected PlayerJoined, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn remove_player_sends_player_left_command() {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let handle = RoomHandle::new_with_tx(cmd_tx);

        handle.remove_player(42).unwrap();

        match cmd_rx.recv().await.unwrap() {
            RoomCommand::PlayerLeft { player_id } => {
                assert_eq!(player_id, 42);
            }
            other => panic!("Expected PlayerLeft, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn apply_action_sends_player_action_command() {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let handle = RoomHandle::new_with_tx(cmd_tx);

        let action = Action::Draw;
        handle.apply_action(3, action.clone()).unwrap();

        match cmd_rx.recv().await.unwrap() {
            RoomCommand::PlayerAction { player_id, action: received_action } => {
                assert_eq!(player_id, 3);
                assert_eq!(received_action, action);
            }
            other => panic!("Expected PlayerAction, got {:?}", other),
        }
    }

    #[tokio::test]
async fn subscribe_player_sends_subscribe_command_and_returns_receiver() {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
    let handle = RoomHandle::new_with_tx(cmd_tx);

    let mut _player_rx = handle.subscribe_player(99);

    match cmd_rx.recv().await.unwrap() {
        RoomCommand::SubscribePlayer { player_id, sender } => {
            assert_eq!(player_id, 99);
            // Just send a dummy message to ensure the sender works
            let msg = GameMessage {
                to: None,
                event: ServerEvent::PlayerJoined {
                    player_id: 99,
                    username: "Joe".to_string(),
                },
            };
            assert!(sender.send(msg).is_ok());
        }
        other => panic!("Expected SubscribePlayer, got {:?}", other),
    }
}
}
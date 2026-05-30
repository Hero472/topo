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
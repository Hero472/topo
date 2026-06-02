use tokio::sync::mpsc;

use crate::{core::game::actions::Action, infrastructure::message::GameMessage};

#[derive(Debug)]
pub enum RoomCommand {
    /// Register a new message sender for a player and return the receiver to the caller.
    SubscribePlayer {
        player_id: usize,
        sender: mpsc::UnboundedSender<GameMessage>,
    },

    /// A player joined the room (or entered the match lobby).
    PlayerJoined {
        player_id: usize,
        username: String,
    },

    /// A player disconnected or left voluntarily.
    PlayerLeft {
        player_id: usize,
    },

    TurnTimeout {
        player_id: usize,
    },

    /// A player submitted a game action (draw, play card, etc.).
    PlayerAction {
        player_id: usize,
        action: Action,
    },
}
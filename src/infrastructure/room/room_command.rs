use tokio::sync::mpsc;

use crate::{core::{game::actions::Action, player::PlayerId}, infrastructure::message::GameMessage};

#[derive(Debug)]
pub enum RoomCommand {
    /// Register a new message sender for a player and return the receiver to the caller.
    SubscribePlayer {
        player_id: PlayerId,
        sender: mpsc::UnboundedSender<GameMessage>,
    },

    /// A player joined the room (or entered the match lobby).
    PlayerJoined {
        player_id: PlayerId,
        username: String,
    },

    /// A player disconnected or left voluntarily.
    PlayerLeft {
        player_id: PlayerId,
    },

    TurnTimeout {
        player_id: PlayerId,
    },

    /// A player submitted a game action (draw, play card, etc.).
    PlayerAction {
        player_id: PlayerId,
        action: Action,
    },
}
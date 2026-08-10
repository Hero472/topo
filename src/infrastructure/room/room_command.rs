use tokio::sync::mpsc;

use crate::{core::{game::{actions::Action, state::state_types::Seed}, player::PlayerId}, infrastructure::message::GameMessage};
use tokio::sync::oneshot;

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
        username: String
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

    PlayerReconnected {
        player_id: PlayerId
    },

    IsPlayerKnown {
        player_id: PlayerId,
        reply: oneshot::Sender<bool>,
    },

    UnsubscribePlayer {
        player_id: PlayerId,
    },

    DisconnectTimeout {
        player_id: PlayerId,
    },

    Shutdown,

    SetSeed(Seed)
}
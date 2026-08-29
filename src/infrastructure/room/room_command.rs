use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{core::{game::{actions::Action, state::state_types::Seed}, player::PlayerId}, infrastructure::message::GameMessage};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LobbyAction {
    PlayerReady,
    PlayAgain,
    Leave
}


#[derive(Debug)]
pub enum RoomCommand {
    // ── Connection Lifecycle ──
    
    /// WebSocket opened. Register the sender so the actor can push events to this player.
    SubscribePlayer {
        player_id: PlayerId,
        sender: mpsc::UnboundedSender<GameMessage>,
    },

    /// Pure cleanup: removes the subscription placeholder if a join fails.
    /// Does NOT trigger grace periods or game logic.
    UnsubscribePlayer { player_id: PlayerId },

    /// Player entered the room/lobby.
    PlayerJoined {
        player_id: PlayerId,
        username: String,
    },

    /// Player's WebSocket dropped unexpectedly (network loss, browser crash, etc.).
    /// Starts a grace period timer. Does NOT remove the player yet.
    NetworkDisconnect { player_id: PlayerId },

    /// Player's WebSocket reconnected before the grace period expired.
    /// Cancels the timer and marks them as connected.
    PlayerReconnected { player_id: PlayerId },

    /// Internal: The grace period timer expired. 
    /// Now the player is officially removed from the room.
    DisconnectTimeout { player_id: PlayerId },

    // ── Intentional Player Actions ──

    /// Player intentionally clicked "Ready" in the lobby.
    PlayerReady { player_id: PlayerId },

    /// Player intentionally clicked "Play Again" after game over.
    PlayAgain { player_id: PlayerId },

    /// Player intentionally clicked "Leave Game". 
    /// Bypasses the grace period and removes them immediately.
    PlayerLeft { player_id: PlayerId },

    // ── Gameplay & State ──

    /// Player submitted a game action (draw, play card, etc.).
    PlayerAction {
        player_id: PlayerId,
        action: Action,
    },

    /// Check if a player is known to this room (used during reconnect).
    IsPlayerKnown {
        player_id: PlayerId,
        reply: oneshot::Sender<bool>,
    },

    // ── Game Lifecycle ──
    StartGame,
    GameStartingTick { seconds_remaining: u8 },
    TurnTimeout { player_id: PlayerId },
    SetSeed(Seed),
    Shutdown,
}
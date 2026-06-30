use std::time::Duration;

use async_trait::async_trait;
use crate::infrastructure::{error::ErrorCode, room::utils::send_to, server_event::ServerEvent};

use super::*;

pub struct OverPhase {
    pub room_id: String,
    pub cmd_tx: mpsc::UnboundedSender<RoomCommand>,
    pub shutdown_scheduled: bool,
}

impl OverPhase {
    pub fn new(room_id: String, cmd_tx: mpsc::UnboundedSender<RoomCommand>) -> Self {
        let tx = cmd_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(5)).await;
            let _ = tx.send(RoomCommand::Shutdown);
        });

        Self {
            room_id,
            cmd_tx,
            shutdown_scheduled: true,
        }
    }
}

#[async_trait]
impl RoomPhase for OverPhase {
    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<PlayerId, PlayerInfo>,
        _state: &mut Option<GameState>,
        _timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase + Send>> {
        // Schedule shutdown on the very first command (or immediately)
        if !self.shutdown_scheduled {
            self.shutdown_scheduled = true;
            let tx = cmd_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let _ = tx.send(RoomCommand::Shutdown);
            });
        }

        match cmd {
            RoomCommand::PlayerAction { player_id, .. } => {
                send_to(
                    players,
                    player_id,
                    ServerEvent::Error {
                        code: ErrorCode::GameOver,
                        message: Some("Game is already over".into()),
                        details: None,
                    },
                );
            }
            RoomCommand::IsPlayerKnown { player_id, reply } => {
                let known = players.contains_key(&player_id);
                let _ = reply.send(known);
            }
            RoomCommand::UnsubscribePlayer { player_id } => {
                players.remove(&player_id);
            }
            // All other commands are silently ignored
            _ => {}
        }
        None // Stay in OverPhase
    }
}
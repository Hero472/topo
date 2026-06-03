use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::collections::HashMap;

use crate::infrastructure::room::player_info::PlayerInfo;
use crate::infrastructure::room::room_command::RoomCommand;
use crate::core::game::state::GameState;

pub mod lobby;
pub mod playing;
pub mod over;

pub use lobby::LobbyPhase;
pub use playing::PlayingPhase;
pub use over::OverPhase;

#[async_trait]
pub trait RoomPhase {
    async fn handle_command(
        &mut self,
        cmd: RoomCommand,
        players: &mut HashMap<usize, PlayerInfo>,
        state: &mut Option<GameState>,
        timer: &mut Option<CancellationToken>,
        cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase>>;
}
use async_trait::async_trait;
use super::*;

pub struct OverPhase {
    pub room_id: String,
}

#[async_trait]
impl RoomPhase for OverPhase {
    async fn handle_command(
        &mut self,
        _cmd: RoomCommand,
        _players: &mut HashMap<PlayerId, PlayerInfo>,
        _state: &mut Option<GameState>,
        _timer: &mut Option<CancellationToken>,
        _cmd_tx: &mpsc::UnboundedSender<RoomCommand>,
    ) -> Option<Box<dyn RoomPhase + Send>> {
        None
    }
}
use async_trait::async_trait;
use super::*;

pub struct OverPhase {
    pub room_id: String,
}

#[async_trait]
impl RoomPhase for OverPhase {
    
}
use serde::Serialize;
use crate::infrastructure::server_event::ServerEvent;

#[derive(Debug, Clone, Serialize)]
pub struct GameMessage {
    /// If Some(id), only the player with that id should process this event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<usize>,
    #[serde(flatten)]
    pub event: ServerEvent,
}
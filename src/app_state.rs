use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::infrastructure::room::room_handler::RoomHandle;

pub struct AppState {
    pub rooms: Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>,
    pub room_shutdown_tx: mpsc::UnboundedSender<String>,
}
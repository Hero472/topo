use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::infrastructure::room::room_handler::RoomHandle;

pub struct AppState {
    pub rooms: Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>,
}
use std::{collections::HashMap, sync::{Arc, Mutex}};

use crate::{core::game::state::GameState, infrastructure::room::room_handler::RoomHandle};

pub type SharedRoom    = Arc<Mutex<GameState>>;
pub type RoomRegistry  = Arc<Mutex<HashMap<String, Arc<RoomHandle>>>>;

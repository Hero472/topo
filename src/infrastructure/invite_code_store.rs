use std::collections::HashMap;
use crate::{core::game_id::GameId, utils::invite_code::InviteCode};

pub struct InviteCodeStore {
    code_to_game: HashMap<InviteCode, GameId>,
    game_to_code: HashMap<GameId, InviteCode>,
}

impl InviteCodeStore {
    pub fn new() -> Self {
        Self {
            code_to_game: HashMap::new(),
            game_to_code: HashMap::new(),
        }
    }

    /// Generate a unique code for a game. Retries on collision.
    pub fn create_code(&mut self, game_id: GameId) -> InviteCode {
        // If this game already has a code, return it
        if let Some(existing) = self.game_to_code.get(&game_id) {
            return existing.clone();
        }

        loop {
            let code = InviteCode::generate();
            if !self.code_to_game.contains_key(&code) {
                self.code_to_game.insert(code.clone(), game_id.clone());
                self.game_to_code.insert(game_id, code.clone());
                return code;
            }
            // collision → just loop and try again
        }
    }

    /// Resolve an invite code back to a GameId
    pub fn resolve(&self, code: &InviteCode) -> Option<&GameId> {
        self.code_to_game.get(code)
    }

    /// Clean up when a game ends
    pub fn remove_game(&mut self, game_id: &GameId) {
        if let Some(code) = self.game_to_code.remove(game_id) {
            self.code_to_game.remove(&code);
        }
    }
}
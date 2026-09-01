use std::sync::RwLock;

use super::model::PlayerSnapshot;

#[derive(Default)]
pub struct PlayerStore {
    state: RwLock<Option<PlayerSnapshot>>,
}

impl PlayerStore {
    pub fn update(
        &self,
        next: PlayerSnapshot,
    ) -> Result<Option<PlayerSnapshot>, String> {
        let mut state = self
            .state
            .write()
            .map_err(|_| "player state lock is poisoned".to_string())?;

        let previous = state.clone();

        *state = Some(next);

        Ok(previous)
    }

    #[allow(dead_code)]
    pub fn snapshot(&self) -> Result<Option<PlayerSnapshot>, String> {
        let state = self
            .state
            .read()
            .map_err(|_| "player state lock is poisoned".to_string())?;

        Ok(state.clone())
    }
}
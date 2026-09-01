use std::sync::RwLock;

use super::model::PlayerSnapshot;

#[derive(Default)]
pub struct PlayerStore {
    state: RwLock<Option<PlayerSnapshot>>,
}

impl PlayerStore {
    pub fn update(
        &self,
        mut next: PlayerSnapshot,
    ) -> Result<(Option<PlayerSnapshot>, PlayerSnapshot), String> {
        let mut state = self
            .state
            .write()
            .map_err(|_| "player state lock is poisoned".to_string())?;

        let previous = state.clone();

        // YouTube Music may temporarily clear MediaSession metadata
        // while navigating or changing tracks. Preserve the last
        // known metadata during these transient gaps.
        if next.metadata.is_none() {
            if let Some(previous_state) = &previous {
                next.metadata = previous_state.metadata.clone();
            }
        }

        *state = Some(next.clone());

        Ok((previous, next))
    }

    pub fn snapshot(
        &self,
    ) -> Result<Option<PlayerSnapshot>, String> {
        let state = self
            .state
            .read()
            .map_err(|_| "player state lock is poisoned".to_string())?;

        Ok(state.clone())
    }
}
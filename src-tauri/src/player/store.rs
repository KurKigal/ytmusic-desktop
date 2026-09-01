use tokio::sync::watch;

use super::model::PlayerSnapshot;

/// Stores the latest player state and broadcasts updates
/// to native integrations.
///
/// A watch channel is used because consumers generally care
/// about the latest state rather than every intermediate update.
pub struct PlayerStore {
    updates: watch::Sender<Option<PlayerSnapshot>>,
}

impl Default for PlayerStore {
    fn default() -> Self {
        let (updates, _) = watch::channel(None);

        Self { updates }
    }
}

impl PlayerStore {
    /// Normalizes and stores a new player snapshot.
    ///
    /// YouTube Music can temporarily clear MediaSession metadata
    /// during navigation or track transitions. In that case, the
    /// last known metadata is preserved.
    pub fn update(&self, mut next: PlayerSnapshot) -> Result<PlayerSnapshot, String> {
        let previous = self.updates.borrow().clone();

        if next.metadata.is_none() {
            if let Some(previous_state) = previous {
                next.metadata = previous_state.metadata;
            }
        }

        self.updates.send_replace(Some(next.clone()));

        Ok(next)
    }

    /// Creates a new subscriber that receives the latest
    /// player state whenever it changes.
    pub fn subscribe(&self) -> watch::Receiver<Option<PlayerSnapshot>> {
        self.updates.subscribe()
    }
}

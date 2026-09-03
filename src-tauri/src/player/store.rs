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
    /// YouTube Music can temporarily clear MediaSession metadata.
    /// The last known metadata is preserved only when both snapshots
    /// identify the same playback timeline.
    pub fn update(&self, mut next: PlayerSnapshot) -> Result<PlayerSnapshot, String> {
        let previous = self.updates.borrow().clone();

        if next.metadata.is_none() {
            if let Some(previous_state) = previous {
                let same_timeline = match (&previous_state.timeline_id, &next.timeline_id) {
                    (Some(previous_id), Some(next_id)) => previous_id == next_id,
                    _ => false,
                };

                if same_timeline {
                    next.metadata = previous_state.metadata;
                    next.metadata_id = previous_state.metadata_id;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{PlaybackStatus, TrackMetadata};

    fn metadata(title: &str) -> TrackMetadata {
        TrackMetadata {
            title: Some(title.to_string()),
            artist: Some("Artist".to_string()),
            album: None,
            artwork: Vec::new(),
        }
    }

    fn snapshot(
        metadata: Option<TrackMetadata>,
        media_id: Option<&str>,
        timeline_id: Option<&str>,
    ) -> PlayerSnapshot {
        let metadata_id = metadata.as_ref().map(|_| "10".to_string());

        PlayerSnapshot {
            metadata,
            metadata_id,
            playback: PlaybackStatus::Playing,
            position: 0.0,
            duration: 120.0,
            paused: false,
            media_type: Some("VIDEO".to_string()),
            media_id: media_id.map(str::to_string),
            timeline_id: timeline_id.map(str::to_string),
            timing_metadata_id: Some("10".to_string()),
            timing_observation_id: Some("11".to_string()),
            playback_rate: 1.0,
        }
    }

    #[test]
    fn preserves_metadata_for_same_playback_timeline() {
        let store = PlayerStore::default();
        let expected = metadata("Track A");

        store
            .update(snapshot(Some(expected.clone()), Some("1"), Some("100")))
            .expect("initial snapshot should be stored");

        let updated = store
            .update(snapshot(None, Some("2"), Some("100")))
            .expect("metadata gap should be stored");

        assert_eq!(updated.metadata, Some(expected));
        assert_eq!(updated.metadata_id.as_deref(), Some("10"));
    }

    #[test]
    fn does_not_splice_metadata_across_timeline_transition() {
        let store = PlayerStore::default();

        store
            .update(snapshot(Some(metadata("Track A")), Some("1"), Some("100")))
            .expect("initial snapshot should be stored");

        let updated = store
            .update(snapshot(None, Some("1"), Some("200")))
            .expect("transition snapshot should be stored");

        assert_eq!(updated.metadata, None);
        assert_eq!(updated.metadata_id, None);
    }

    #[test]
    fn does_not_preserve_metadata_without_timeline_identity() {
        for (previous_id, next_id) in [(None, None), (Some("1"), None), (None, Some("1"))] {
            let store = PlayerStore::default();

            store
                .update(snapshot(Some(metadata("Track A")), Some("1"), previous_id))
                .expect("initial snapshot should be stored");

            let updated = store
                .update(snapshot(None, Some("1"), next_id))
                .expect("metadata gap should be stored");

            assert_eq!(updated.metadata, None);
        }
    }
}

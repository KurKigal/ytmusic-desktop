use serde::{Deserialize, Serialize};

use crate::player::{PlaybackStatus, PlayerCommand, PlayerSnapshot};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MiniPlayerPlayback {
    Playing,
    Paused,
    Inactive,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MiniPlayerState {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub artwork_url: Option<String>,
    pub playback: MiniPlayerPlayback,
    pub position: f64,
    pub duration: f64,
}

impl From<&PlayerSnapshot> for MiniPlayerState {
    fn from(snapshot: &PlayerSnapshot) -> Self {
        let (title, artist, artwork_url) = snapshot
            .metadata
            .as_ref()
            .map(|metadata| {
                (
                    metadata.title.clone(),
                    metadata.artist.clone(),
                    metadata
                        .artwork
                        .iter()
                        .rev()
                        .find_map(|artwork| allowed_artwork_url(&artwork.src)),
                )
            })
            .unwrap_or((None, None, None));

        Self {
            title,
            artist,
            artwork_url,
            playback: MiniPlayerPlayback::from(&snapshot.playback),
            position: if snapshot.duration > 0.0 {
                snapshot.position.min(snapshot.duration)
            } else {
                snapshot.position
            },
            duration: snapshot.duration,
        }
    }
}

impl From<&PlaybackStatus> for MiniPlayerPlayback {
    fn from(playback: &PlaybackStatus) -> Self {
        match playback {
            PlaybackStatus::Playing => Self::Playing,
            PlaybackStatus::Paused => Self::Paused,
            PlaybackStatus::Inactive => Self::Inactive,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MiniPlayerCommand {
    TogglePlayback,
    Previous,
    Next,
    Seek { position: f64 },
}

impl MiniPlayerCommand {
    pub(crate) fn into_player_command(self) -> Result<PlayerCommand, String> {
        match self {
            Self::TogglePlayback => Ok(PlayerCommand::TogglePlayback),
            Self::Previous => Ok(PlayerCommand::Previous),
            Self::Next => Ok(PlayerCommand::Next),
            Self::Seek { position } if position.is_finite() && position >= 0.0 => {
                Ok(PlayerCommand::Seek { position })
            }
            Self::Seek { .. } => Err("invalid seek position".to_string()),
        }
    }
}

fn allowed_artwork_url(value: &str) -> Option<String> {
    let url = tauri::Url::parse(value.trim()).ok()?;

    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }

    Some(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{Artwork, TrackMetadata};

    fn snapshot(playback: PlaybackStatus) -> PlayerSnapshot {
        let paused = !matches!(playback, PlaybackStatus::Playing);

        PlayerSnapshot {
            metadata: Some(TrackMetadata {
                title: Some("Track title".to_string()),
                artist: Some("Track artist".to_string()),
                album: Some("Internal album".to_string()),
                artwork: vec![
                    Artwork {
                        src: "data:image/png;base64,AAAA".to_string(),
                        sizes: None,
                        r#type: None,
                    },
                    Artwork {
                        src: "https://example.com/cover.jpg".to_string(),
                        sizes: Some("512x512".to_string()),
                        r#type: Some("image/jpeg".to_string()),
                    },
                ],
            }),
            metadata_id: Some("10".to_string()),
            playback,
            position: 24.5,
            duration: 180.0,
            paused,
            media_type: Some("VIDEO".to_string()),
            media_id: Some("20".to_string()),
            timeline_id: Some("30".to_string()),
            timing_metadata_id: Some("40".to_string()),
            timing_observation_id: Some("50".to_string()),
            playback_rate: 1.0,
        }
    }

    #[test]
    fn maps_only_the_public_mini_player_fields() {
        let state = MiniPlayerState::from(&snapshot(PlaybackStatus::Playing));
        let json = serde_json::to_value(state).expect("mini player state should serialize");

        assert_eq!(json["title"], "Track title");
        assert_eq!(json["artist"], "Track artist");
        assert_eq!(json["artworkUrl"], "https://example.com/cover.jpg");
        assert_eq!(json["playback"], "playing");
        assert_eq!(json["position"], 24.5);
        assert_eq!(json["duration"], 180.0);
        assert_eq!(json.as_object().map(serde_json::Map::len), Some(6));
    }

    #[test]
    fn filters_non_http_artwork_urls() {
        let mut snapshot = snapshot(PlaybackStatus::Paused);
        let metadata = snapshot.metadata.as_mut().expect("metadata should exist");
        metadata.artwork = vec![
            Artwork {
                src: "https://example.com/older-cover.jpg".to_string(),
                sizes: None,
                r#type: None,
            },
            Artwork {
                src: "javascript:alert(1)".to_string(),
                sizes: None,
                r#type: None,
            },
            Artwork {
                src: "file:///tmp/cover.jpg".to_string(),
                sizes: None,
                r#type: None,
            },
            Artwork {
                src: "http://example.com/cover.jpg".to_string(),
                sizes: None,
                r#type: None,
            },
        ];

        let state = MiniPlayerState::from(&snapshot);

        assert_eq!(
            state.artwork_url.as_deref(),
            Some("http://example.com/cover.jpg")
        );
    }

    #[test]
    fn prefers_the_last_valid_artwork_entry() {
        let state = MiniPlayerState::from(&snapshot(PlaybackStatus::Playing));

        assert_eq!(
            state.artwork_url.as_deref(),
            Some("https://example.com/cover.jpg")
        );
    }

    #[test]
    fn clamps_position_to_a_known_duration() {
        let mut known_duration = snapshot(PlaybackStatus::Playing);
        known_duration.position = 240.0;
        known_duration.duration = 180.0;

        assert_eq!(MiniPlayerState::from(&known_duration).position, 180.0);

        known_duration.duration = 0.0;

        assert_eq!(MiniPlayerState::from(&known_duration).position, 240.0);
    }

    #[test]
    fn maps_all_playback_states() {
        for (playback, expected) in [
            (PlaybackStatus::Playing, MiniPlayerPlayback::Playing),
            (PlaybackStatus::Paused, MiniPlayerPlayback::Paused),
            (PlaybackStatus::Inactive, MiniPlayerPlayback::Inactive),
        ] {
            assert_eq!(
                MiniPlayerState::from(&snapshot(playback)).playback,
                expected
            );
        }
    }

    #[test]
    fn maps_commands_to_the_existing_player_api() {
        assert!(matches!(
            MiniPlayerCommand::TogglePlayback.into_player_command(),
            Ok(PlayerCommand::TogglePlayback)
        ));
        assert!(matches!(
            MiniPlayerCommand::Previous.into_player_command(),
            Ok(PlayerCommand::Previous)
        ));
        assert!(matches!(
            MiniPlayerCommand::Next.into_player_command(),
            Ok(PlayerCommand::Next)
        ));
        assert!(matches!(
            MiniPlayerCommand::Seek { position: 42.0 }.into_player_command(),
            Ok(PlayerCommand::Seek { position: 42.0 })
        ));
    }

    #[test]
    fn uses_the_narrow_tagged_command_contract() {
        let toggle = serde_json::to_value(MiniPlayerCommand::TogglePlayback)
            .expect("toggle command should serialize");
        let seek = serde_json::to_value(MiniPlayerCommand::Seek { position: 91.25 })
            .expect("seek command should serialize");

        assert_eq!(toggle, serde_json::json!({ "type": "togglePlayback" }));
        assert_eq!(
            seek,
            serde_json::json!({ "type": "seek", "position": 91.25 })
        );
    }

    #[test]
    fn rejects_invalid_seek_positions() {
        for position in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                MiniPlayerCommand::Seek { position }
                    .into_player_command()
                    .expect_err("invalid seek position should be rejected"),
                "invalid seek position"
            );
        }
    }
}

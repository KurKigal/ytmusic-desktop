use serde::{Deserialize, Serialize};
use std::time::Duration;

const MAX_OPAQUE_ID_LENGTH: usize = 64;
const MAX_PLAYBACK_RATE: f64 = 16.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artwork {
    pub src: String,
    pub sizes: Option<String>,
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub artwork: Vec<Artwork>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackStatus {
    Playing,
    Paused,

    #[serde(rename = "none")]
    Inactive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub metadata: Option<TrackMetadata>,
    #[serde(default)]
    pub metadata_id: Option<String>,
    pub playback: PlaybackStatus,
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub media_type: Option<String>,
    #[serde(default)]
    pub media_id: Option<String>,
    #[serde(default)]
    pub timeline_id: Option<String>,
    #[serde(default)]
    pub timing_metadata_id: Option<String>,
    #[serde(default)]
    pub timing_observation_id: Option<String>,
    #[serde(default = "default_playback_rate")]
    pub playback_rate: f64,
}

impl PlayerSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_media_time(self.position) {
            return Err("invalid playback position".into());
        }

        if !is_valid_media_time(self.duration) {
            return Err("invalid media duration".into());
        }

        let expected_paused = !matches!(self.playback, PlaybackStatus::Playing);

        if self.paused != expected_paused {
            return Err("playback and paused state are inconsistent".into());
        }

        validate_opaque_id("metadata", self.metadata_id.as_deref())?;
        validate_opaque_id("media", self.media_id.as_deref())?;
        validate_opaque_id("timeline", self.timeline_id.as_deref())?;
        validate_opaque_id("timing metadata", self.timing_metadata_id.as_deref())?;
        validate_opaque_id("timing observation", self.timing_observation_id.as_deref())?;

        if !self.playback_rate.is_finite()
            || self.playback_rate <= 0.0
            || self.playback_rate > MAX_PLAYBACK_RATE
        {
            return Err("invalid playback rate".into());
        }

        if let Some(metadata) = &self.metadata {
            validate_optional_string("title", &metadata.title, 512)?;
            validate_optional_string("artist", &metadata.artist, 512)?;
            validate_optional_string("album", &metadata.album, 512)?;

            if metadata.artwork.len() > 8 {
                return Err("too many artwork entries".into());
            }

            for artwork in &metadata.artwork {
                if artwork.src.len() > 4096 {
                    return Err("artwork URL is too long".into());
                }
            }
        }

        Ok(())
    }
}

const fn default_playback_rate() -> f64 {
    1.0
}

fn is_valid_media_time(value: f64) -> bool {
    value.is_finite() && value >= 0.0 && Duration::try_from_secs_f64(value).is_ok()
}

fn validate_optional_string(
    field: &str,
    value: &Option<String>,
    max_length: usize,
) -> Result<(), String> {
    if let Some(value) = value {
        if value.len() > max_length {
            return Err(format!("{field} exceeds maximum length"));
        }
    }

    Ok(())
}

fn validate_opaque_id(kind: &str, value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };

    if value.is_empty()
        || value.len() > MAX_OPAQUE_ID_LENGTH
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("invalid {kind} identifier"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(playback: PlaybackStatus, paused: bool, media_id: Option<&str>) -> PlayerSnapshot {
        PlayerSnapshot {
            metadata: None,
            metadata_id: None,
            playback,
            position: 0.0,
            duration: 120.0,
            paused,
            media_type: Some("VIDEO".to_string()),
            media_id: media_id.map(str::to_string),
            timeline_id: None,
            timing_metadata_id: None,
            timing_observation_id: None,
            playback_rate: 1.0,
        }
    }

    #[test]
    fn accepts_valid_media_identifier() {
        let snapshot = snapshot(PlaybackStatus::Playing, false, Some("42"));

        assert_eq!(snapshot.validate(), Ok(()));
    }

    #[test]
    fn rejects_invalid_media_identifiers() {
        for media_id in ["", "resource-1", &"1".repeat(MAX_OPAQUE_ID_LENGTH + 1)] {
            let snapshot = snapshot(PlaybackStatus::Playing, false, Some(media_id));

            assert_eq!(
                snapshot.validate(),
                Err("invalid media identifier".to_string())
            );
        }
    }

    #[test]
    fn defaults_missing_timeline_fields_for_backwards_compatibility() {
        let snapshot: PlayerSnapshot = serde_json::from_value(serde_json::json!({
            "metadata": null,
            "playback": "paused",
            "position": 0.0,
            "duration": 0.0,
            "paused": true,
            "mediaType": null
        }))
        .expect("legacy snapshot should deserialize");

        assert_eq!(snapshot.metadata_id, None);
        assert_eq!(snapshot.media_id, None);
        assert_eq!(snapshot.timeline_id, None);
        assert_eq!(snapshot.timing_metadata_id, None);
        assert_eq!(snapshot.timing_observation_id, None);
        assert_eq!(snapshot.playback_rate, 1.0);
        assert_eq!(snapshot.validate(), Ok(()));
    }

    #[test]
    fn uses_camel_case_timing_fields() {
        let snapshot: PlayerSnapshot = serde_json::from_value(serde_json::json!({
            "metadata": null,
            "metadataId": "10",
            "playback": "playing",
            "position": 30.0,
            "duration": 120.0,
            "paused": false,
            "mediaType": "VIDEO",
            "mediaId": "20",
            "timelineId": "30",
            "timingMetadataId": "40",
            "timingObservationId": "50",
            "playbackRate": 1.25
        }))
        .expect("camelCase snapshot should deserialize");

        assert_eq!(snapshot.timeline_id.as_deref(), Some("30"));
        assert_eq!(snapshot.timing_metadata_id.as_deref(), Some("40"));
        assert_eq!(snapshot.timing_observation_id.as_deref(), Some("50"));
        assert_eq!(snapshot.playback_rate, 1.25);
        assert_eq!(snapshot.validate(), Ok(()));

        let serialized = serde_json::to_value(snapshot).expect("snapshot should serialize");

        assert_eq!(serialized["timelineId"], "30");
        assert_eq!(serialized["timingMetadataId"], "40");
        assert_eq!(serialized["timingObservationId"], "50");
        assert_eq!(serialized["playbackRate"], 1.25);
    }

    #[test]
    fn rejects_invalid_metadata_identifier() {
        let mut snapshot = snapshot(PlaybackStatus::Playing, false, Some("1"));
        snapshot.metadata_id = Some("metadata-1".to_string());

        assert_eq!(
            snapshot.validate(),
            Err("invalid metadata identifier".to_string())
        );
    }

    #[test]
    fn accepts_valid_timeline_identity_and_playback_rate() {
        let mut snapshot = snapshot(PlaybackStatus::Playing, false, Some("1"));
        snapshot.timeline_id = Some("20".to_string());
        snapshot.timing_metadata_id = Some("30".to_string());

        for playback_rate in [0.25, 1.0, MAX_PLAYBACK_RATE] {
            snapshot.playback_rate = playback_rate;

            assert_eq!(snapshot.validate(), Ok(()));
        }
    }

    #[test]
    fn rejects_invalid_timing_identifiers() {
        for identifier in ["", "timeline-1", &"1".repeat(MAX_OPAQUE_ID_LENGTH + 1)] {
            let mut invalid_timeline = snapshot(PlaybackStatus::Playing, false, Some("1"));
            invalid_timeline.timeline_id = Some(identifier.to_string());

            assert_eq!(
                invalid_timeline.validate(),
                Err("invalid timeline identifier".to_string())
            );

            let mut invalid_timing_metadata = snapshot(PlaybackStatus::Playing, false, Some("1"));
            invalid_timing_metadata.timing_metadata_id = Some(identifier.to_string());

            assert_eq!(
                invalid_timing_metadata.validate(),
                Err("invalid timing metadata identifier".to_string())
            );

            let mut invalid_timing_observation =
                snapshot(PlaybackStatus::Playing, false, Some("1"));
            invalid_timing_observation.timing_observation_id = Some(identifier.to_string());

            assert_eq!(
                invalid_timing_observation.validate(),
                Err("invalid timing observation identifier".to_string())
            );
        }
    }

    #[test]
    fn rejects_invalid_playback_rates() {
        for playback_rate in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1.0,
            0.0,
            MAX_PLAYBACK_RATE + 0.1,
        ] {
            let mut snapshot = snapshot(PlaybackStatus::Playing, false, Some("1"));
            snapshot.playback_rate = playback_rate;

            assert_eq!(
                snapshot.validate(),
                Err("invalid playback rate".to_string())
            );
        }
    }

    #[test]
    fn rejects_inconsistent_playback_and_paused_state() {
        let playing_but_paused = snapshot(PlaybackStatus::Playing, true, Some("1"));
        let paused_but_playing = snapshot(PlaybackStatus::Paused, false, Some("1"));

        for snapshot in [playing_but_paused, paused_but_playing] {
            assert_eq!(
                snapshot.validate(),
                Err("playback and paused state are inconsistent".to_string())
            );
        }
    }

    #[test]
    fn rejects_finite_times_that_cannot_be_represented_safely() {
        let mut invalid_position = snapshot(PlaybackStatus::Playing, false, Some("1"));
        invalid_position.position = f64::MAX;

        assert_eq!(
            invalid_position.validate(),
            Err("invalid playback position".to_string())
        );

        let mut invalid_duration = snapshot(PlaybackStatus::Playing, false, Some("1"));
        invalid_duration.duration = f64::MAX;

        assert_eq!(
            invalid_duration.validate(),
            Err("invalid media duration".to_string())
        );
    }
}

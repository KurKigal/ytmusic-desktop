use serde::{Deserialize, Serialize};

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
    pub playback: PlaybackStatus,
    pub position: f64,
    pub duration: f64,
    pub paused: bool,
    pub media_type: Option<String>,
}

impl PlayerSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if !self.position.is_finite() || self.position < 0.0 {
            return Err("invalid playback position".into());
        }

        if !self.duration.is_finite() || self.duration < 0.0 {
            return Err("invalid media duration".into());
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

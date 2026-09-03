use std::{
    collections::VecDeque,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};

use tauri::{App, Manager};

use crate::player::{PlaybackStatus, PlayerSnapshot, PlayerStore};

const DISCORD_CLIENT_ID: &str = "1544449927776571483";

const DISCORD_RETRY_INTERVAL: Duration = Duration::from_secs(15);

// Discord permits at most five activity writes in a 20-second window.
// Material state changes can use the available burst capacity, while the
// sliding window still protects the IPC connection from rapid seek/toggle
// sequences.
const DISCORD_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(20);
const DISCORD_MAX_WRITES_PER_WINDOW: usize = 5;

const SEEK_DRIFT_THRESHOLD_SECONDS: u64 = 3;
const DURATION_DRIFT_THRESHOLD_SECONDS: u64 = 2;
const POSITION_END_TOLERANCE_SECONDS: f64 = 2.0;

// A transition is accepted from observed state, not elapsed wall time. Two
// distinct paired PositionState observations are enough to reject YTM's
// transient handoff metadata without imposing an arbitrary sleep.
const TRANSITION_STABILITY_OBSERVATIONS: u8 = 2;
const TRANSITION_PAUSE_STABILITY_OBSERVATIONS: u8 = 2;

const DISCORD_TEXT_LIMIT: usize = 128;

const YOUTUBE_MUSIC_URL: &str = "https://music.youtube.com";

/// Starts the Discord Rich Presence integration.
///
/// The PlayerStore receiver is consumed on a dedicated thread because
/// Discord IPC uses synchronous I/O. A watch receiver is kept end-to-end so
/// slow Discord operations coalesce intermediate player ticks instead of
/// replaying stale snapshots later.
pub fn setup_discord_presence(app: &App) {
    if DISCORD_CLIENT_ID.trim().is_empty() || DISCORD_CLIENT_ID == "YOUR_DISCORD_APPLICATION_ID" {
        eprintln!("[discord] Discord Application ID is not configured");

        return;
    }

    let mut player_receiver = app.state::<PlayerStore>().subscribe();

    let worker = thread::Builder::new()
        .name("discord-rpc".to_string())
        .spawn(move || {
            let mut presence = DiscordPresence::new();

            let initial = player_receiver.borrow_and_update().clone();

            if let Some(snapshot) = initial {
                presence.sync(&snapshot);
            }

            loop {
                if tauri::async_runtime::block_on(player_receiver.changed()).is_err() {
                    break;
                }

                let current = player_receiver.borrow_and_update().clone();

                let Some(current) = current else {
                    continue;
                };

                presence.sync(&current);
            }

            presence.shutdown();
        });

    if let Err(error) = worker {
        eprintln!("[discord] failed to start worker thread: {error}");

        return;
    }

    println!("[discord] Rich Presence integration initialized");
}

struct DiscordPresence {
    client: DiscordIpcClient,
    connected: bool,
    next_retry_at: Instant,
    activity: ActivityPlanner,
    rate_limiter: WriteRateLimiter,
}

enum ConnectionState {
    Ready,
    JustConnected,
    Unavailable,
}

impl DiscordPresence {
    fn new() -> Self {
        Self {
            client: DiscordIpcClient::new(DISCORD_CLIENT_ID),
            connected: false,
            next_retry_at: Instant::now(),
            activity: ActivityPlanner::default(),
            rate_limiter: WriteRateLimiter::default(),
        }
    }

    fn sync(&mut self, snapshot: &PlayerSnapshot) {
        let Some(now) = unix_timestamp_seconds() else {
            return;
        };

        let desired = match self.activity.evaluate(snapshot, now) {
            ActivityPlan::Publish(desired) => desired,
            ActivityPlan::Clear => {
                self.clear_if_needed();
                return;
            }
            ActivityPlan::Hold | ActivityPlan::Ignore => return,
        };

        match self.ensure_connected() {
            ConnectionState::Ready => {}
            ConnectionState::JustConnected | ConnectionState::Unavailable => return,
        }

        let observed_at = Instant::now();

        if !self.rate_limiter.allows_activity_update(observed_at) {
            return;
        }

        let activity = build_activity(&desired);

        match self.client.set_activity(activity) {
            Ok(()) => {
                println!(
                    "[discord] activity updated: {} — {}",
                    desired.track.title, desired.track.artist
                );

                self.rate_limiter.record(observed_at);
                self.activity.mark_published(desired);
            }

            Err(error) => {
                self.handle_connection_error(&format!("failed to update activity: {error}"));
            }
        }
    }

    fn ensure_connected(&mut self) -> ConnectionState {
        if self.connected {
            return ConnectionState::Ready;
        }

        let now = Instant::now();

        if now < self.next_retry_at {
            return ConnectionState::Unavailable;
        }

        match self.client.connect() {
            Ok(()) => {
                self.connected = true;
                self.activity.mark_hidden();

                println!("[discord] connected to Discord");

                // Re-evaluate the latest PlayerStore value on the next tick
                // instead of publishing the snapshot captured before a
                // potentially slow Discord handshake.
                ConnectionState::JustConnected
            }

            Err(error) => {
                self.next_retry_at = now + DISCORD_RETRY_INTERVAL;

                eprintln!("[discord] Discord is unavailable: {error}");

                ConnectionState::Unavailable
            }
        }
    }

    fn clear_if_needed(&mut self) {
        if !self.connected || !self.activity.is_visible() {
            return;
        }

        let now = Instant::now();

        if !self.rate_limiter.allows_clear(now) {
            return;
        }

        match self.client.clear_activity() {
            Ok(()) => {
                self.activity.mark_hidden();
                self.rate_limiter.record(now);

                println!("[discord] activity cleared");
            }

            Err(error) => {
                self.handle_connection_error(&format!("failed to clear activity: {error}"));
            }
        }
    }

    fn handle_connection_error(&mut self, message: &str) {
        eprintln!("[discord] {message}");

        let _ = self.client.close();

        self.client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
        self.connected = false;
        self.activity.mark_hidden();
        self.next_retry_at = Instant::now() + DISCORD_RETRY_INTERVAL;
    }

    fn shutdown(&mut self) {
        if !self.connected {
            return;
        }

        if self.activity.is_visible() {
            let _ = self.client.clear_activity();
        }

        let _ = self.client.close();

        self.connected = false;
        self.activity.mark_hidden();

        println!("[discord] Rich Presence integration stopped");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrackIdentity {
    title: String,
    artist: String,
    album: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PlaybackAnchor {
    started_at: i64,
    duration: i64,
}

impl PlaybackAnchor {
    fn from_player(snapshot: &PlayerSnapshot, now: i64) -> Option<Self> {
        if snapshot.duration <= 0.0
            || snapshot.position > snapshot.duration + POSITION_END_TOLERANCE_SECONDS
            || !snapshot.playback_rate.is_finite()
            || snapshot.playback_rate <= 0.0
        {
            return None;
        }

        // Discord's timestamps always advance at wall-clock speed. Scaling by
        // playbackRate keeps the anchor stable and the end time truthful when
        // YouTube Music is playing faster or slower than 1x.
        let duration = rounded_seconds(snapshot.duration / snapshot.playback_rate)?;

        if duration <= 0 {
            return None;
        }

        let position =
            rounded_seconds(snapshot.position.min(snapshot.duration) / snapshot.playback_rate)?;

        Some(Self {
            started_at: now.saturating_sub(position),
            duration,
        })
    }

    fn ends_at(&self) -> i64 {
        self.started_at.saturating_add(self.duration)
    }

    fn materially_differs(&self, other: &Self) -> bool {
        self.started_at.abs_diff(other.started_at) >= SEEK_DRIFT_THRESHOLD_SECONDS
            || self.duration.abs_diff(other.duration) >= DURATION_DRIFT_THRESHOLD_SECONDS
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PresenceSnapshot {
    track: TrackIdentity,
    artwork_url: Option<String>,
    metadata_id: String,
    timeline_id: String,
    timing_observation_id: String,
    playback: PlaybackAnchor,
}

impl PresenceSnapshot {
    fn from_player(snapshot: &PlayerSnapshot, now: i64) -> Option<Self> {
        if !matches!(snapshot.playback, PlaybackStatus::Playing) || snapshot.paused {
            return None;
        }

        let metadata = snapshot.metadata.as_ref()?;
        let metadata_id = snapshot.metadata_id.clone()?;
        let timeline_id = snapshot.timeline_id.clone()?;
        let timing_observation_id = snapshot.timing_observation_id.clone()?;

        if snapshot.timing_metadata_id.as_deref() != Some(metadata_id.as_str()) {
            return None;
        }

        let track = TrackIdentity {
            title: normalize_required_text(metadata.title.as_deref(), "Unknown title"),
            artist: normalize_required_text(metadata.artist.as_deref(), "Unknown artist"),
            album: normalize_optional_text(metadata.album.as_deref()),
        };

        let artwork_url = metadata
            .artwork
            .iter()
            .rev()
            .find(|artwork| {
                artwork.src.starts_with("https://") || artwork.src.starts_with("http://")
            })
            .map(|artwork| artwork.src.clone());

        let playback = PlaybackAnchor::from_player(snapshot, now)?;

        Some(Self {
            track,
            artwork_url,
            metadata_id,
            timeline_id,
            timing_observation_id,
            playback,
        })
    }

    fn materially_differs(&self, other: &Self) -> bool {
        self.track != other.track
            || self.artwork_url != other.artwork_url
            || self.playback.materially_differs(&other.playback)
    }

    fn generation(&self) -> GenerationKey {
        GenerationKey {
            metadata_id: self.metadata_id.clone(),
            timeline_id: self.timeline_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GenerationKey {
    metadata_id: String,
    timeline_id: String,
}

impl GenerationKey {
    fn from_paired_snapshot(snapshot: &PlayerSnapshot) -> Option<Self> {
        let metadata_id = snapshot.metadata_id.clone()?;

        if snapshot.timing_metadata_id.as_deref() != Some(metadata_id.as_str()) {
            return None;
        }

        Some(Self {
            metadata_id,
            timeline_id: snapshot.timeline_id.clone()?,
        })
    }
}

#[derive(Default)]
struct ActivityPlanner {
    transition: TransitionGuard,
    visible: bool,
    last_published: Option<PresenceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActivityPlan {
    Publish(PresenceSnapshot),
    Hold,
    Clear,
    Ignore,
}

impl ActivityPlanner {
    fn evaluate(&mut self, snapshot: &PlayerSnapshot, now: i64) -> ActivityPlan {
        match self.transition.evaluate(snapshot, now) {
            TransitionDecision::Ready(desired) => {
                let changed = self
                    .last_published
                    .as_ref()
                    .is_none_or(|previous| previous.materially_differs(&desired));

                if self.visible && !changed {
                    ActivityPlan::Ignore
                } else {
                    ActivityPlan::Publish(desired)
                }
            }
            TransitionDecision::KeepVisible => ActivityPlan::Hold,
            TransitionDecision::Clear if self.visible => ActivityPlan::Clear,
            TransitionDecision::Clear => ActivityPlan::Ignore,
        }
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn mark_published(&mut self, published: PresenceSnapshot) {
        self.visible = true;
        self.last_published = Some(published);
    }

    fn mark_hidden(&mut self) {
        self.visible = false;
    }
}

/// Holds asynchronous MediaSession handoffs until metadata and timing report
/// the same generation in distinct PositionState observations. A pause on the
/// coherent generation remains an immediate manual pause; a pause on a new
/// generation needs two paired observations before it is considered settled.
#[derive(Default)]
struct TransitionGuard {
    last_coherent: Option<PresenceSnapshot>,
    pending_presence: Option<PendingPresence>,
    pending_pause: Option<PendingPause>,
}

struct PendingPresence {
    candidate: PresenceSnapshot,
    observations: u8,
}

struct PendingPause {
    generation: GenerationKey,
    observations: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TransitionDecision {
    Ready(PresenceSnapshot),
    KeepVisible,
    Clear,
}

impl TransitionGuard {
    fn evaluate(&mut self, snapshot: &PlayerSnapshot, now: i64) -> TransitionDecision {
        if snapshot.paused || !matches!(snapshot.playback, PlaybackStatus::Playing) {
            return self.evaluate_paused(snapshot);
        }

        self.pending_pause = None;

        let Some(desired) = PresenceSnapshot::from_player(snapshot, now) else {
            self.pending_presence = None;
            return TransitionDecision::KeepVisible;
        };

        let Some(coherent) = self.last_coherent.as_ref() else {
            self.last_coherent = Some(desired.clone());
            return TransitionDecision::Ready(desired);
        };

        let generation_changed = desired.generation() != coherent.generation();

        if !generation_changed || !coherent.materially_differs(&desired) {
            self.pending_presence = None;
            self.last_coherent = Some(desired.clone());
            return TransitionDecision::Ready(desired);
        }

        let observations = self.pending_presence.as_ref().map_or(1, |pending| {
            let same_stable_candidate = pending.candidate.generation() == desired.generation()
                && !pending.candidate.materially_differs(&desired);
            let new_timing_observation =
                pending.candidate.timing_observation_id != desired.timing_observation_id;

            if same_stable_candidate && new_timing_observation {
                pending.observations.saturating_add(1)
            } else if same_stable_candidate {
                pending.observations
            } else {
                1
            }
        });

        if observations >= TRANSITION_STABILITY_OBSERVATIONS {
            self.pending_presence = None;
            self.last_coherent = Some(desired.clone());
            TransitionDecision::Ready(desired)
        } else {
            self.pending_presence = Some(PendingPresence {
                candidate: desired,
                observations,
            });
            TransitionDecision::KeepVisible
        }
    }

    fn evaluate_paused(&mut self, snapshot: &PlayerSnapshot) -> TransitionDecision {
        self.pending_presence = None;

        let Some(coherent) = self.last_coherent.as_ref() else {
            self.pending_pause = None;
            return TransitionDecision::Clear;
        };

        let coherent_generation = coherent.generation();
        let metadata_changed =
            snapshot.metadata_id.as_deref() != Some(coherent_generation.metadata_id.as_str());
        let timeline_changed =
            snapshot.timeline_id.as_deref() != Some(coherent_generation.timeline_id.as_str());
        let timing_unpaired = snapshot.timing_metadata_id != snapshot.metadata_id;

        if !metadata_changed && !timeline_changed && !timing_unpaired {
            self.pending_pause = None;
            return TransitionDecision::Clear;
        }

        let Some(generation) = GenerationKey::from_paired_snapshot(snapshot) else {
            self.pending_pause = None;
            return TransitionDecision::KeepVisible;
        };

        let observations = self
            .pending_pause
            .as_ref()
            .filter(|pending| pending.generation == generation)
            .map_or(1, |pending| pending.observations.saturating_add(1));

        if observations >= TRANSITION_PAUSE_STABILITY_OBSERVATIONS {
            self.pending_pause = None;
            TransitionDecision::Clear
        } else {
            self.pending_pause = Some(PendingPause {
                generation,
                observations,
            });
            TransitionDecision::KeepVisible
        }
    }
}

#[derive(Default)]
struct WriteRateLimiter {
    writes: VecDeque<Instant>,
}

impl WriteRateLimiter {
    fn allows_activity_update(&mut self, now: Instant) -> bool {
        self.allows(now, DISCORD_MAX_WRITES_PER_WINDOW - 1)
    }

    fn allows_clear(&mut self, now: Instant) -> bool {
        self.allows(now, DISCORD_MAX_WRITES_PER_WINDOW)
    }

    fn allows(&mut self, now: Instant, limit: usize) -> bool {
        while self
            .writes
            .front()
            .is_some_and(|written_at| now.duration_since(*written_at) >= DISCORD_RATE_LIMIT_WINDOW)
        {
            self.writes.pop_front();
        }

        self.writes.len() < limit
    }

    fn record(&mut self, now: Instant) {
        self.writes.push_back(now);
    }
}

fn build_activity<'a>(presence: &'a PresenceSnapshot) -> activity::Activity<'a> {
    let mut result = activity::Activity::new()
        .activity_type(activity::ActivityType::Listening)
        .details(presence.track.title.as_str())
        .state(presence.track.artist.as_str())
        .buttons(vec![activity::Button::new(
            "Open YouTube Music",
            YOUTUBE_MUSIC_URL,
        )])
        .timestamps(
            activity::Timestamps::new()
                .start(presence.playback.started_at)
                .end(presence.playback.ends_at()),
        );

    if let Some(artwork_url) = &presence.artwork_url {
        let large_text = presence
            .track
            .album
            .as_deref()
            .unwrap_or(presence.track.title.as_str());

        let assets = activity::Assets::new()
            .large_image(artwork_url.as_str())
            .large_text(large_text)
            .large_url(YOUTUBE_MUSIC_URL);

        result = result.assets(assets);
    }

    result
}

fn unix_timestamp_seconds() -> Option<i64> {
    // discord-rich-presence forwards timestamps unchanged. The desktop
    // SET_ACTIVITY RPC uses the legacy Unix-seconds convention; Discord's
    // millisecond Activity timestamps describe the separate Gateway model.
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;

    i64::try_from(elapsed.as_secs()).ok()
}

fn rounded_seconds(value: f64) -> Option<i64> {
    if !value.is_finite() || value < 0.0 || value > i64::MAX as f64 {
        return None;
    }

    Some(value.round() as i64)
}

fn normalize_required_text(value: Option<&str>, fallback: &str) -> String {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);

    truncate_text(value)
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(truncate_text)
}

fn truncate_text(value: &str) -> String {
    value.chars().take(DISCORD_TEXT_LIMIT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{Artwork, TrackMetadata};

    const NOW: i64 = 1_000;

    fn snapshot(
        title: &str,
        metadata_id: &str,
        timeline_id: &str,
        timing_metadata_id: Option<&str>,
        playback: PlaybackStatus,
        timing: (f64, f64),
        playback_rate: f64,
    ) -> PlayerSnapshot {
        let (position, duration) = timing;
        let paused = !matches!(playback, PlaybackStatus::Playing);

        PlayerSnapshot {
            metadata: Some(TrackMetadata {
                title: Some(title.to_owned()),
                artist: Some("Artist".to_owned()),
                album: Some("Album".to_owned()),
                artwork: vec![Artwork {
                    src: "https://example.com/art.jpg".to_owned(),
                    sizes: None,
                    r#type: None,
                }],
            }),
            metadata_id: Some(metadata_id.to_owned()),
            playback,
            position,
            duration,
            paused,
            media_type: Some("AUDIO".to_owned()),
            media_id: Some(timeline_id.to_owned()),
            timeline_id: Some(timeline_id.to_owned()),
            timing_metadata_id: timing_metadata_id.map(str::to_owned),
            timing_observation_id: Some(((position * 1_000.0).round() as u64 + 1).to_string()),
            playback_rate,
        }
    }

    fn playing(
        title: &str,
        metadata_id: &str,
        timeline_id: &str,
        position: f64,
        duration: f64,
    ) -> PlayerSnapshot {
        snapshot(
            title,
            metadata_id,
            timeline_id,
            Some(metadata_id),
            PlaybackStatus::Playing,
            (position, duration),
            1.0,
        )
    }

    fn paused(
        title: &str,
        metadata_id: &str,
        timeline_id: &str,
        timing_metadata_id: Option<&str>,
        position: f64,
        duration: f64,
    ) -> PlayerSnapshot {
        snapshot(
            title,
            metadata_id,
            timeline_id,
            timing_metadata_id,
            PlaybackStatus::Paused,
            (position, duration),
            1.0,
        )
    }

    fn evaluate_and_commit(
        planner: &mut ActivityPlanner,
        snapshot: &PlayerSnapshot,
        now: i64,
    ) -> ActivityPlan {
        let plan = planner.evaluate(snapshot, now);
        match &plan {
            ActivityPlan::Publish(presence) => planner.mark_published(presence.clone()),
            ActivityPlan::Clear => planner.mark_hidden(),
            ActivityPlan::Hold | ActivityPlan::Ignore => {}
        }
        plan
    }

    fn published(plan: &ActivityPlan) -> &PresenceSnapshot {
        match plan {
            ActivityPlan::Publish(presence) => presence,
            other => panic!("expected publish, got {other:?}"),
        }
    }

    fn publish_count(plans: &[ActivityPlan]) -> usize {
        plans
            .iter()
            .filter(|plan| matches!(plan, ActivityPlan::Publish(_)))
            .count()
    }

    fn clear_count(plans: &[ActivityPlan]) -> usize {
        plans
            .iter()
            .filter(|plan| matches!(plan, ActivityPlan::Clear))
            .count()
    }

    #[test]
    fn normal_sixty_second_progress_publishes_once() {
        let mut planner = ActivityPlanner::default();
        let mut plans = Vec::new();

        for second in 0..=60 {
            plans.push(evaluate_and_commit(
                &mut planner,
                &playing("Track A", "10", "100", second as f64, 180.0),
                NOW + second,
            ));
        }

        assert_eq!(publish_count(&plans), 1);
        assert_eq!(clear_count(&plans), 0);
        assert!(plans[1..]
            .iter()
            .all(|plan| matches!(plan, ActivityPlan::Ignore)));

        let first = published(&plans[0]);
        assert_eq!(
            first.playback,
            PlaybackAnchor {
                started_at: NOW,
                duration: 180,
            }
        );
    }

    #[test]
    fn large_seek_causes_exactly_one_corrective_publish() {
        let mut planner = ActivityPlanner::default();
        let plans = vec![
            evaluate_and_commit(
                &mut planner,
                &playing("Track A", "10", "100", 30.0, 180.0),
                NOW,
            ),
            evaluate_and_commit(
                &mut planner,
                &playing("Track A", "10", "100", 31.0, 180.0),
                NOW + 1,
            ),
            evaluate_and_commit(
                &mut planner,
                &playing("Track A", "10", "100", 120.0, 180.0),
                NOW + 2,
            ),
            evaluate_and_commit(
                &mut planner,
                &playing("Track A", "10", "100", 121.0, 180.0),
                NOW + 3,
            ),
        ];

        assert_eq!(publish_count(&plans), 2);
        assert_eq!(clear_count(&plans), 0);
        assert!(matches!(plans[1], ActivityPlan::Ignore));
        assert!(matches!(plans[3], ActivityPlan::Ignore));

        let correction = published(&plans[2]);
        assert_eq!(
            correction.playback,
            PlaybackAnchor {
                started_at: NOW - 118,
                duration: 180,
            }
        );
    }

    #[test]
    fn transient_new_track_pause_and_unpaired_timing_hold_until_two_coherent_samples() {
        let mut planner = ActivityPlanner::default();
        let initial = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 170.0, 180.0),
            NOW,
        );
        assert!(matches!(initial, ActivityPlan::Publish(_)));

        let transient_pause = evaluate_and_commit(
            &mut planner,
            &paused("Track B", "20", "200", Some("10"), 0.0, 160.0),
            NOW + 10,
        );
        assert!(matches!(transient_pause, ActivityPlan::Hold));

        let active_b_with_old_timing = snapshot(
            "Track B",
            "20",
            "200",
            Some("10"),
            PlaybackStatus::Playing,
            (0.0, 160.0),
            1.0,
        );
        let incomplete = evaluate_and_commit(&mut planner, &active_b_with_old_timing, NOW + 10);
        assert!(matches!(incomplete, ActivityPlan::Hold));

        let mut first_b_timing = playing("Track B", "20", "200", 0.0, 160.0);
        first_b_timing.timing_observation_id = Some("500".to_owned());
        let first_coherent = evaluate_and_commit(&mut planner, &first_b_timing, NOW + 10);
        assert!(matches!(first_coherent, ActivityPlan::Hold));

        let mut repeated_b_timing = first_b_timing.clone();
        repeated_b_timing.position = 1.0;
        let repeated_coherent = evaluate_and_commit(&mut planner, &repeated_b_timing, NOW + 11);
        assert!(matches!(repeated_coherent, ActivityPlan::Hold));

        let mut second_b_timing = repeated_b_timing;
        second_b_timing.timing_observation_id = Some("501".to_owned());
        let second_coherent = evaluate_and_commit(&mut planner, &second_b_timing, NOW + 11);
        let published_b = published(&second_coherent);
        assert_eq!(published_b.track.title, "Track B");
        assert_eq!(published_b.generation().metadata_id, "20");
        assert_eq!(published_b.generation().timeline_id, "200");
        assert_eq!(
            published_b.playback,
            PlaybackAnchor {
                started_at: NOW + 10,
                duration: 160,
            }
        );

        let transition_plans = [
            transient_pause,
            incomplete,
            first_coherent,
            repeated_coherent,
            second_coherent,
        ];
        assert_eq!(clear_count(&transition_plans), 0);
        assert_eq!(publish_count(&transition_plans), 1);
    }

    #[test]
    fn manual_same_generation_pause_clears_immediately() {
        let mut planner = ActivityPlanner::default();
        let initial = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 30.0, 180.0),
            NOW,
        );
        assert!(matches!(initial, ActivityPlan::Publish(_)));

        let first_pause = evaluate_and_commit(
            &mut planner,
            &paused("Track A", "10", "100", Some("10"), 30.0, 180.0),
            NOW + 1,
        );
        let repeated_pause = evaluate_and_commit(
            &mut planner,
            &paused("Track A", "10", "100", Some("10"), 30.0, 180.0),
            NOW + 2,
        );

        assert!(matches!(first_pause, ActivityPlan::Clear));
        assert!(matches!(repeated_pause, ActivityPlan::Ignore));
    }

    #[test]
    fn resume_republishes_once_with_a_fresh_anchor() {
        let mut planner = ActivityPlanner::default();
        let initial = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 30.0, 180.0),
            NOW,
        );
        assert_eq!(published(&initial).playback.started_at, NOW - 30);

        let pause = evaluate_and_commit(
            &mut planner,
            &paused("Track A", "10", "100", Some("10"), 30.0, 180.0),
            NOW + 2,
        );
        assert!(matches!(pause, ActivityPlan::Clear));

        let resume = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 30.0, 180.0),
            NOW + 5,
        );
        assert_eq!(
            published(&resume).playback,
            PlaybackAnchor {
                started_at: NOW - 25,
                duration: 180,
            }
        );

        let progress = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 31.0, 180.0),
            NOW + 6,
        );
        assert!(matches!(progress, ActivityPlan::Ignore));
    }

    #[test]
    fn reconnect_republishes_the_current_activity_once() {
        let mut planner = ActivityPlanner::default();
        let initial = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 0.0, 180.0),
            NOW,
        );
        assert!(matches!(initial, ActivityPlan::Publish(_)));

        let progress = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 1.0, 180.0),
            NOW + 1,
        );
        assert!(matches!(progress, ActivityPlan::Ignore));

        // This is the planner-side state change made by ensure_connected after
        // Discord establishes a fresh IPC session.
        planner.mark_hidden();

        let reconnect = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 2.0, 180.0),
            NOW + 2,
        );
        assert_eq!(published(&reconnect).playback.started_at, NOW);

        let after_reconnect = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 3.0, 180.0),
            NOW + 3,
        );
        assert!(matches!(after_reconnect, ActivityPlan::Ignore));
    }

    #[test]
    fn metadata_correction_and_id_churn_publish_only_the_payload_change() {
        let mut planner = ActivityPlanner::default();
        let mut plans = vec![evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 0.0, 180.0),
            NOW,
        )];

        plans.push(evaluate_and_commit(
            &mut planner,
            &playing("Track A (corrected)", "20", "100", 1.0, 180.0),
            NOW + 1,
        ));
        plans.push(evaluate_and_commit(
            &mut planner,
            &playing("Track A (corrected)", "20", "100", 2.0, 180.0),
            NOW + 2,
        ));

        for offset in 3..=12 {
            let metadata_id = (20 + offset).to_string();
            plans.push(evaluate_and_commit(
                &mut planner,
                &playing(
                    "Track A (corrected)",
                    &metadata_id,
                    "100",
                    offset as f64,
                    180.0,
                ),
                NOW + offset,
            ));
        }

        assert!(matches!(plans[1], ActivityPlan::Hold));
        assert!(matches!(plans[2], ActivityPlan::Publish(_)));
        assert!(plans[3..]
            .iter()
            .all(|plan| matches!(plan, ActivityPlan::Ignore)));
        assert_eq!(publish_count(&plans), 2);
        assert_eq!(clear_count(&plans), 0);
    }

    #[test]
    fn stable_paused_new_generation_clears_after_two_observations() {
        let mut planner = ActivityPlanner::default();
        let initial = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 170.0, 180.0),
            NOW,
        );
        assert!(matches!(initial, ActivityPlan::Publish(_)));

        let first = evaluate_and_commit(
            &mut planner,
            &paused("Track B", "20", "200", Some("20"), 0.0, 160.0),
            NOW + 1,
        );
        let second = evaluate_and_commit(
            &mut planner,
            &paused("Track B", "20", "200", Some("20"), 0.0, 160.0),
            NOW + 2,
        );

        assert!(matches!(first, ActivityPlan::Hold));
        assert!(matches!(second, ActivityPlan::Clear));
    }

    #[test]
    fn double_speed_progress_keeps_wall_clock_anchor_stable() {
        let mut planner = ActivityPlanner::default();
        let first_snapshot = snapshot(
            "Track A",
            "10",
            "100",
            Some("10"),
            PlaybackStatus::Playing,
            (60.0, 180.0),
            2.0,
        );
        let first = evaluate_and_commit(&mut planner, &first_snapshot, NOW);
        assert_eq!(
            published(&first).playback,
            PlaybackAnchor {
                started_at: NOW - 30,
                duration: 90,
            }
        );

        let progressed_snapshot = snapshot(
            "Track A",
            "10",
            "100",
            Some("10"),
            PlaybackStatus::Playing,
            (120.0, 180.0),
            2.0,
        );
        let progressed = evaluate_and_commit(&mut planner, &progressed_snapshot, NOW + 30);
        assert!(matches!(progressed, ActivityPlan::Ignore));
    }

    #[test]
    fn invalid_or_unpaired_active_snapshots_do_not_publish_or_clear() {
        let mut planner = ActivityPlanner::default();
        let initial = evaluate_and_commit(
            &mut planner,
            &playing("Track A", "10", "100", 0.0, 180.0),
            NOW,
        );
        assert!(matches!(initial, ActivityPlan::Publish(_)));

        let unpaired = snapshot(
            "Track B",
            "20",
            "200",
            Some("10"),
            PlaybackStatus::Playing,
            (0.0, 160.0),
            1.0,
        );
        assert!(matches!(
            evaluate_and_commit(&mut planner, &unpaired, NOW + 1),
            ActivityPlan::Hold
        ));

        let mut missing_timeline = unpaired;
        missing_timeline.timeline_id = None;
        missing_timeline.timing_metadata_id = Some("20".to_owned());
        assert!(matches!(
            evaluate_and_commit(&mut planner, &missing_timeline, NOW + 2),
            ActivityPlan::Hold
        ));
    }

    #[test]
    fn rate_limiter_respects_window_and_reserved_clear_capacity() {
        let mut limiter = WriteRateLimiter::default();
        let now = Instant::now();

        for _ in 0..(DISCORD_MAX_WRITES_PER_WINDOW - 1) {
            assert!(limiter.allows_activity_update(now));
            limiter.record(now);
        }
        assert!(!limiter.allows_activity_update(now));
        assert!(limiter.allows_clear(now));
        limiter.record(now);
        assert!(!limiter.allows_clear(now));

        assert!(!limiter.allows_activity_update(now + Duration::from_secs(19)));
        let after_window = now + DISCORD_RATE_LIMIT_WINDOW;
        assert!(limiter.allows_activity_update(after_window));
        limiter.record(after_window);
        assert!(limiter.allows_clear(after_window));
    }

    #[test]
    fn unicode_text_is_truncated_by_scalar_and_activity_serializes_as_listening() {
        let long_title = "🎵".repeat(DISCORD_TEXT_LIMIT + 12);
        let source = snapshot(
            &long_title,
            "10",
            "100",
            Some("10"),
            PlaybackStatus::Playing,
            (30.0, 180.0),
            1.0,
        );
        let presence = PresenceSnapshot::from_player(&source, NOW).unwrap();

        assert_eq!(presence.track.title.chars().count(), DISCORD_TEXT_LIMIT);
        assert_eq!(presence.track.title, "🎵".repeat(DISCORD_TEXT_LIMIT));

        let serialized = serde_json::to_value(build_activity(&presence)).unwrap();
        assert_eq!(serialized["type"], 2);
        assert_eq!(
            serialized["details"].as_str().unwrap().chars().count(),
            DISCORD_TEXT_LIMIT
        );
        assert_eq!(serialized["state"], "Artist");
        assert_eq!(serialized["timestamps"]["start"], NOW - 30);
        assert_eq!(serialized["timestamps"]["end"], NOW + 150);
        assert_eq!(serialized["buttons"][0]["label"], "Open YouTube Music");
        assert_eq!(serialized["buttons"][0]["url"], "https://music.youtube.com");
    }

    #[test]
    fn rounded_anchor_tolerates_small_clock_jitter_but_not_a_seek() {
        let baseline = PlaybackAnchor {
            started_at: 1_000,
            duration: 180,
        };
        assert!(!baseline.materially_differs(&PlaybackAnchor {
            started_at: 1_002,
            duration: 181,
        }));
        assert!(baseline.materially_differs(&PlaybackAnchor {
            started_at: 1_003,
            duration: 180,
        }));
        assert!(baseline.materially_differs(&PlaybackAnchor {
            started_at: 1_000,
            duration: 182,
        }));
    }
}

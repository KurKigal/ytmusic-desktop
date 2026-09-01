use tauri::{App, Manager};

use super::{PlayerSnapshot, PlayerStore};

/// Starts a background observer for player state changes.
///
/// This currently handles diagnostic logging. Future integrations
/// such as native media controls and Discord RPC will subscribe
/// to the same PlayerStore independently.
pub fn start_player_state_observer(app: &App) {
    let mut receiver = app.state::<PlayerStore>().subscribe();

    tauri::async_runtime::spawn(async move {
        let mut previous: Option<PlayerSnapshot> = None;

        loop {
            if receiver.changed().await.is_err() {
                break;
            }

            let current = receiver.borrow_and_update().clone();

            let Some(current) = current else {
                continue;
            };

            log_state_transition(previous.as_ref(), &current);

            previous = Some(current);
        }
    });

    println!("[player] state observer initialized");
}

fn log_state_transition(previous: Option<&PlayerSnapshot>, current: &PlayerSnapshot) {
    let Some(previous) = previous else {
        log_track_change(current);

        println!("[player] playback initialized: {:?}", current.playback);

        return;
    };

    if track_changed(previous, current) {
        log_track_change(current);
    }

    if previous.playback != current.playback {
        println!("[player] playback changed: {:?}", current.playback);
    }
}

fn track_changed(previous: &PlayerSnapshot, current: &PlayerSnapshot) -> bool {
    match (&previous.metadata, &current.metadata) {
        (None, None) => false,

        (Some(_), None) | (None, Some(_)) => true,

        (Some(previous), Some(current)) => {
            previous.title != current.title || previous.artist != current.artist
        }
    }
}

fn log_track_change(snapshot: &PlayerSnapshot) {
    let Some(metadata) = &snapshot.metadata else {
        println!("[player] track metadata unavailable");

        return;
    };

    let title = metadata.title.as_deref().unwrap_or("Unknown title");

    let artist = metadata.artist.as_deref().unwrap_or("Unknown artist");

    println!("[player] track changed: {title} — {artist}");
}

mod controller;
mod model;
mod store;

pub use controller::control_player;

pub(crate) use controller::{
    dispatch_player_command,
    PlayerCommand,
};

pub use model::*;
pub use store::PlayerStore;

use tauri::State;

#[tauri::command]
pub fn update_player_state(
    store: State<'_, PlayerStore>,
    payload: PlayerSnapshot,
) -> Result<(), String> {
    payload.validate()?;

    let (previous, current) = store.update(payload)?;

    if let Some(previous) = previous {
        if track_changed(&previous, &current) {
            log_track_change(&current);
        }

        if previous.playback != current.playback {
            println!(
                "[player] playback changed: {:?}",
                current.playback
            );
        }
    } else {
        log_track_change(&current);

        println!(
            "[player] playback initialized: {:?}",
            current.playback
        );
    }

    Ok(())
}

fn track_changed(
    previous: &PlayerSnapshot,
    current: &PlayerSnapshot,
) -> bool {
    match (&previous.metadata, &current.metadata) {
        (None, None) => false,

        (Some(_), None) | (None, Some(_)) => true,

        (Some(previous), Some(current)) => {
            previous.title != current.title
                || previous.artist != current.artist
                || previous.album != current.album
        }
    }
}

fn log_track_change(snapshot: &PlayerSnapshot) {
    let Some(metadata) = &snapshot.metadata else {
        println!("[player] track metadata unavailable");
        return;
    };

    let title = metadata
        .title
        .as_deref()
        .unwrap_or("Unknown title");

    let artist = metadata
        .artist
        .as_deref()
        .unwrap_or("Unknown artist");

    println!(
        "[player] track changed: {title} — {artist}"
    );
}
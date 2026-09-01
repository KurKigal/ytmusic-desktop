mod model;
mod store;

pub use model::*;
pub use store::PlayerStore;

use tauri::State;

#[tauri::command]
pub fn update_player_state(
    store: State<'_, PlayerStore>,
    payload: PlayerSnapshot,
) -> Result<(), String> {
    payload.validate()?;

    let previous = store.update(payload.clone())?;

    if let Some(previous) = previous {
        if previous.metadata != payload.metadata {
            log_track_change(&payload);
        }

        if previous.playback != payload.playback {
            println!(
                "[player] playback changed: {:?}",
                payload.playback
            );
        }
    } else {
        log_track_change(&payload);

        println!(
            "[player] playback initialized: {:?}",
            payload.playback
        );
    }

    Ok(())
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

    println!("[player] track changed: {title} — {artist}");
}
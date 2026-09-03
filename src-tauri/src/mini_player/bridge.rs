use tauri::{App, Emitter, Manager};

use crate::player::PlayerStore;

use super::{MiniPlayerState, MINI_PLAYER_STATE_EVENT, MINI_PLAYER_WINDOW_LABEL};

/// Forwards the narrow public player state to the trusted local Mini Player.
/// The current snapshot is emitted once before waiting for later store updates.
pub fn start_mini_player_state_bridge(app: &App) {
    let mut receiver = app.state::<PlayerStore>().subscribe();
    let app_handle = app.handle().clone();

    tauri::async_runtime::spawn(async move {
        let current = receiver.borrow().clone();

        if let Some(snapshot) = current.as_ref() {
            emit_state(&app_handle, MiniPlayerState::from(snapshot));
        }

        loop {
            if receiver.changed().await.is_err() {
                break;
            }

            let current = receiver.borrow_and_update().clone();

            if let Some(snapshot) = current.as_ref() {
                emit_state(&app_handle, MiniPlayerState::from(snapshot));
            }
        }

        println!("[mini-player] state bridge stopped");
    });

    println!("[mini-player] state bridge initialized");
}

fn emit_state(app: &tauri::AppHandle, state: MiniPlayerState) {
    if let Err(error) = app.emit_to(MINI_PLAYER_WINDOW_LABEL, MINI_PLAYER_STATE_EVENT, state) {
        eprintln!("[mini-player] failed to emit player state: {error}");
    }
}

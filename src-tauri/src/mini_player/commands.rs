use tauri::{AppHandle, State, WebviewWindow};

use crate::player::{dispatch_player_command, PlayerStore};

use super::{MiniPlayerCommand, MiniPlayerState, MINI_PLAYER_WINDOW_LABEL};

#[tauri::command]
pub fn get_mini_player_state(
    window: WebviewWindow,
    store: State<'_, PlayerStore>,
) -> Result<Option<MiniPlayerState>, String> {
    ensure_mini_player_window(&window)?;

    let receiver = store.subscribe();
    let snapshot = receiver.borrow().clone();

    Ok(snapshot.as_ref().map(MiniPlayerState::from))
}

#[tauri::command]
pub fn control_mini_player(
    window: WebviewWindow,
    app: AppHandle,
    command: MiniPlayerCommand,
) -> Result<(), String> {
    ensure_mini_player_window(&window)?;
    dispatch_player_command(&app, command.into_player_command()?)
}

fn ensure_mini_player_window(window: &WebviewWindow) -> Result<(), String> {
    ensure_mini_player_window_label(window.label())
}

fn ensure_mini_player_window_label(label: &str) -> Result<(), String> {
    if label == MINI_PLAYER_WINDOW_LABEL {
        Ok(())
    } else {
        Err("mini player commands are only available to the local Mini Player window".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_mini_player_window_label() {
        assert_eq!(ensure_mini_player_window_label("mini-player"), Ok(()));
        assert_eq!(
            ensure_mini_player_window_label("main"),
            Err(
                "mini player commands are only available to the local Mini Player window"
                    .to_string()
            )
        );
        assert!(ensure_mini_player_window_label("settings").is_err());
    }
}

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlayerCommand {
    Play,
    Pause,
    TogglePlayback,
    Stop,
    Next,
    Previous,
    Seek { position: f64 },
    SeekBy { offset: f64 },
}

impl PlayerCommand {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::Seek { position } if !position.is_finite() || *position < 0.0 => {
                Err("invalid seek position".into())
            }

            Self::SeekBy { offset } if !offset.is_finite() => Err("invalid seek offset".into()),

            _ => Ok(()),
        }
    }
}

/// Dispatches a playback command from the Rust core to the
/// YouTube Music WebView.
///
/// Native integrations such as tray controls and OS media
/// controls call this function directly.
pub fn dispatch_player_command(app: &AppHandle, command: PlayerCommand) -> Result<(), String> {
    command.validate()?;

    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| "main YouTube Music webview not found".to_string())?;

    let payload = serde_json::to_string(&command)
        .map_err(|error| format!("failed to serialize player command: {error}"))?;

    let script = format!(
        r#"
        (() => {{
            const api = window.__YTMDESKTOP__;

            if (!api) {{
                console.error(
                    "[YTMusic Desktop] playback adapter is not available"
                );
                return;
            }}

            Promise.resolve(
                api.executeNativeCommand({payload})
            ).catch((error) => {{
                console.error(
                    "[YTMusic Desktop] native player command failed:",
                    error
                );
            }});
        }})();
        "#
    );

    webview
        .eval(&script)
        .map_err(|error| format!("failed to execute player command: {error}"))
}

/// IPC wrapper used for development and future local UI.
///
/// Native integrations should prefer `dispatch_player_command`
/// directly instead of going through IPC.
#[tauri::command]
pub fn control_player(app: AppHandle, command: PlayerCommand) -> Result<(), String> {
    dispatch_player_command(&app, command)
}

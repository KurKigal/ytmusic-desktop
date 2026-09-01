use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlayerCommand {
    Play,
    Pause,
    TogglePlayback,
    Next,
    Previous,
    Seek {
        position: f64,
    },
}

impl PlayerCommand {
    fn validate(&self) -> Result<(), String> {
        if let Self::Seek { position } = self {
            if !position.is_finite() || *position < 0.0 {
                return Err("invalid seek position".into());
            }
        }

        Ok(())
    }
}

/// Dispatches a playback command from the Rust core to the
/// YouTube Music WebView.
///
/// Native integrations such as tray controls and MPRIS will call
/// this function directly in later milestones.
pub fn dispatch_player_command(
    app: &AppHandle,
    command: PlayerCommand,
) -> Result<(), String> {
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
pub fn control_player(
    app: AppHandle,
    command: PlayerCommand,
) -> Result<(), String> {
    dispatch_player_command(&app, command)
}
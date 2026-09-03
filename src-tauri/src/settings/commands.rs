use tauri::{AppHandle, State, WebviewWindow};

use crate::shortcuts::ShortcutManager;

use super::{AppSettings, SettingsStore, ShortcutAction};

const SETTINGS_WINDOW_LABEL: &str = "settings";

#[tauri::command]
pub fn get_settings(
    window: WebviewWindow,
    store: State<'_, SettingsStore>,
) -> Result<AppSettings, String> {
    ensure_settings_window(&window)?;
    Ok(store.snapshot())
}

#[tauri::command]
pub fn update_shortcut(
    window: WebviewWindow,
    app: AppHandle,
    store: State<'_, SettingsStore>,
    manager: State<'_, ShortcutManager>,
    action: ShortcutAction,
    shortcut: String,
) -> Result<AppSettings, String> {
    ensure_settings_window(&window)?;
    manager.update_shortcut(&app, store.inner(), action, shortcut)
}

#[tauri::command]
pub fn restore_default_shortcuts(
    window: WebviewWindow,
    app: AppHandle,
    store: State<'_, SettingsStore>,
    manager: State<'_, ShortcutManager>,
) -> Result<AppSettings, String> {
    ensure_settings_window(&window)?;
    manager.restore_defaults(&app, store.inner())
}

fn ensure_settings_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == SETTINGS_WINDOW_LABEL {
        Ok(())
    } else {
        Err("settings commands are only available to the local Settings window".to_string())
    }
}

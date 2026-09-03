use tauri::{AppHandle, State, WebviewWindow};

use crate::{runtime_settings::RuntimeSettings, shortcuts::ShortcutManager};

use super::{AppSettings, ApplicationSettings, SettingsStore, ShortcutAction};

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
pub fn update_application_settings(
    window: WebviewWindow,
    app: AppHandle,
    store: State<'_, SettingsStore>,
    runtime: State<'_, RuntimeSettings>,
    application: ApplicationSettings,
) -> Result<AppSettings, String> {
    ensure_settings_window(&window)?;
    let current = store.snapshot();

    if current.application == application {
        return Ok(current);
    }

    runtime.prepare_change(&app, &current.application, &application)?;

    let updated = match store.update(|settings| settings.application = application.clone()) {
        Ok(updated) => updated,
        Err(error) => {
            return Err(with_runtime_rollback(
                error,
                runtime.rollback_change(&app, &current.application, &application),
            ));
        }
    };

    runtime.apply_committed_change(&app, &current.application, &updated.application);
    Ok(updated)
}

#[tauri::command]
pub fn restore_defaults(
    window: WebviewWindow,
    app: AppHandle,
    store: State<'_, SettingsStore>,
    manager: State<'_, ShortcutManager>,
    runtime: State<'_, RuntimeSettings>,
) -> Result<AppSettings, String> {
    ensure_settings_window(&window)?;
    let current = store.snapshot();
    let default_application = ApplicationSettings::default();
    runtime.prepare_change(&app, &current.application, &default_application)?;

    let updated = match manager.restore_defaults(&app, store.inner()) {
        Ok(updated) => updated,
        Err(error) => {
            return Err(with_runtime_rollback(
                error,
                runtime.rollback_change(&app, &current.application, &default_application),
            ));
        }
    };

    runtime.apply_committed_change(&app, &current.application, &updated.application);
    Ok(updated)
}

fn ensure_settings_window(window: &WebviewWindow) -> Result<(), String> {
    ensure_settings_window_label(window.label())
}

fn ensure_settings_window_label(label: &str) -> Result<(), String> {
    if label == SETTINGS_WINDOW_LABEL {
        Ok(())
    } else {
        Err("settings commands are only available to the local Settings window".to_string())
    }
}

fn with_runtime_rollback(primary: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => primary,
        Err(rollback_error) => {
            format!("{primary}; restoring the previous runtime settings failed: {rollback_error}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_commands_accept_only_the_settings_window() {
        assert_eq!(ensure_settings_window_label("settings"), Ok(()));
        assert!(ensure_settings_window_label("main").is_err());
        assert!(ensure_settings_window_label("mini-player").is_err());
    }

    #[test]
    fn rollback_errors_retain_both_failure_reasons() {
        let message = with_runtime_rollback(
            "settings could not be saved".to_string(),
            Err("always-on-top rollback failed".to_string()),
        );

        assert!(message.contains("settings could not be saved"));
        assert!(message.contains("always-on-top rollback failed"));
    }
}

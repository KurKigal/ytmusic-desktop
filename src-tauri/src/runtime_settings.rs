use tauri::{AppHandle, Emitter, Manager};

use crate::{
    integrations::{update_tray_language, DiscordController},
    localization::{native_ui_strings, LOCAL_UI_LANGUAGE_CHANGED_EVENT},
    mini_player::MINI_PLAYER_WINDOW_LABEL,
    settings::{ApplicationSettings, Language},
};

pub struct RuntimeSettings {
    discord: DiscordController,
}

impl RuntimeSettings {
    pub fn new(discord: DiscordController) -> Self {
        Self { discord }
    }

    /// Applies the only fallible live window mutation before persistence, so a
    /// rejected always-on-top change cannot leave saved settings out of sync.
    pub fn prepare_change(
        &self,
        app: &AppHandle,
        current: &ApplicationSettings,
        updated: &ApplicationSettings,
    ) -> Result<(), String> {
        if current.mini_player_always_on_top != updated.mini_player_always_on_top {
            set_mini_player_always_on_top(app, updated.mini_player_always_on_top)?;
        }

        Ok(())
    }

    pub fn rollback_change(
        &self,
        app: &AppHandle,
        current: &ApplicationSettings,
        updated: &ApplicationSettings,
    ) -> Result<(), String> {
        if current.mini_player_always_on_top != updated.mini_player_always_on_top {
            set_mini_player_always_on_top(app, current.mini_player_always_on_top)?;
        }

        Ok(())
    }

    /// Applies non-fallible worker state and best-effort presentation updates
    /// after the new settings have been persisted.
    pub fn apply_committed_change(
        &self,
        app: &AppHandle,
        current: &ApplicationSettings,
        updated: &ApplicationSettings,
    ) {
        if current.discord_rich_presence_enabled != updated.discord_rich_presence_enabled {
            self.discord
                .set_enabled(updated.discord_rich_presence_enabled);
        }

        if current.language != updated.language {
            apply_local_ui_language(app, updated.language);
        }
    }
}

fn set_mini_player_always_on_top(app: &AppHandle, always_on_top: bool) -> Result<(), String> {
    let window = app
        .get_webview_window(MINI_PLAYER_WINDOW_LABEL)
        .ok_or_else(|| "mini player webview window not found".to_string())?;

    window
        .set_always_on_top(always_on_top)
        .map_err(|error| format!("failed to update mini player always-on-top state: {error}"))
}

fn apply_local_ui_language(app: &AppHandle, language: Language) {
    let strings = native_ui_strings(language);

    if let Err(error) = update_tray_language(app, language) {
        eprintln!("[settings] failed to update tray language: {error}");
    }

    if let Some(window) = app.get_webview_window("settings") {
        if let Err(error) = window.set_title(strings.settings_window_title) {
            eprintln!("[settings] failed to update Settings window title: {error}");
        }
    }

    if let Some(window) = app.get_webview_window(MINI_PLAYER_WINDOW_LABEL) {
        if let Err(error) = window.set_title(strings.mini_player_window_title) {
            eprintln!("[settings] failed to update Mini Player window title: {error}");
        }
    }

    if let Err(error) = app.emit_to(
        MINI_PLAYER_WINDOW_LABEL,
        LOCAL_UI_LANGUAGE_CHANGED_EVENT,
        language,
    ) {
        eprintln!("[settings] failed to publish local UI language: {error}");
    }
}

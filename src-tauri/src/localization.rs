use tauri::{State, WebviewWindow};

use crate::{
    mini_player::MINI_PLAYER_WINDOW_LABEL,
    settings::{Language, SettingsStore},
};

pub const LOCAL_UI_LANGUAGE_CHANGED_EVENT: &str = "local-ui-language-changed";

pub struct NativeUiStrings {
    pub open: &'static str,
    pub settings: &'static str,
    pub mini_player: &'static str,
    pub play_pause: &'static str,
    pub previous: &'static str,
    pub next: &'static str,
    pub quit: &'static str,
    pub settings_window_title: &'static str,
    pub mini_player_window_title: &'static str,
}

pub const fn native_ui_strings(language: Language) -> NativeUiStrings {
    match language {
        Language::English => NativeUiStrings {
            open: "Open YTMusic Desktop",
            settings: "Settings",
            mini_player: "Mini Player",
            play_pause: "Play / Pause",
            previous: "Previous",
            next: "Next",
            quit: "Quit",
            settings_window_title: "YTMusic Desktop Settings",
            mini_player_window_title: "YTMusic Desktop Mini Player",
        },
        Language::Turkish => NativeUiStrings {
            open: "YTMusic Desktop'i Aç",
            settings: "Ayarlar",
            mini_player: "Mini Oynatıcı",
            play_pause: "Oynat / Duraklat",
            previous: "Önceki",
            next: "Sonraki",
            quit: "Çıkış",
            settings_window_title: "YTMusic Desktop Ayarları",
            mini_player_window_title: "YTMusic Desktop Mini Oynatıcı",
        },
    }
}

#[tauri::command]
pub fn get_local_ui_language(
    window: WebviewWindow,
    store: State<'_, SettingsStore>,
) -> Result<Language, String> {
    ensure_mini_player_window_label(window.label())?;
    Ok(store.snapshot().application.language)
}

fn ensure_mini_player_window_label(label: &str) -> Result<(), String> {
    if label == MINI_PLAYER_WINDOW_LABEL {
        Ok(())
    } else {
        Err("local UI language access is only available to the Mini Player window".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_strings_cover_both_supported_languages() {
        assert_eq!(native_ui_strings(Language::English).settings, "Settings");
        assert_eq!(native_ui_strings(Language::Turkish).settings, "Ayarlar");
        assert_eq!(native_ui_strings(Language::Turkish).quit, "Çıkış");
    }

    #[test]
    fn language_command_accepts_only_the_mini_player_window() {
        assert_eq!(ensure_mini_player_window_label("mini-player"), Ok(()));
        assert!(ensure_mini_player_window_label("settings").is_err());
        assert!(ensure_mini_player_window_label("main").is_err());
    }
}

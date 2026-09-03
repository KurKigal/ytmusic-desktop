use std::collections::HashMap;

use serde::{Deserialize, Serialize};

const MAX_SHORTCUT_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShortcutAction {
    PlayPause,
    Next,
    Previous,
    SeekForward10,
    SeekBackward10,
}

impl ShortcutAction {
    pub const ALL: [Self; 5] = [
        Self::PlayPause,
        Self::Next,
        Self::Previous,
        Self::SeekForward10,
        Self::SeekBackward10,
    ];
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ShortcutSettings {
    pub play_pause: String,
    pub next: String,
    pub previous: String,
    pub seek_forward_10: String,
    pub seek_backward_10: String,
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            play_pause: "Ctrl+Alt+Space".to_string(),
            next: "Ctrl+Alt+Right".to_string(),
            previous: "Ctrl+Alt+Left".to_string(),
            seek_forward_10: "Ctrl+Alt+Shift+Right".to_string(),
            seek_backward_10: "Ctrl+Alt+Shift+Left".to_string(),
        }
    }
}

impl ShortcutSettings {
    pub fn get(&self, action: ShortcutAction) -> &str {
        match action {
            ShortcutAction::PlayPause => &self.play_pause,
            ShortcutAction::Next => &self.next,
            ShortcutAction::Previous => &self.previous,
            ShortcutAction::SeekForward10 => &self.seek_forward_10,
            ShortcutAction::SeekBackward10 => &self.seek_backward_10,
        }
    }

    pub fn set(&mut self, action: ShortcutAction, shortcut: String) {
        match action {
            ShortcutAction::PlayPause => self.play_pause = shortcut,
            ShortcutAction::Next => self.next = shortcut,
            ShortcutAction::Previous => self.previous = shortcut,
            ShortcutAction::SeekForward10 => self.seek_forward_10 = shortcut,
            ShortcutAction::SeekBackward10 => self.seek_backward_10 = shortcut,
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (ShortcutAction, &str)> {
        ShortcutAction::ALL
            .into_iter()
            .map(move |action| (action, self.get(action)))
    }

    pub fn validate(&self) -> Result<(), String> {
        let mut assigned = HashMap::new();

        for (action, shortcut) in self.iter() {
            let trimmed = shortcut.trim();

            if trimmed.is_empty() {
                return Err(format!("{} shortcut cannot be empty", action.field_name()));
            }

            if shortcut.len() > MAX_SHORTCUT_LENGTH {
                return Err(format!(
                    "{} shortcut exceeds the maximum length",
                    action.field_name()
                ));
            }

            let normalized = normalize_for_duplicate_detection(trimmed);

            if let Some(previous) = assigned.insert(normalized, action) {
                return Err(format!(
                    "{} and {} use the same shortcut",
                    previous.field_name(),
                    action.field_name()
                ));
            }
        }

        Ok(())
    }
}

impl ShortcutAction {
    const fn field_name(self) -> &'static str {
        match self {
            Self::PlayPause => "play/pause",
            Self::Next => "next",
            Self::Previous => "previous",
            Self::SeekForward10 => "seek forward",
            Self::SeekBackward10 => "seek backward",
        }
    }
}

fn normalize_for_duplicate_detection(shortcut: &str) -> String {
    let mut parts = shortcut
        .split('+')
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();

    parts.sort_unstable();
    parts.join("+")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct AppSettings {
    pub shortcuts: ShortcutSettings,
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        self.shortcuts.validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_documented_shortcuts() {
        let settings = AppSettings::default();

        assert_eq!(settings.shortcuts.play_pause, "Ctrl+Alt+Space");
        assert_eq!(settings.shortcuts.next, "Ctrl+Alt+Right");
        assert_eq!(settings.shortcuts.previous, "Ctrl+Alt+Left");
        assert_eq!(settings.shortcuts.seek_forward_10, "Ctrl+Alt+Shift+Right");
        assert_eq!(settings.shortcuts.seek_backward_10, "Ctrl+Alt+Shift+Left");
        assert_eq!(settings.validate(), Ok(()));
    }

    #[test]
    fn settings_use_camel_case_json_fields() {
        let serialized = serde_json::to_value(AppSettings::default())
            .expect("default settings should serialize");

        assert_eq!(serialized["shortcuts"]["playPause"], "Ctrl+Alt+Space");
        assert_eq!(
            serialized["shortcuts"]["seekForward10"],
            "Ctrl+Alt+Shift+Right"
        );
    }

    #[test]
    fn shortcut_actions_use_the_settings_ui_identifiers() {
        let serialized = ShortcutAction::ALL
            .into_iter()
            .map(|action| serde_json::to_value(action).expect("shortcut action should serialize"))
            .collect::<Vec<_>>();

        assert_eq!(
            serialized,
            vec![
                "playPause",
                "next",
                "previous",
                "seekForward10",
                "seekBackward10"
            ]
        );
    }

    #[test]
    fn missing_json_fields_receive_defaults() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({
            "shortcuts": {
                "playPause": "Ctrl+Shift+P"
            }
        }))
        .expect("partial settings should deserialize");

        assert_eq!(settings.shortcuts.play_pause, "Ctrl+Shift+P");
        assert_eq!(settings.shortcuts.next, "Ctrl+Alt+Right");
    }

    #[test]
    fn rejects_empty_and_overlong_shortcuts() {
        let mut settings = AppSettings::default();
        settings.shortcuts.next = "  ".to_string();

        assert_eq!(
            settings.validate(),
            Err("next shortcut cannot be empty".to_string())
        );

        settings.shortcuts.next = "x".repeat(MAX_SHORTCUT_LENGTH + 1);

        assert_eq!(
            settings.validate(),
            Err("next shortcut exceeds the maximum length".to_string())
        );
    }

    #[test]
    fn rejects_duplicates_ignoring_case_spacing_and_modifier_order() {
        let mut settings = AppSettings::default();
        settings.shortcuts.next = "CTRL + alt + right".to_string();
        settings.shortcuts.previous = "Right+Alt+Ctrl".to_string();

        assert_eq!(
            settings.validate(),
            Err("next and previous use the same shortcut".to_string())
        );
    }

    #[test]
    fn get_set_and_iter_cover_every_action() {
        let mut shortcuts = ShortcutSettings::default();

        for action in ShortcutAction::ALL {
            let value = format!("Ctrl+Shift+{:?}", action);
            shortcuts.set(action, value.clone());
            assert_eq!(shortcuts.get(action), value);
        }

        assert_eq!(shortcuts.iter().count(), ShortcutAction::ALL.len());
    }
}

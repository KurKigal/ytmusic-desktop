use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::RwLock,
};

use super::{AppSettings, ShortcutSettings};

pub struct SettingsStore {
    path: PathBuf,
    settings: RwLock<AppSettings>,
}

impl SettingsStore {
    #[cfg(test)]
    pub fn load(path: PathBuf) -> Self {
        Self::load_with_diagnostics(path).0
    }

    pub fn load_with_diagnostics(path: PathBuf) -> (Self, Option<String>) {
        let (settings, warning) = match fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<AppSettings>(&contents) {
                Ok(settings) => match settings.validate() {
                    Ok(()) => (settings, None),
                    Err(error) => (
                        AppSettings::default(),
                        Some(format!("settings are invalid; using defaults: {error}")),
                    ),
                },
                Err(error) => (
                    AppSettings::default(),
                    Some(format!("settings JSON is invalid; using defaults: {error}")),
                ),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => (AppSettings::default(), None),
            Err(error) => (
                AppSettings::default(),
                Some(format!("failed to read settings; using defaults: {error}")),
            ),
        };

        (
            Self {
                path,
                settings: RwLock::new(settings),
            },
            warning,
        )
    }

    pub fn snapshot(&self) -> AppSettings {
        self.settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn replace(&self, settings: AppSettings) -> Result<(), String> {
        settings.validate()?;

        let mut current = self
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        persist(&self.path, &settings)?;
        *current = settings;

        Ok(())
    }

    /// Resets the in-memory snapshot even when the invalid settings file
    /// cannot be replaced. This is reserved for startup recovery so the app
    /// never continues with shortcut data that failed semantic validation.
    #[cfg(test)]
    pub fn recover_defaults(&self) -> Result<(), String> {
        let defaults = AppSettings::default();
        let mut current = self
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        *current = defaults.clone();
        persist(&self.path, &defaults)
    }

    /// Repairs only shortcut data that fails platform-level validation while
    /// preserving otherwise valid application preferences.
    pub fn recover_shortcut_defaults(&self) -> Result<(), String> {
        let mut current = self
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut recovered = current.clone();
        recovered.shortcuts = ShortcutSettings::default();

        *current = recovered.clone();
        persist(&self.path, &recovered)
    }

    pub fn update<F>(&self, update: F) -> Result<AppSettings, String>
    where
        F: FnOnce(&mut AppSettings),
    {
        let mut current = self
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut updated = current.clone();

        update(&mut updated);
        updated.validate()?;
        persist(&self.path, &updated)?;
        *current = updated.clone();

        Ok(updated)
    }
}

fn persist(path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create settings directory: {error}"))?;
    }

    let serialized = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("failed to serialize settings: {error}"))?;
    let temporary_path = temporary_path(path);

    let write_result = (|| -> io::Result<()> {
        let mut file = fs::File::create(&temporary_path)?;
        file.write_all(&serialized)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("failed to write settings: {error}"));
    }

    if let Err(error) = replace_file(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(format!("failed to replace settings file: {error}"));
    }

    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "settings.json".into());
    file_name.push(".tmp");

    path.with_file_name(file_name)
}

fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_error) if destination.exists() => {
            let backup = backup_path(destination);
            let _ = fs::remove_file(&backup);
            fs::rename(destination, &backup)?;

            match fs::rename(source, destination) {
                Ok(()) => {
                    let _ = fs::remove_file(backup);
                    Ok(())
                }
                Err(replacement_error) => {
                    let _ = fs::rename(&backup, destination);
                    Err(replacement_error)
                }
            }
        }
        Err(error) => Err(error),
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "settings.json".into());
    file_name.push(".backup");

    path.with_file_name(file_name)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should follow the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ytmdesktop-settings-{name}-{}-{unique}",
                std::process::id()
            ));

            fs::create_dir_all(&path).expect("test directory should be created");

            Self(path)
        }

        fn settings_path(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn missing_file_loads_defaults_without_a_warning() {
        let directory = TestDirectory::new("missing");
        let (store, warning) = SettingsStore::load_with_diagnostics(directory.settings_path());

        assert_eq!(store.snapshot(), AppSettings::default());
        assert_eq!(warning, None);
    }

    #[test]
    fn replace_persists_and_load_roundtrips_settings() {
        let directory = TestDirectory::new("roundtrip");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());
        let mut settings = store.snapshot();
        settings.shortcuts.play_pause = "Ctrl+Shift+P".to_string();

        store
            .replace(settings.clone())
            .expect("valid settings should persist");

        settings.shortcuts.next = "Ctrl+Shift+N".to_string();
        store
            .replace(settings.clone())
            .expect("a second valid replacement should persist");

        let (reloaded, warning) = SettingsStore::load_with_diagnostics(path);

        assert_eq!(reloaded.snapshot(), settings);
        assert_eq!(warning, None);
    }

    #[test]
    fn corrupt_json_loads_defaults_with_a_warning() {
        let directory = TestDirectory::new("corrupt");
        let path = directory.settings_path();
        fs::write(&path, "{ definitely not JSON")
            .expect("corrupt settings fixture should be written");

        let (store, warning) = SettingsStore::load_with_diagnostics(path);

        assert_eq!(store.snapshot(), AppSettings::default());
        assert!(warning
            .as_deref()
            .is_some_and(|message| message.contains("settings JSON is invalid")));
    }

    #[test]
    fn invalid_settings_load_defaults_with_a_warning() {
        let directory = TestDirectory::new("invalid");
        let path = directory.settings_path();
        let mut settings = AppSettings::default();
        settings.shortcuts.previous = settings.shortcuts.next.clone();
        fs::write(
            &path,
            serde_json::to_vec(&settings).expect("invalid fixture should serialize"),
        )
        .expect("invalid settings fixture should be written");

        let (store, warning) = SettingsStore::load_with_diagnostics(path);

        assert_eq!(store.snapshot(), AppSettings::default());
        assert!(warning
            .as_deref()
            .is_some_and(|message| message.contains("settings are invalid")));
    }

    #[test]
    fn unsupported_schema_version_loads_defaults_with_a_warning() {
        let directory = TestDirectory::new("unsupported-schema");
        let path = directory.settings_path();
        let mut settings = AppSettings::default();
        settings.schema_version += 1;
        fs::write(
            &path,
            serde_json::to_vec(&settings).expect("unsupported schema fixture should serialize"),
        )
        .expect("unsupported schema fixture should be written");

        let (store, warning) = SettingsStore::load_with_diagnostics(path);

        assert_eq!(store.snapshot(), AppSettings::default());
        assert!(warning
            .as_deref()
            .is_some_and(|message| message.contains("unsupported settings schema version")));
    }

    #[test]
    fn rejected_replacement_does_not_change_memory_or_disk() {
        let directory = TestDirectory::new("reject");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());
        let expected = store.snapshot();

        store
            .replace(expected.clone())
            .expect("defaults should persist");

        let mut invalid = expected.clone();
        invalid.shortcuts.previous = invalid.shortcuts.next.clone();

        assert!(store.replace(invalid).is_err());
        assert_eq!(store.snapshot(), expected);
        assert_eq!(SettingsStore::load(path).snapshot(), expected);
    }

    #[test]
    fn persistence_failure_does_not_change_the_in_memory_snapshot() {
        let directory = TestDirectory::new("write-failure");
        let blocked_parent = directory.0.join("not-a-directory");
        fs::write(&blocked_parent, "blocking file")
            .expect("blocking file fixture should be written");
        let store = SettingsStore::load(blocked_parent.join("settings.json"));
        let expected = store.snapshot();
        let mut changed = expected.clone();
        changed.shortcuts.play_pause = "Ctrl+Shift+P".to_string();

        assert!(store.replace(changed).is_err());
        assert_eq!(store.snapshot(), expected);
    }

    #[test]
    fn update_validates_and_persists_the_changed_snapshot() {
        let directory = TestDirectory::new("update");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());

        let updated = store
            .update(|settings| {
                settings.shortcuts.seek_forward_10 = "Ctrl+Shift+Period".to_string();
            })
            .expect("valid update should persist");

        assert_eq!(updated.shortcuts.seek_forward_10, "Ctrl+Shift+Period");
        assert_eq!(SettingsStore::load(path).snapshot(), updated);
    }

    #[test]
    fn application_update_preserves_shortcut_settings() {
        let directory = TestDirectory::new("application-update");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());

        store
            .update(|settings| {
                settings.shortcuts.play_pause = "Ctrl+Shift+P".to_string();
            })
            .expect("custom shortcut should persist");
        let shortcuts = store.snapshot().shortcuts;

        let updated = store
            .update(|settings| {
                settings.application.start_minimized = true;
                settings.application.close_to_tray = false;
            })
            .expect("application settings should persist");

        assert_eq!(updated.shortcuts, shortcuts);
        assert!(updated.application.start_minimized);
        assert!(!updated.application.close_to_tray);
        assert_eq!(SettingsStore::load(path).snapshot(), updated);
    }

    #[test]
    fn rejected_update_does_not_change_memory_or_disk() {
        let directory = TestDirectory::new("rejected-update");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());
        let expected = store.snapshot();

        store
            .replace(expected.clone())
            .expect("defaults should persist");

        let error = store
            .update(|settings| settings.schema_version += 1)
            .expect_err("unsupported schema update should be rejected");

        assert!(error.contains("unsupported settings schema version"));
        assert_eq!(store.snapshot(), expected);
        assert_eq!(SettingsStore::load(path).snapshot(), expected);
    }

    #[test]
    fn recovery_resets_memory_and_disk_to_defaults() {
        let directory = TestDirectory::new("recovery");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());

        store
            .update(|settings| {
                settings.shortcuts.play_pause = "Ctrl+Shift+P".to_string();
            })
            .expect("custom settings should persist");

        store
            .recover_defaults()
            .expect("defaults should persist during recovery");

        assert_eq!(store.snapshot(), AppSettings::default());
        assert_eq!(SettingsStore::load(path).snapshot(), AppSettings::default());
    }

    #[test]
    fn shortcut_recovery_preserves_application_settings() {
        let directory = TestDirectory::new("shortcut-recovery");
        let path = directory.settings_path();
        let store = SettingsStore::load(path.clone());

        store
            .update(|settings| {
                settings.application.language = crate::settings::Language::Turkish;
                settings.application.start_minimized = true;
                settings.shortcuts.play_pause = "Ctrl+Shift+P".to_string();
            })
            .expect("custom settings should persist");

        store
            .recover_shortcut_defaults()
            .expect("shortcut defaults should persist during recovery");

        let recovered = SettingsStore::load(path).snapshot();
        assert_eq!(recovered.shortcuts, ShortcutSettings::default());
        assert_eq!(
            recovered.application.language,
            crate::settings::Language::Turkish
        );
        assert!(recovered.application.start_minimized);
    }
}

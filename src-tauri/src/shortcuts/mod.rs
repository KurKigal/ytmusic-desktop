use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

use crate::{
    player::{dispatch_player_command, PlayerCommand},
    settings::{AppSettings, SettingsStore, ShortcutAction, ShortcutSettings},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRegistrationFailure {
    pub action: ShortcutAction,
    pub shortcut: String,
    pub error: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StartupRegistrationReport {
    pub registered: usize,
    pub failures: Vec<ShortcutRegistrationFailure>,
}

#[derive(Debug, Clone)]
struct RegisteredBinding {
    text: String,
    shortcut: Shortcut,
}

#[derive(Debug, Default)]
struct ShortcutManagerState {
    registrations: HashMap<ShortcutAction, RegisteredBinding>,
}

#[derive(Debug, Default)]
pub struct ShortcutManager {
    state: Mutex<ShortcutManagerState>,
}

impl ShortcutManager {
    /// Registers each configured shortcut independently. A conflict or invalid
    /// entry is reported without preventing the remaining shortcuts or app
    /// startup from continuing.
    pub fn register_startup(
        &self,
        app: &AppHandle,
        shortcuts: &ShortcutSettings,
    ) -> StartupRegistrationReport {
        self.register_startup_with(&TauriRegistrationBackend { app }, shortcuts)
    }

    /// Replaces one shortcut and persists it only after OS registration
    /// succeeds. Any failure after the old shortcut is removed attempts to put
    /// that exact registration back before returning an error.
    pub fn update_shortcut(
        &self,
        app: &AppHandle,
        store: &SettingsStore,
        action: ShortcutAction,
        shortcut: String,
    ) -> Result<AppSettings, String> {
        self.update_shortcut_with(&TauriRegistrationBackend { app }, store, action, shortcut)
    }

    /// Replaces all configured registrations with the defaults as one
    /// operation. If any default cannot be registered, every successfully
    /// removed previous registration is restored and settings stay unchanged.
    pub fn restore_defaults(
        &self,
        app: &AppHandle,
        store: &SettingsStore,
    ) -> Result<AppSettings, String> {
        self.restore_defaults_with(&TauriRegistrationBackend { app }, store)
    }

    fn state(&self) -> MutexGuard<'_, ShortcutManagerState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn register_startup_with<B: RegistrationBackend>(
        &self,
        backend: &B,
        shortcuts: &ShortcutSettings,
    ) -> StartupRegistrationReport {
        let mut state = self.state();
        let mut report = StartupRegistrationReport::default();
        let mut assigned = HashMap::new();

        for action in ShortcutAction::ALL {
            let text = shortcuts.get(action).trim().to_string();
            let shortcut = match parse_shortcut(action, &text) {
                Ok(shortcut) => shortcut,
                Err(error) => {
                    report.failures.push(ShortcutRegistrationFailure {
                        action,
                        shortcut: text,
                        error,
                    });
                    continue;
                }
            };

            if let Some(previous) = assigned.insert(shortcut.id(), action) {
                report.failures.push(ShortcutRegistrationFailure {
                    action,
                    shortcut: text,
                    error: duplicate_error(previous, action),
                });
                continue;
            }

            match backend.register(action, shortcut) {
                Ok(()) => {
                    state
                        .registrations
                        .insert(action, RegisteredBinding { text, shortcut });
                    report.registered += 1;
                }
                Err(error) => report.failures.push(ShortcutRegistrationFailure {
                    action,
                    shortcut: text,
                    error: format!(
                        "failed to register {} shortcut: {error}",
                        action_name(action)
                    ),
                }),
            }
        }

        report
    }

    fn update_shortcut_with<B, P>(
        &self,
        backend: &B,
        persistence: &P,
        action: ShortcutAction,
        shortcut: String,
    ) -> Result<AppSettings, String>
    where
        B: RegistrationBackend,
        P: SettingsPersistence,
    {
        let mut state = self.state();
        let current = persistence.snapshot();
        let mut updated = current.clone();
        let text = shortcut.trim().to_string();
        updated.shortcuts.set(action, text.clone());
        validate_shortcut_settings(&updated.shortcuts)?;

        let new_binding = RegisteredBinding {
            shortcut: parse_shortcut(action, &text)?,
            text,
        };
        let old_binding = state.registrations.remove(&action);

        if let Some(old_binding) = old_binding.as_ref() {
            if let Err(error) = backend.unregister(old_binding.shortcut) {
                state.registrations.insert(action, old_binding.clone());
                return Err(format!(
                    "failed to unregister the previous {} shortcut: {error}",
                    action_name(action)
                ));
            }
        }

        if let Err(error) = backend.register(action, new_binding.shortcut) {
            return Err(registration_failure_with_rollback(
                backend,
                &mut state,
                action,
                old_binding,
                error,
            ));
        }

        state.registrations.insert(action, new_binding.clone());

        if let Err(error) = persistence.replace(updated.clone()) {
            return Err(persistence_failure_with_rollback(
                backend,
                &mut state,
                action,
                &new_binding,
                old_binding,
                error,
            ));
        }

        Ok(updated)
    }

    fn restore_defaults_with<B, P>(
        &self,
        backend: &B,
        persistence: &P,
    ) -> Result<AppSettings, String>
    where
        B: RegistrationBackend,
        P: SettingsPersistence,
    {
        let mut state = self.state();
        let current = persistence.snapshot();
        let mut defaults = current.clone();
        defaults.shortcuts = ShortcutSettings::default();
        let default_bindings = validated_bindings(&defaults.shortcuts)?;
        let previous_bindings = state.registrations.clone();
        let mut removed_previous = Vec::new();

        for action in ShortcutAction::ALL {
            let Some(binding) = previous_bindings.get(&action) else {
                continue;
            };

            if let Err(error) = backend.unregister(binding.shortcut) {
                let rollback_errors = restore_bindings(backend, &mut state, removed_previous);
                return Err(with_rollback_errors(
                    format!(
                        "failed to unregister the previous {} shortcut: {error}",
                        action_name(action)
                    ),
                    rollback_errors,
                ));
            }

            state.registrations.remove(&action);
            removed_previous.push((action, binding.clone()));
        }

        let mut registered_defaults = Vec::new();
        for (action, binding) in default_bindings {
            if let Err(error) = backend.register(action, binding.shortcut) {
                let mut rollback_errors =
                    remove_bindings(backend, &mut state, registered_defaults.into_iter().rev());
                rollback_errors.extend(restore_bindings(backend, &mut state, previous_bindings));
                return Err(with_rollback_errors(
                    format!(
                        "failed to register the default {} shortcut `{}`: {error}",
                        action_name(action),
                        binding.text
                    ),
                    rollback_errors,
                ));
            }

            state.registrations.insert(action, binding.clone());
            registered_defaults.push((action, binding));
        }

        if let Err(error) = persistence.replace(defaults.clone()) {
            let mut rollback_errors =
                remove_bindings(backend, &mut state, registered_defaults.into_iter().rev());
            rollback_errors.extend(restore_bindings(backend, &mut state, previous_bindings));
            return Err(with_rollback_errors(
                format!("failed to persist default shortcuts: {error}"),
                rollback_errors,
            ));
        }

        Ok(defaults)
    }
}

pub fn validate_shortcut_settings(shortcuts: &ShortcutSettings) -> Result<(), String> {
    validated_bindings(shortcuts).map(|_| ())
}

fn validated_bindings(
    shortcuts: &ShortcutSettings,
) -> Result<Vec<(ShortcutAction, RegisteredBinding)>, String> {
    shortcuts.validate()?;

    let mut assigned = HashMap::new();
    let mut bindings = Vec::with_capacity(ShortcutAction::ALL.len());

    for action in ShortcutAction::ALL {
        let text = shortcuts.get(action).trim().to_string();
        let shortcut = parse_shortcut(action, &text)?;

        if let Some(previous) = assigned.insert(shortcut.id(), action) {
            return Err(duplicate_error(previous, action));
        }

        bindings.push((action, RegisteredBinding { text, shortcut }));
    }

    Ok(bindings)
}

fn parse_shortcut(action: ShortcutAction, shortcut: &str) -> Result<Shortcut, String> {
    let trimmed = shortcut.trim();
    let parsed = trimmed.parse::<Shortcut>().map_err(|error| {
        format!(
            "invalid {} shortcut `{trimmed}`: {error}",
            action_name(action)
        )
    })?;

    if is_hardware_media_key(parsed.key) {
        return Err(format!(
            "{} shortcut cannot use a hardware media key because native media controls already handle it",
            action_name(action)
        ));
    }

    Ok(parsed)
}

fn is_hardware_media_key(key: Code) -> bool {
    matches!(
        key,
        Code::MediaFastForward
            | Code::MediaPause
            | Code::MediaPlay
            | Code::MediaPlayPause
            | Code::MediaRewind
            | Code::MediaStop
            | Code::MediaTrackNext
            | Code::MediaTrackPrevious
    )
}

fn duplicate_error(first: ShortcutAction, second: ShortcutAction) -> String {
    format!(
        "{} and {} use the same shortcut",
        action_name(first),
        action_name(second)
    )
}

const fn action_name(action: ShortcutAction) -> &'static str {
    match action {
        ShortcutAction::PlayPause => "play/pause",
        ShortcutAction::Next => "next",
        ShortcutAction::Previous => "previous",
        ShortcutAction::SeekForward10 => "seek forward",
        ShortcutAction::SeekBackward10 => "seek backward",
    }
}

fn player_command(action: ShortcutAction) -> PlayerCommand {
    match action {
        ShortcutAction::PlayPause => PlayerCommand::TogglePlayback,
        ShortcutAction::Next => PlayerCommand::Next,
        ShortcutAction::Previous => PlayerCommand::Previous,
        ShortcutAction::SeekForward10 => PlayerCommand::SeekBy { offset: 10.0 },
        ShortcutAction::SeekBackward10 => PlayerCommand::SeekBy { offset: -10.0 },
    }
}

trait RegistrationBackend {
    fn register(&self, action: ShortcutAction, shortcut: Shortcut) -> Result<(), String>;
    fn unregister(&self, shortcut: Shortcut) -> Result<(), String>;
}

struct TauriRegistrationBackend<'a> {
    app: &'a AppHandle,
}

impl RegistrationBackend for TauriRegistrationBackend<'_> {
    fn register(&self, action: ShortcutAction, shortcut: Shortcut) -> Result<(), String> {
        self.app
            .global_shortcut()
            .on_shortcut(shortcut, move |app, _, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }

                if let Err(error) = dispatch_player_command(app, player_command(action)) {
                    eprintln!(
                        "[shortcuts] {} command failed: {error}",
                        action_name(action)
                    );
                }
            })
            .map_err(|error| error.to_string())
    }

    fn unregister(&self, shortcut: Shortcut) -> Result<(), String> {
        self.app
            .global_shortcut()
            .unregister(shortcut)
            .map_err(|error| error.to_string())
    }
}

trait SettingsPersistence {
    fn snapshot(&self) -> AppSettings;
    fn replace(&self, settings: AppSettings) -> Result<(), String>;
}

impl SettingsPersistence for SettingsStore {
    fn snapshot(&self) -> AppSettings {
        SettingsStore::snapshot(self)
    }

    fn replace(&self, settings: AppSettings) -> Result<(), String> {
        SettingsStore::replace(self, settings)
    }
}

fn registration_failure_with_rollback<B: RegistrationBackend>(
    backend: &B,
    state: &mut ShortcutManagerState,
    action: ShortcutAction,
    old_binding: Option<RegisteredBinding>,
    error: String,
) -> String {
    let primary = format!(
        "failed to register the new {} shortcut: {error}",
        action_name(action)
    );

    let Some(old_binding) = old_binding else {
        return primary;
    };

    match backend.register(action, old_binding.shortcut) {
        Ok(()) => {
            state.registrations.insert(action, old_binding);
            format!("{primary}; the previous shortcut was restored")
        }
        Err(rollback_error) => {
            format!("{primary}; restoring the previous shortcut also failed: {rollback_error}")
        }
    }
}

fn persistence_failure_with_rollback<B: RegistrationBackend>(
    backend: &B,
    state: &mut ShortcutManagerState,
    action: ShortcutAction,
    new_binding: &RegisteredBinding,
    old_binding: Option<RegisteredBinding>,
    error: String,
) -> String {
    let primary = format!("failed to persist shortcut settings: {error}");

    if let Err(rollback_error) = backend.unregister(new_binding.shortcut) {
        return format!(
            "{primary}; unregistering the new shortcut during rollback also failed: {rollback_error}"
        );
    }

    state.registrations.remove(&action);

    let Some(old_binding) = old_binding else {
        return primary;
    };

    match backend.register(action, old_binding.shortcut) {
        Ok(()) => {
            state.registrations.insert(action, old_binding);
            format!("{primary}; the previous shortcut was restored")
        }
        Err(rollback_error) => {
            format!("{primary}; restoring the previous shortcut also failed: {rollback_error}")
        }
    }
}

fn remove_bindings<B, I>(backend: &B, state: &mut ShortcutManagerState, bindings: I) -> Vec<String>
where
    B: RegistrationBackend,
    I: IntoIterator<Item = (ShortcutAction, RegisteredBinding)>,
{
    let mut errors = Vec::new();

    for (action, binding) in bindings {
        match backend.unregister(binding.shortcut) {
            Ok(()) => {
                state.registrations.remove(&action);
            }
            Err(error) => errors.push(format!(
                "failed to unregister {} shortcut `{}` during rollback: {error}",
                action_name(action),
                binding.text
            )),
        }
    }

    errors
}

fn restore_bindings<B, I>(backend: &B, state: &mut ShortcutManagerState, bindings: I) -> Vec<String>
where
    B: RegistrationBackend,
    I: IntoIterator<Item = (ShortcutAction, RegisteredBinding)>,
{
    let mut errors = Vec::new();

    for (action, binding) in bindings {
        match backend.register(action, binding.shortcut) {
            Ok(()) => {
                state.registrations.insert(action, binding);
            }
            Err(error) => errors.push(format!(
                "failed to restore {} shortcut `{}`: {error}",
                action_name(action),
                binding.text
            )),
        }
    }

    errors
}

fn with_rollback_errors(primary: String, rollback_errors: Vec<String>) -> String {
    if rollback_errors.is_empty() {
        format!("{primary}; the previous shortcuts were restored")
    } else {
        format!(
            "{primary}; rollback was incomplete: {}",
            rollback_errors.join("; ")
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };

    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        registrations: RefCell<HashMap<u32, ShortcutAction>>,
        fail_registration: Cell<Option<u32>>,
    }

    impl FakeBackend {
        fn fail_registration_for(&self, shortcut: &str) {
            self.fail_registration.set(Some(
                shortcut
                    .parse::<Shortcut>()
                    .expect("test shortcut should parse")
                    .id(),
            ));
        }

        fn is_registered(&self, shortcut: &str) -> bool {
            let shortcut = shortcut
                .parse::<Shortcut>()
                .expect("test shortcut should parse");
            self.registrations.borrow().contains_key(&shortcut.id())
        }
    }

    impl RegistrationBackend for FakeBackend {
        fn register(&self, action: ShortcutAction, shortcut: Shortcut) -> Result<(), String> {
            if self.fail_registration.get() == Some(shortcut.id()) {
                return Err("shortcut is already in use".to_string());
            }

            if self.registrations.borrow().contains_key(&shortcut.id()) {
                return Err("shortcut is already registered".to_string());
            }

            self.registrations
                .borrow_mut()
                .insert(shortcut.id(), action);

            Ok(())
        }

        fn unregister(&self, shortcut: Shortcut) -> Result<(), String> {
            if self
                .registrations
                .borrow_mut()
                .remove(&shortcut.id())
                .is_some()
            {
                Ok(())
            } else {
                Err("shortcut is not registered".to_string())
            }
        }
    }

    struct FakePersistence {
        settings: RefCell<AppSettings>,
        fail_replace: Cell<bool>,
    }

    impl FakePersistence {
        fn new(settings: AppSettings) -> Self {
            Self {
                settings: RefCell::new(settings),
                fail_replace: Cell::new(false),
            }
        }
    }

    impl SettingsPersistence for FakePersistence {
        fn snapshot(&self) -> AppSettings {
            self.settings.borrow().clone()
        }

        fn replace(&self, settings: AppSettings) -> Result<(), String> {
            if self.fail_replace.get() {
                Err("disk is read-only".to_string())
            } else {
                self.settings.replace(settings);
                Ok(())
            }
        }
    }

    #[test]
    fn defaults_use_valid_non_media_shortcuts() {
        assert_eq!(
            validate_shortcut_settings(&ShortcutSettings::default()),
            Ok(())
        );
    }

    #[test]
    fn rejects_invalid_syntax() {
        let error = parse_shortcut(ShortcutAction::PlayPause, "Ctrl+DefinitelyNotAKey")
            .expect_err("unknown keys should be rejected");

        assert!(error.contains("invalid play/pause shortcut"));
    }

    #[test]
    fn rejects_hardware_media_keys() {
        let error = parse_shortcut(ShortcutAction::PlayPause, "MediaPlayPause")
            .expect_err("native media keys should not be registered twice");

        assert!(error.contains("native media controls already handle it"));
    }

    #[test]
    fn detects_semantic_duplicates_across_aliases() {
        let shortcuts = ShortcutSettings {
            next: "Control+Alt+ArrowLeft".to_string(),
            ..ShortcutSettings::default()
        };

        let error = validate_shortcut_settings(&shortcuts)
            .expect_err("Left and ArrowLeft should identify the same shortcut");

        assert_eq!(error, "next and previous use the same shortcut");
    }

    #[test]
    fn startup_keeps_registering_after_one_os_conflict() {
        let backend = FakeBackend::default();
        backend.fail_registration_for("Ctrl+Alt+Right");
        let manager = ShortcutManager::default();
        let report = manager.register_startup_with(&backend, &ShortcutSettings::default());

        assert_eq!(report.registered, 4);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].action, ShortcutAction::Next);
        assert!(backend.is_registered("Ctrl+Alt+Space"));
        assert!(backend.is_registered("Ctrl+Alt+Left"));
    }

    #[test]
    fn failed_replacement_restores_the_previous_registration() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings::default();
        let persistence = FakePersistence::new(settings.clone());
        let report = manager.register_startup_with(&backend, &settings.shortcuts);
        assert!(report.failures.is_empty());
        backend.fail_registration_for("Ctrl+Shift+P");

        let error = manager
            .update_shortcut_with(
                &backend,
                &persistence,
                ShortcutAction::PlayPause,
                "Ctrl+Shift+P".to_string(),
            )
            .expect_err("simulated OS conflict should reject the update");

        assert!(error.contains("the previous shortcut was restored"));
        assert!(backend.is_registered("Ctrl+Alt+Space"));
        assert!(!backend.is_registered("Ctrl+Shift+P"));
        assert_eq!(persistence.snapshot(), settings);
    }

    #[test]
    fn successful_replacement_registers_and_persists_the_new_shortcut() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings::default();
        let persistence = FakePersistence::new(settings);
        let report = manager.register_startup_with(&backend, &persistence.snapshot().shortcuts);
        assert!(report.failures.is_empty());

        let updated = manager
            .update_shortcut_with(
                &backend,
                &persistence,
                ShortcutAction::PlayPause,
                "Ctrl+Shift+P".to_string(),
            )
            .expect("available shortcut should be applied");

        assert_eq!(updated, persistence.snapshot());
        assert_eq!(updated.shortcuts.play_pause, "Ctrl+Shift+P");
        assert!(backend.is_registered("Ctrl+Shift+P"));
        assert!(!backend.is_registered("Ctrl+Alt+Space"));
    }

    #[test]
    fn persistence_failure_restores_the_previous_registration() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings::default();
        let persistence = FakePersistence::new(settings.clone());
        let report = manager.register_startup_with(&backend, &settings.shortcuts);
        assert!(report.failures.is_empty());
        persistence.fail_replace.set(true);

        let error = manager
            .update_shortcut_with(
                &backend,
                &persistence,
                ShortcutAction::PlayPause,
                "Ctrl+Shift+P".to_string(),
            )
            .expect_err("simulated disk failure should reject the update");

        assert!(error.contains("the previous shortcut was restored"));
        assert!(backend.is_registered("Ctrl+Alt+Space"));
        assert!(!backend.is_registered("Ctrl+Shift+P"));
        assert_eq!(persistence.snapshot(), settings);
    }

    #[test]
    fn duplicate_update_does_not_change_registrations_or_settings() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings::default();
        let persistence = FakePersistence::new(settings.clone());
        let report = manager.register_startup_with(&backend, &settings.shortcuts);
        assert!(report.failures.is_empty());

        let error = manager
            .update_shortcut_with(
                &backend,
                &persistence,
                ShortcutAction::Previous,
                "Control+Alt+ArrowRight".to_string(),
            )
            .expect_err("semantic duplicate should be rejected before mutation");

        assert_eq!(error, "next and previous use the same shortcut");
        assert!(backend.is_registered("Ctrl+Alt+Right"));
        assert!(backend.is_registered("Ctrl+Alt+Left"));
        assert_eq!(persistence.snapshot(), settings);
    }

    #[test]
    fn restore_defaults_rolls_back_all_registrations_on_conflict() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings {
            shortcuts: ShortcutSettings {
                play_pause: "Ctrl+Shift+P".to_string(),
                next: "Ctrl+Shift+N".to_string(),
                previous: "Ctrl+Shift+B".to_string(),
                seek_forward_10: "Ctrl+Shift+F".to_string(),
                seek_backward_10: "Ctrl+Shift+R".to_string(),
            },
        };
        let persistence = FakePersistence::new(settings.clone());
        let report = manager.register_startup_with(&backend, &settings.shortcuts);
        assert!(report.failures.is_empty());
        backend.fail_registration_for("Ctrl+Alt+Right");

        let error = manager
            .restore_defaults_with(&backend, &persistence)
            .expect_err("one conflicting default should roll back the entire restore");

        assert!(error.contains("the previous shortcuts were restored"));
        for (_, shortcut) in settings.shortcuts.iter() {
            assert!(backend.is_registered(shortcut));
        }
        assert_eq!(persistence.snapshot(), settings);
    }

    #[test]
    fn restore_defaults_rolls_back_when_persistence_fails() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings {
            shortcuts: ShortcutSettings {
                play_pause: "Ctrl+Shift+P".to_string(),
                next: "Ctrl+Shift+N".to_string(),
                previous: "Ctrl+Shift+B".to_string(),
                seek_forward_10: "Ctrl+Shift+F".to_string(),
                seek_backward_10: "Ctrl+Shift+R".to_string(),
            },
        };
        let persistence = FakePersistence::new(settings.clone());
        let report = manager.register_startup_with(&backend, &settings.shortcuts);
        assert!(report.failures.is_empty());
        persistence.fail_replace.set(true);

        let error = manager
            .restore_defaults_with(&backend, &persistence)
            .expect_err("disk failure should roll back restored defaults");

        assert!(error.contains("the previous shortcuts were restored"));
        assert_eq!(persistence.snapshot(), settings);
        for (_, shortcut) in settings.shortcuts.iter() {
            assert!(backend.is_registered(shortcut));
        }
        for (_, shortcut) in ShortcutSettings::default().iter() {
            assert!(!backend.is_registered(shortcut));
        }
    }

    #[test]
    fn restore_defaults_registers_and_persists_every_default() {
        let backend = FakeBackend::default();
        let manager = ShortcutManager::default();
        let settings = AppSettings {
            shortcuts: ShortcutSettings {
                play_pause: "Ctrl+Shift+P".to_string(),
                next: "Ctrl+Shift+N".to_string(),
                previous: "Ctrl+Shift+B".to_string(),
                seek_forward_10: "Ctrl+Shift+F".to_string(),
                seek_backward_10: "Ctrl+Shift+R".to_string(),
            },
        };
        let persistence = FakePersistence::new(settings.clone());
        let report = manager.register_startup_with(&backend, &settings.shortcuts);
        assert!(report.failures.is_empty());

        let restored = manager
            .restore_defaults_with(&backend, &persistence)
            .expect("available defaults should be restored");

        assert_eq!(restored, AppSettings::default());
        assert_eq!(persistence.snapshot(), AppSettings::default());
        for (_, shortcut) in ShortcutSettings::default().iter() {
            assert!(backend.is_registered(shortcut));
        }
        for (_, shortcut) in settings.shortcuts.iter() {
            assert!(!backend.is_registered(shortcut));
        }
    }
}

mod integrations;
mod player;
mod settings;
mod shortcuts;

use integrations::{
    configure_windows_identity, install_close_to_tray, install_settings_close_handler,
    setup_discord_presence, setup_native_media_controls, setup_tray,
};

use player::{control_player, start_player_state_observer, update_player_state, PlayerStore};
use settings::{
    get_settings, restore_default_shortcuts, update_shortcut, SettingsStore, ShortcutAction,
};
use shortcuts::{validate_shortcut_settings, ShortcutManager};

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const YTMUSIC_INIT_SCRIPT: &str = include_str!("../injected/ytmusic.js");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure_windows_identity();
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .manage(PlayerStore::default())
        .invoke_handler(tauri::generate_handler![
            update_player_state,
            control_player,
            get_settings,
            update_shortcut,
            restore_default_shortcuts,
        ])
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let (settings_store, warning) = SettingsStore::load_with_diagnostics(settings_path);

            if let Some(warning) = warning {
                eprintln!("[settings] {warning}");
            }

            if let Err(error) = validate_shortcut_settings(&settings_store.snapshot().shortcuts) {
                eprintln!(
                    "[settings] loaded shortcut settings are invalid; using defaults: {error}"
                );

                if let Err(error) = settings_store.recover_defaults() {
                    eprintln!("[settings] failed to persist recovered defaults: {error}");
                }
            }

            app.manage(settings_store);
            app.manage(ShortcutManager::default());

            start_player_state_observer(app);

            // Subscribe before the WebView starts publishing
            // player snapshots.
            setup_discord_presence(app);

            let url = "https://music.youtube.com"
                .parse()
                .expect("invalid YouTube Music URL");

            let main_window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("YTMusic Desktop")
                .inner_size(1280.0, 800.0)
                .min_inner_size(900.0, 600.0)
                .center()
                .resizable(true)
                .devtools(true)
                .initialization_script(YTMUSIC_INIT_SCRIPT)
                .build()?;

            install_close_to_tray(&main_window);

            let settings_window =
                WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
                    .title("YTMusic Desktop Settings")
                    .inner_size(760.0, 700.0)
                    .min_inner_size(520.0, 480.0)
                    .center()
                    .resizable(true)
                    .visible(false)
                    .build()?;

            install_settings_close_handler(&settings_window);

            setup_native_media_controls(app, &main_window);

            setup_tray(app)?;

            let app_settings = app.state::<SettingsStore>().snapshot();
            let report = app
                .state::<ShortcutManager>()
                .register_startup(app.handle(), &app_settings.shortcuts);

            for failure in report.failures {
                eprintln!(
                    "[shortcuts] startup registration failed for {:?} (`{}`): {}",
                    failure.action, failure.shortcut, failure.error
                );
            }

            println!(
                "[shortcuts] registered {} of {} configured shortcuts",
                report.registered,
                ShortcutAction::ALL.len()
            );

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

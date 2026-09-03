mod integrations;
mod localization;
mod mini_player;
mod player;
mod runtime_settings;
mod settings;
mod shortcuts;

use integrations::{
    configure_windows_identity, install_close_to_tray, install_mini_player_close_handler,
    install_settings_close_handler, setup_discord_presence, setup_native_media_controls,
    setup_tray,
};

use localization::{get_local_ui_language, native_ui_strings};
use mini_player::{
    control_mini_player, get_mini_player_state, start_mini_player_state_bridge,
    MINI_PLAYER_WINDOW_LABEL,
};

use player::{control_player, start_player_state_observer, update_player_state, PlayerStore};
use runtime_settings::RuntimeSettings;
use settings::{
    get_settings, restore_defaults, update_application_settings, update_shortcut, SettingsStore,
    ShortcutAction,
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
            update_application_settings,
            update_shortcut,
            restore_defaults,
            get_local_ui_language,
            get_mini_player_state,
            control_mini_player,
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

                if let Err(error) = settings_store.recover_shortcut_defaults() {
                    eprintln!("[settings] failed to persist recovered shortcut defaults: {error}");
                }
            }

            let initial_settings = settings_store.snapshot();
            app.manage(settings_store);
            app.manage(ShortcutManager::default());

            start_player_state_observer(app);

            // Subscribe before the WebView starts publishing
            // player snapshots.
            let discord = setup_discord_presence(
                app,
                initial_settings.application.discord_rich_presence_enabled,
            );
            app.manage(RuntimeSettings::new(discord));

            let native_strings = native_ui_strings(initial_settings.application.language);

            let url = "https://music.youtube.com"
                .parse()
                .expect("invalid YouTube Music URL");

            let main_window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("YTMusic Desktop")
                .inner_size(1280.0, 800.0)
                .min_inner_size(900.0, 600.0)
                .center()
                .resizable(true)
                .visible(!initial_settings.application.start_minimized)
                .devtools(true)
                .initialization_script(YTMUSIC_INIT_SCRIPT)
                .build()?;

            install_close_to_tray(&main_window);

            let settings_window =
                WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
                    .title(native_strings.settings_window_title)
                    .inner_size(760.0, 700.0)
                    .min_inner_size(520.0, 480.0)
                    .center()
                    .resizable(true)
                    .visible(false)
                    .build()?;

            install_settings_close_handler(&settings_window);

            let mini_player_window = WebviewWindowBuilder::new(
                app,
                MINI_PLAYER_WINDOW_LABEL,
                WebviewUrl::App("mini-player.html".into()),
            )
            .title(native_strings.mini_player_window_title)
            .inner_size(460.0, 220.0)
            .min_inner_size(400.0, 200.0)
            .max_inner_size(640.0, 320.0)
            .center()
            .resizable(true)
            .always_on_top(initial_settings.application.mini_player_always_on_top)
            .visible(false)
            .build()?;

            install_mini_player_close_handler(&mini_player_window);

            setup_native_media_controls(app, &main_window);
            start_mini_player_state_bridge(app);

            setup_tray(app, initial_settings.application.language)?;

            let report = app
                .state::<ShortcutManager>()
                .register_startup(app.handle(), &initial_settings.shortcuts);

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

mod integrations;
mod player;

use integrations::{install_close_to_tray, setup_tray};

use player::{control_player, start_player_state_observer, update_player_state, PlayerStore};

use tauri::{WebviewUrl, WebviewWindowBuilder};

const YTMUSIC_INIT_SCRIPT: &str = include_str!("../injected/ytmusic.js");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(PlayerStore::default())
        .invoke_handler(tauri::generate_handler![
            update_player_state,
            control_player,
        ])
        .setup(|app| {
            // Start native player-state subscribers before
            // creating the YouTube Music WebView.
            start_player_state_observer(app);

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

            setup_tray(app)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

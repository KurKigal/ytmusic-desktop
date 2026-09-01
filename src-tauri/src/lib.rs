mod player;

use player::{update_player_state, PlayerStore};
use tauri::{WebviewUrl, WebviewWindowBuilder};

const YTMUSIC_INIT_SCRIPT: &str =
    include_str!("../injected/ytmusic.js");

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(PlayerStore::default())
        .invoke_handler(tauri::generate_handler![
            update_player_state
        ])
        .setup(|app| {
            let url = "https://music.youtube.com"
                .parse()
                .expect("invalid YouTube Music URL");

            WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(url),
            )
            .title("YTMusic Desktop")
            .inner_size(1280.0, 800.0)
            .min_inner_size(900.0, 600.0)
            .center()
            .resizable(true)
            .devtools(true)
            .initialization_script(YTMUSIC_INIT_SCRIPT)
            .build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
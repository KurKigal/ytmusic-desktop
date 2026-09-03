mod discord;
mod media;
mod tray;
mod window;
mod windows_identity;

pub use discord::{setup_discord_presence, DiscordController};
pub use media::setup_native_media_controls;
pub use tray::{setup_tray, update_tray_language};
pub use window::{
    install_close_to_tray, install_mini_player_close_handler, install_settings_close_handler,
};
pub use windows_identity::configure_windows_identity;

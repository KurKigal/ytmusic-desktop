mod discord;
mod media;
mod tray;
mod window;
mod windows_identity;

pub use discord::setup_discord_presence;
pub use media::setup_native_media_controls;
pub use tray::setup_tray;
pub use window::install_close_to_tray;
pub use windows_identity::configure_windows_identity;

mod bridge;
mod commands;
mod model;

pub use bridge::start_mini_player_state_bridge;
pub use commands::{control_mini_player, get_mini_player_state};
pub use model::{MiniPlayerCommand, MiniPlayerState};

pub const MINI_PLAYER_STATE_EVENT: &str = "mini-player-state";
pub const MINI_PLAYER_WINDOW_LABEL: &str = "mini-player";

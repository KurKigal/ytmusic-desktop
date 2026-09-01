mod controller;
mod model;
mod observer;
mod store;

pub use controller::control_player;

pub(crate) use controller::{dispatch_player_command, PlayerCommand};

pub(crate) use observer::start_player_state_observer;

pub use model::*;
pub use store::PlayerStore;

use tauri::State;

#[tauri::command]
pub fn update_player_state(
    store: State<'_, PlayerStore>,
    payload: PlayerSnapshot,
) -> Result<(), String> {
    payload.validate()?;

    store.update(payload)?;

    Ok(())
}

use tauri::{Manager, WebviewWindow, WindowEvent};

use crate::settings::SettingsStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainCloseBehavior {
    HideToTray,
    Exit,
}

const fn main_close_behavior(close_to_tray: bool) -> MainCloseBehavior {
    if close_to_tray {
        MainCloseBehavior::HideToTray
    } else {
        MainCloseBehavior::Exit
    }
}

/// Applies the current close-to-tray preference whenever the main window's
/// native close button is pressed. Hiding preserves the playback WebView;
/// disabling the preference exits the application normally.
pub fn install_close_to_tray(window: &WebviewWindow) {
    let window_for_event = window.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let close_to_tray = window_for_event
                .app_handle()
                .state::<SettingsStore>()
                .snapshot()
                .application
                .close_to_tray;

            match main_close_behavior(close_to_tray) {
                MainCloseBehavior::HideToTray => {
                    // Prevent Tauri from destroying the WebView.
                    api.prevent_close();

                    if let Err(error) = window_for_event.hide() {
                        eprintln!("[window] failed to hide main window: {error}");
                    } else {
                        println!("[window] main window hidden to tray");
                    }
                }
                MainCloseBehavior::Exit => {
                    println!("[window] main window close requested application exit");
                    window_for_event.app_handle().exit(0);
                }
            }
        }
    });
}

/// Keeps the local settings WebView available for reuse when its
/// native close button is pressed.
pub fn install_settings_close_handler(window: &WebviewWindow) {
    let window_for_event = window.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();

            if let Err(error) = window_for_event.hide() {
                eprintln!("[window] failed to hide settings window: {error}");
            }
        }
    });
}

/// Keeps the local mini player WebView available for reuse when its
/// native close button is pressed.
pub fn install_mini_player_close_handler(window: &WebviewWindow) {
    let window_for_event = window.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();

            if let Err(error) = window_for_event.hide() {
                eprintln!("[window] failed to hide mini player window: {error}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_close_behavior_follows_the_setting() {
        assert_eq!(main_close_behavior(true), MainCloseBehavior::HideToTray);
        assert_eq!(main_close_behavior(false), MainCloseBehavior::Exit);
    }
}

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager,
};

use crate::{
    localization::native_ui_strings,
    mini_player::MINI_PLAYER_WINDOW_LABEL,
    player::{dispatch_player_command, PlayerCommand},
    settings::{Language, SettingsStore},
};

struct TrayMenuItems {
    open: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    mini_player: MenuItem<tauri::Wry>,
    play_pause: MenuItem<tauri::Wry>,
    previous: MenuItem<tauri::Wry>,
    next: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

impl TrayMenuItems {
    fn set_language(&self, language: Language) -> Result<(), String> {
        let labels = native_ui_strings(language);
        let updates = [
            (&self.open, labels.open),
            (&self.settings, labels.settings),
            (&self.mini_player, labels.mini_player),
            (&self.play_pause, labels.play_pause),
            (&self.previous, labels.previous),
            (&self.next, labels.next),
            (&self.quit, labels.quit),
        ];
        let errors = updates
            .into_iter()
            .filter_map(|(item, text)| item.set_text(text).err().map(|error| error.to_string()))
            .collect::<Vec<_>>();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }
}

/// Creates the native system tray icon and menu.
pub fn setup_tray(app: &mut App, language: Language) -> tauri::Result<()> {
    let labels = native_ui_strings(language);
    let open_item = MenuItem::with_id(app, "open", labels.open, true, None::<&str>)?;

    let settings_item = MenuItem::with_id(app, "settings", labels.settings, true, None::<&str>)?;

    let mini_player_item =
        MenuItem::with_id(app, "mini_player", labels.mini_player, true, None::<&str>)?;

    let play_pause_item =
        MenuItem::with_id(app, "play_pause", labels.play_pause, true, None::<&str>)?;

    let previous_item = MenuItem::with_id(app, "previous", labels.previous, true, None::<&str>)?;

    let next_item = MenuItem::with_id(app, "next", labels.next, true, None::<&str>)?;

    let separator_one = PredefinedMenuItem::separator(app)?;

    let separator_two = PredefinedMenuItem::separator(app)?;

    let quit_item = MenuItem::with_id(app, "quit", labels.quit, true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &settings_item,
            &mini_player_item,
            &separator_one,
            &play_pause_item,
            &previous_item,
            &next_item,
            &separator_two,
            &quit_item,
        ],
    )?;

    let mut tray_builder = TrayIconBuilder::new()
        .menu(&menu)
        // Left click opens the window.
        // Right click opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Err(error) = show_main_window(app) {
                    eprintln!("[tray] failed to show main window: {error}");
                }
            }

            "settings" => {
                if let Err(error) = show_settings_window(app) {
                    eprintln!("[tray] failed to show settings window: {error}");
                }
            }

            "mini_player" => {
                if let Err(error) = show_mini_player_window(app) {
                    eprintln!("[tray] failed to show mini player window: {error}");
                }
            }

            "play_pause" => {
                execute_player_command(app, PlayerCommand::TogglePlayback);
            }

            "previous" => {
                execute_player_command(app, PlayerCommand::Previous);
            }

            "next" => {
                execute_player_command(app, PlayerCommand::Next);
            }

            "quit" => {
                println!("[tray] quit requested");

                app.exit(0);
            }

            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();

                if let Err(error) = show_main_window(app) {
                    eprintln!("[tray] failed to show main window: {error}");
                }
            }
        });

    // Reuse the configured application icon when available.
    if let Some(icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    }

    tray_builder.build(app)?;
    app.manage(TrayMenuItems {
        open: open_item,
        settings: settings_item,
        mini_player: mini_player_item,
        play_pause: play_pause_item,
        previous: previous_item,
        next: next_item,
        quit: quit_item,
    });

    println!("[tray] system tray initialized");

    Ok(())
}

pub fn update_tray_language(app: &AppHandle, language: Language) -> Result<(), String> {
    app.state::<TrayMenuItems>().set_language(language)
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main webview window not found".to_string())?;

    window
        .unminimize()
        .map_err(|error| format!("failed to unminimize window: {error}"))?;

    window
        .show()
        .map_err(|error| format!("failed to show window: {error}"))?;

    window
        .set_focus()
        .map_err(|error| format!("failed to focus window: {error}"))?;

    Ok(())
}

fn show_settings_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings webview window not found".to_string())?;

    window
        .unminimize()
        .map_err(|error| format!("failed to unminimize settings window: {error}"))?;

    window
        .show()
        .map_err(|error| format!("failed to show settings window: {error}"))?;

    window
        .set_focus()
        .map_err(|error| format!("failed to focus settings window: {error}"))?;

    Ok(())
}

fn show_mini_player_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(MINI_PLAYER_WINDOW_LABEL)
        .ok_or_else(|| "mini player webview window not found".to_string())?;

    let always_on_top = app
        .state::<SettingsStore>()
        .snapshot()
        .application
        .mini_player_always_on_top;

    window
        .set_always_on_top(always_on_top)
        .map_err(|error| format!("failed to update mini player always-on-top state: {error}"))?;

    window
        .unminimize()
        .map_err(|error| format!("failed to unminimize mini player window: {error}"))?;

    window
        .show()
        .map_err(|error| format!("failed to show mini player window: {error}"))?;

    window
        .set_focus()
        .map_err(|error| format!("failed to focus mini player window: {error}"))?;

    Ok(())
}

fn execute_player_command(app: &AppHandle, command: PlayerCommand) {
    if let Err(error) = dispatch_player_command(app, command) {
        eprintln!("[tray] player command failed: {error}");
    }
}

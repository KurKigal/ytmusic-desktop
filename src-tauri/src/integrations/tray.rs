use tauri::{
    menu::{
        Menu,
        MenuItem,
        PredefinedMenuItem,
    },
    tray::{
        MouseButton,
        MouseButtonState,
        TrayIconBuilder,
        TrayIconEvent,
    },
    App,
    AppHandle,
    Manager,
};

use crate::player::{
    dispatch_player_command,
    PlayerCommand,
};

/// Creates the native system tray icon and menu.
pub fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let open_item = MenuItem::with_id(
        app,
        "open",
        "Open YTMusic Desktop",
        true,
        None::<&str>,
    )?;

    let play_pause_item = MenuItem::with_id(
        app,
        "play_pause",
        "Play / Pause",
        true,
        None::<&str>,
    )?;

    let previous_item = MenuItem::with_id(
        app,
        "previous",
        "Previous",
        true,
        None::<&str>,
    )?;

    let next_item = MenuItem::with_id(
        app,
        "next",
        "Next",
        true,
        None::<&str>,
    )?;

    let separator_one =
        PredefinedMenuItem::separator(app)?;

    let separator_two =
        PredefinedMenuItem::separator(app)?;

    let quit_item = MenuItem::with_id(
        app,
        "quit",
        "Quit",
        true,
        None::<&str>,
    )?;

    let menu = Menu::with_items(
        app,
        &[
            &open_item,
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
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "open" => {
                    if let Err(error) = show_main_window(app) {
                        eprintln!(
                            "[tray] failed to show main window: {error}"
                        );
                    }
                }

                "play_pause" => {
                    execute_player_command(
                        app,
                        PlayerCommand::TogglePlayback,
                    );
                }

                "previous" => {
                    execute_player_command(
                        app,
                        PlayerCommand::Previous,
                    );
                }

                "next" => {
                    execute_player_command(
                        app,
                        PlayerCommand::Next,
                    );
                }

                "quit" => {
                    println!("[tray] quit requested");

                    app.exit(0);
                }

                _ => {}
            }
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
                    eprintln!(
                        "[tray] failed to show main window: {error}"
                    );
                }
            }
        });

    // Reuse the configured application icon when available.
    if let Some(icon) = app.default_window_icon() {
        tray_builder =
            tray_builder.icon(icon.clone());
    }

    tray_builder.build(app)?;

    println!("[tray] system tray initialized");

    Ok(())
}

fn show_main_window(
    app: &AppHandle,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| {
            "main webview window not found".to_string()
        })?;

    window
        .unminimize()
        .map_err(|error| {
            format!(
                "failed to unminimize window: {error}"
            )
        })?;

    window
        .show()
        .map_err(|error| {
            format!(
                "failed to show window: {error}"
            )
        })?;

    window
        .set_focus()
        .map_err(|error| {
            format!(
                "failed to focus window: {error}"
            )
        })?;

    Ok(())
}

fn execute_player_command(
    app: &AppHandle,
    command: PlayerCommand,
) {
    if let Err(error) =
        dispatch_player_command(app, command)
    {
        eprintln!(
            "[tray] player command failed: {error}"
        );
    }
}
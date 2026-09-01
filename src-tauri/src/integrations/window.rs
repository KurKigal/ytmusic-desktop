use tauri::{WebviewWindow, WindowEvent};

/// Changes the main window close behavior so that pressing
/// the native close button hides the window instead of
/// terminating the application.
///
/// Playback continues because the WebView remains alive.
pub fn install_close_to_tray(window: &WebviewWindow) {
    let window_for_event = window.clone();

    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            // Prevent Tauri from destroying the WebView.
            api.prevent_close();

            if let Err(error) = window_for_event.hide() {
                eprintln!("[window] failed to hide main window: {error}");
            } else {
                println!("[window] main window hidden to tray");
            }
        }
    });
}

use std::time::Duration;

use playwire::{Capabilities, Event, MediaControls, PlaybackState, PlayerConfig, Repeat, Track};

use tauri::{App, AppHandle, Manager, WebviewWindow};

use tokio::sync::mpsc;

use crate::player::{
    dispatch_player_command, PlaybackStatus, PlayerCommand, PlayerSnapshot, PlayerStore,
};

/// Starts the operating system media integration.
///
/// Player state flows from `PlayerStore` to the OS, while media
/// key events flow from the OS back into the Rust player API.
pub fn setup_native_media_controls(app: &App, window: &WebviewWindow) {
    let config = match build_player_config(window) {
        Ok(config) => config,

        Err(error) => {
            eprintln!("[media] failed to build native media config: {error}");

            return;
        }
    };

    let (event_sender, mut event_receiver) = mpsc::unbounded_channel::<Event>();

    let controls = MediaControls::new(config, move |event| {
        // playwire callbacks can arrive from an OS-owned
        // thread. Only enqueue here; never block it.
        let _ = event_sender.send(event);
    });

    let mut controls = match controls {
        Ok(controls) => controls,

        Err(error) => {
            // Native media controls are an optional integration.
            // Failure should not prevent the music client from
            // starting.
            eprintln!("[media] native media controls unavailable: {error}");

            return;
        }
    };

    let mut state_receiver = app.state::<PlayerStore>().subscribe();

    let app_handle = app.handle().clone();

    tauri::async_runtime::spawn(async move {
        // A subscriber may be created after the first state was
        // published, so publish the current snapshot once before
        // waiting for changes.
        let initial = state_receiver.borrow().clone();

        if let Some(snapshot) = initial {
            publish_state(&mut controls, &snapshot);
        }

        loop {
            tokio::select! {
                changed = state_receiver.changed() => {
                    if changed.is_err() {
                        break;
                    }

                    let current = state_receiver
                        .borrow_and_update()
                        .clone();

                    let Some(current) = current else {
                        continue;
                    };

                    publish_state(
                        &mut controls,
                        &current,
                    );
                }

                event = event_receiver.recv() => {
                    let Some(event) = event else {
                        break;
                    };

                    handle_media_event(
                        &app_handle,
                        event,
                    );
                }
            }
        }

        println!("[media] native media integration stopped");
    });

    println!("[media] native media controls initialized");
}

fn build_player_config(window: &WebviewWindow) -> Result<PlayerConfig, String> {
    // The value passed to PlayerConfig::new must also be a valid
    // D-Bus name component, so keep this machine-friendly.
    let mut config = PlayerConfig::new("YTMusicDesktop")
        .desktop_entry("com.emirhankeser.ytmdesktop")
        .track_id_prefix("/com/emirhankeser/ytmdesktop/track");

    // Human-readable name displayed by integrations such as MPRIS.
    config.identity = "YTMusic Desktop".to_string();

    #[cfg(windows)]
    {
        let hwnd = window
            .hwnd()
            .map_err(|error| format!("failed to obtain main window HWND: {error}"))?;

        let hwnd = hwnd.0 as usize as u64;

        config = config.hwnd(hwnd);
    }

    #[cfg(not(windows))]
    {
        let _ = window;
    }

    Ok(config)
}

fn publish_state(controls: &mut MediaControls, snapshot: &PlayerSnapshot) {
    let state = convert_player_state(snapshot);

    if let Err(error) = controls.set_state(&state) {
        // playwire documents publish failures as generally
        // transient, so keep the integration alive and try again
        // on the next state update.
        eprintln!("[media] failed to publish player state: {error}");
    }
}

fn convert_player_state(snapshot: &PlayerSnapshot) -> PlaybackState {
    let duration = if snapshot.duration > 0.0 {
        Some(Duration::from_secs_f64(snapshot.duration))
    } else {
        None
    };

    let position_seconds = match duration {
        Some(duration) => snapshot.position.min(duration.as_secs_f64()),

        None => snapshot.position,
    };

    PlaybackState {
        track: snapshot.metadata.as_ref().map(build_track),

        playing: matches!(snapshot.playback, PlaybackStatus::Playing),

        position: Duration::from_secs_f64(position_seconds),

        duration,

        // We do not synchronize YouTube Music volume yet.
        // Windows and macOS ignore this field.
        volume: 1.0,

        repeat: Repeat::Off,

        shuffle: false,

        capabilities: Capabilities {
            can_go_next: true,
            can_go_previous: true,
            can_seek: duration.is_some(),
        },
    }
}

fn build_track(metadata: &crate::player::TrackMetadata) -> Track {
    let title = metadata
        .title
        .clone()
        .unwrap_or_else(|| "Unknown title".to_string());

    let artist = metadata.artist.clone().unwrap_or_default();

    let album = metadata.album.clone().unwrap_or_default();

    let artwork_url = select_artwork_url(metadata);

    let id = create_track_id(&title, &artist);

    Track {
        id,
        title,
        artists: if artist.is_empty() {
            Vec::new()
        } else {
            vec![artist]
        },
        album,
        artwork_url,

        // We do not currently expose the canonical YouTube Music
        // track URL in PlayerSnapshot.
        url: String::new(),
    }
}

fn select_artwork_url(metadata: &crate::player::TrackMetadata) -> String {
    metadata
        .artwork
        .iter()
        .rev()
        .find(|artwork| artwork.src.starts_with("https://") || artwork.src.starts_with("http://"))
        .map(|artwork| artwork.src.clone())
        .unwrap_or_default()
}

/// Generates a deterministic lightweight track identifier from
/// the fields currently used as our track identity.
///
/// We can replace this with the YouTube video ID later when that
/// becomes part of PlayerSnapshot.
fn create_track_id(title: &str, artist: &str) -> String {
    // 64-bit FNV-1a.
    let mut hash: u64 = 0xcbf29ce484222325;

    for byte in title.bytes().chain([0]).chain(artist.bytes()) {
        hash ^= byte as u64;

        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("{hash:016x}")
}

fn handle_media_event(app: &AppHandle, event: Event) {
    let result = match event {
        Event::Play => dispatch_player_command(app, PlayerCommand::Play),

        Event::Pause => dispatch_player_command(app, PlayerCommand::Pause),

        Event::PlayPause => dispatch_player_command(app, PlayerCommand::TogglePlayback),

        Event::Stop => dispatch_player_command(app, PlayerCommand::Stop),

        Event::Next => dispatch_player_command(app, PlayerCommand::Next),

        Event::Previous => dispatch_player_command(app, PlayerCommand::Previous),

        Event::SeekTo(position) => dispatch_player_command(
            app,
            PlayerCommand::Seek {
                position: position.as_secs_f64(),
            },
        ),

        Event::SeekBy(offset) => dispatch_player_command(app, PlayerCommand::SeekBy { offset }),

        // Volume, shuffle, repeat, OpenUri, Raise and Quit
        // are not connected to our application model yet.
        //
        // Event is non-exhaustive, so the wildcard is also
        // required for forward compatibility.
        _ => Ok(()),
    };

    if let Err(error) = result {
        eprintln!("[media] player command failed: {error}");
    }
}

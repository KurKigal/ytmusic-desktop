#[cfg(windows)]
use windows::{core::HSTRING, Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID};

const APP_USER_MODEL_ID: &str = "KurKigal.YTMusicDesktop";

pub fn configure_windows_identity() {
    #[cfg(windows)]
    {
        let app_id = HSTRING::from(APP_USER_MODEL_ID);

        if let Err(error) = unsafe { SetCurrentProcessExplicitAppUserModelID(&app_id) } {
            eprintln!("[windows] failed to set AppUserModelID: {error}");
        } else {
            println!("[windows] AppUserModelID configured");
        }
    }
}

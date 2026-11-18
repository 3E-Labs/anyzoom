//! Loading and saving the settings file (%APPDATA%\anyzoom\settings.json).

use anyzoom_core::Settings;
use std::path::PathBuf;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::time::Duration;

/// Changes are written once things have been quiet for this long.
const QUIET: Duration = Duration::from_millis(500);

pub fn settings_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("anyzoom").join("settings.json")
}

/// A missing, unreadable or damaged file gives the default settings.
pub fn load() -> Settings {
    match std::fs::read_to_string(settings_path()) {
        Ok(text) => Settings::from_json_or_default(&text),
        Err(_) => Settings::default(),
    }
}

/// Writes to a temporary file first so a crash mid-write cannot leave a
/// half-written settings file behind.
pub fn write_now(settings: &Settings) {
    let path = settings_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, settings.to_json()).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// Saves in the background, folding bursts of changes (dragging the zoom
/// slider) into one write.
pub struct Saver {
    tx: Sender<Settings>,
}

impl Saver {
    pub fn new() -> Saver {
        let (tx, rx) = channel::<Settings>();
        std::thread::spawn(move || {
            while let Ok(mut latest) = rx.recv() {
                loop {
                    match rx.recv_timeout(QUIET) {
                        Ok(newer) => latest = newer,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => {
                            write_now(&latest);
                            return;
                        }
                    }
                }
                write_now(&latest);
            }
        });
        Saver { tx }
    }

    pub fn save(&self, settings: &Settings) {
        let _ = self.tx.send(settings.clone());
    }
}

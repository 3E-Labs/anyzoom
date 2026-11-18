//! anyzoom: zoom and pan for any webcam, published as a virtual camera.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("anyzoom runs on Windows 11 only.");
}

#[cfg(windows)]
mod commands;
#[cfg(windows)]
mod hotkeys;
#[cfg(windows)]
mod store;
#[cfg(windows)]
mod tray;

#[cfg(windows)]
pub use app::AppState;

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(windows)]
mod app {
    use crate::{commands, hotkeys, store, tray};
    use anyzoom_core::Settings;
    use anyzoom_engine::Engine;
    use std::collections::BTreeMap;
    use std::sync::{Mutex, MutexGuard};
    use tauri::{Manager, RunEvent, WindowEvent};
    use tauri_plugin_autostart::ManagerExt;

    /// Passed by the "Start with Windows" entry: start in the tray, no window.
    const HIDDEN_FLAG: &str = "--hidden";

    pub struct AppState {
        pub engine: Engine,
        settings: Mutex<Settings>,
        hotkey_errors: Mutex<BTreeMap<String, String>>,
        saver: store::Saver,
    }

    impl AppState {
        pub fn settings(&self) -> MutexGuard<'_, Settings> {
            self.settings.lock().unwrap_or_else(|e| e.into_inner())
        }

        pub fn hotkey_errors(&self) -> MutexGuard<'_, BTreeMap<String, String>> {
            self.hotkey_errors.lock().unwrap_or_else(|e| e.into_inner())
        }

        /// Changes the settings and schedules a save.
        pub fn update(&self, change: impl FnOnce(&mut Settings)) {
            let mut settings = self.settings();
            change(&mut settings);
            self.saver.save(&settings);
        }

        /// Copies the engine's current target view into the settings.
        pub fn remember_view(&self) {
            let view = self.engine.status().view;
            self.update(|s| s.view = view);
        }
    }

    pub fn run() {
        let app = tauri::Builder::default()
            // must be first: a second launch only brings the first one forward
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_window(app)))
            .plugin(tauri_plugin_global_shortcut::Builder::new().build())
            .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![HIDDEN_FLAG])))
            .invoke_handler(tauri::generate_handler![
                commands::list_cameras,
                commands::get_state,
                commands::set_camera,
                commands::set_output,
                commands::set_view,
                commands::reset_view,
                commands::set_hotkey,
                commands::set_start_with_windows,
                commands::set_preview_wanted,
                commands::get_preview,
            ])
            .setup(|app| {
                let handle = app.handle().clone();
                let mut settings = store::load();
                // the registry entry is the truth for "Start with Windows"
                settings.start_with_windows = handle.autolaunch().is_enabled().unwrap_or(false);
                let engine = Engine::start(settings.clone())?;
                let hotkey_errors = hotkeys::register_all(&handle, &settings.hotkeys);
                app.manage(AppState {
                    engine,
                    settings: Mutex::new(settings),
                    hotkey_errors: Mutex::new(hotkey_errors),
                    saver: store::Saver::new(),
                });
                tray::build(&handle)?;
                if !std::env::args().any(|a| a == HIDDEN_FLAG) {
                    tray::show_window(&handle);
                }
                Ok(())
            })
            .on_window_event(|window, event| {
                match event {
                    // Closing the window keeps anyzoom (and its camera) running in the tray.
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                        tray::sync_preview(window.app_handle());
                    }
                    // minimise, restore, show and hide all arrive as one of these
                    WindowEvent::Resized(_) | WindowEvent::Focused(_) | WindowEvent::Moved(_) => tray::sync_preview(window.app_handle()),
                    _ => {}
                }
            })
            .build(tauri::generate_context!())
            .expect("anyzoom could not start");

        app.run(|handle, event| {
            if let RunEvent::Exit = event {
                if let Some(state) = handle.try_state::<AppState>() {
                    store::write_now(&state.settings());
                    state.engine.clone().shutdown();
                }
            }
        });
    }
}

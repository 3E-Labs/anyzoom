//! The system tray icon: anyzoom keeps running here when its window is closed.

use crate::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

/// True while the main window is shown and not minimised.
pub fn window_on_screen(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

/// Runs the physical camera for the preview only while the window is on screen.
pub fn sync_preview(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.engine.set_preview_wanted(window_on_screen(app));
    }
}

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        sync_preview(app);
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open anyzoom", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "Reset zoom", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &reset, &quit])?;
    let mut tray = TrayIconBuilder::with_id("anyzoom")
        .tooltip("anyzoom")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_window(app),
            "reset" => {
                let state = app.state::<AppState>();
                state.engine.reset();
                state.remember_view();
                let _ = app.emit("view-changed", ());
            }
            // Quit really quits: the virtual camera disappears with the app.
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

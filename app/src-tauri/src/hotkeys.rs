//! Global hotkeys: work while any app has the focus.

use crate::AppState;
use anyzoom_core::Hotkeys;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Performs a hotkey's action and remembers the resulting view.
fn perform(app: &AppHandle, action: &str) {
    let state = app.state::<AppState>();
    match action {
        "zoom_in" => state.engine.nudge_zoom(1),
        "zoom_out" => state.engine.nudge_zoom(-1),
        "reset" => state.engine.reset(),
        "pan_left" => state.engine.nudge_pan(-1, 0),
        "pan_right" => state.engine.nudge_pan(1, 0),
        "pan_up" => state.engine.nudge_pan(0, -1),
        "pan_down" => state.engine.nudge_pan(0, 1),
        _ => return,
    }
    state.remember_view();
    let _ = app.emit("view-changed", ());
}

/// (Re)registers every hotkey. Returns, per action, why its binding could not
/// be registered; the bindings that are fine keep working regardless.
pub fn register_all(app: &AppHandle, hotkeys: &Hotkeys) -> BTreeMap<String, String> {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let mut errors = BTreeMap::new();
    let mut taken: Vec<(Shortcut, &str)> = Vec::new();
    for action in Hotkeys::ACTIONS {
        let accelerator = hotkeys.get(action).unwrap_or("").trim();
        if accelerator.is_empty() {
            continue; // deliberately unbound
        }
        let shortcut = match accelerator.parse::<Shortcut>() {
            Ok(shortcut) => shortcut,
            Err(_) => {
                errors.insert(action.to_string(), format!("\"{accelerator}\" is not a key combination anyzoom understands"));
                continue;
            }
        };
        if let Some((_, other)) = taken.iter().find(|(s, _)| *s == shortcut) {
            errors.insert(action.to_string(), format!("same keys as \"{}\"", other.replace('_', " ")));
            continue;
        }
        let registered = shortcuts.on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                perform(app, action);
            }
        });
        match registered {
            Ok(()) => taken.push((shortcut, action)),
            Err(_) => {
                errors.insert(action.to_string(), "already in use by another program".to_string());
            }
        }
    }
    errors
}

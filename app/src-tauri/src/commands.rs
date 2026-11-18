//! Everything the window can ask the app to do.

use crate::{hotkeys, AppState};
use anyzoom_core::{Hotkeys, Output, Rect, Settings, View};
use anyzoom_engine::EngineStatus;
use base64::Engine as _;
use serde::Serialize;
use std::collections::BTreeMap;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

#[derive(Serialize)]
pub struct CameraDto {
    id: String,
    name: String,
}

#[derive(Serialize)]
pub struct StateDto {
    settings: Settings,
    status: EngineStatus,
    /// action name -> why its hotkey is not working
    hotkey_errors: BTreeMap<String, String>,
    version: &'static str,
}

#[derive(Serialize)]
pub struct PreviewDto {
    jpeg_base64: String,
    crop: Rect,
    src: (u32, u32),
}

// `async` here means "run off the main thread": finding cameras can take a
// moment and must not freeze the window.
#[tauri::command(async)]
pub fn list_cameras() -> Result<Vec<CameraDto>, String> {
    anyzoom_capture_win::list_cameras()
        .map(|cameras| cameras.into_iter().map(|c| CameraDto { id: c.id, name: c.name }).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_state(state: State<'_, AppState>) -> StateDto {
    StateDto {
        settings: state.settings().clone(),
        status: state.engine.status(),
        hotkey_errors: state.hotkey_errors().clone(),
        version: env!("CARGO_PKG_VERSION"),
    }
}

#[tauri::command]
pub fn set_camera(state: State<'_, AppState>, id: Option<String>) {
    state.engine.set_camera(id.clone());
    state.update(|s| s.camera_id = id);
}

#[tauri::command]
pub fn set_output(state: State<'_, AppState>, output: Output) {
    state.engine.set_output(output);
    state.update(|s| s.output = output);
}

#[tauri::command]
pub fn set_view(state: State<'_, AppState>, view: View) {
    state.engine.set_target(view);
    state.remember_view();
}

#[tauri::command]
pub fn reset_view(state: State<'_, AppState>) {
    state.engine.reset();
    state.remember_view();
}

/// Stores the binding and re-registers all hotkeys. An `Err` says why this
/// binding is not working; the other bindings are unaffected.
#[tauri::command]
pub fn set_hotkey(app: AppHandle, state: State<'_, AppState>, action: String, accelerator: String) -> Result<(), String> {
    if !Hotkeys::ACTIONS.contains(&action.as_str()) {
        return Err(format!("unknown action \"{action}\""));
    }
    state.update(|s| {
        s.hotkeys.set(&action, accelerator.trim().to_string());
    });
    let hotkeys = state.settings().hotkeys.clone();
    let errors = hotkeys::register_all(&app, &hotkeys);
    let outcome = errors.get(&action).cloned();
    *state.hotkey_errors() = errors;
    match outcome {
        Some(why) => Err(why),
        None => Ok(()),
    }
}

#[tauri::command]
pub fn set_start_with_windows(app: AppHandle, state: State<'_, AppState>, on: bool) -> Result<(), String> {
    let autostart = app.autolaunch();
    let result = if on { autostart.enable() } else { autostart.disable() };
    result.map_err(|e| e.to_string())?;
    state.update(|s| s.start_with_windows = on);
    Ok(())
}

/// The page may ask for the preview, but it only gets it while the window
/// can really be seen: the camera light must never be on for a hidden window.
#[tauri::command]
pub fn set_preview_wanted(app: AppHandle, state: State<'_, AppState>, on: bool) {
    state.engine.set_preview_wanted(on && crate::tray::window_on_screen(&app));
}

#[tauri::command(async)]
pub fn get_preview(state: State<'_, AppState>) -> Option<PreviewDto> {
    state.engine.preview().map(|(jpeg, crop, src)| PreviewDto {
        jpeg_base64: base64::engine::general_purpose::STANDARD.encode(jpeg),
        crop,
        src,
    })
}

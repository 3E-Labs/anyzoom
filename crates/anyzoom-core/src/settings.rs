//! Persisted user settings.

use crate::view::View;
use serde::{Deserialize, Serialize};

/// Zoom change per hotkey press.
pub const ZOOM_STEP: f32 = 0.1;
/// Pan change per hotkey press.
pub const PAN_STEP: f32 = 0.1;

/// Size of the virtual camera's picture.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Output {
    #[default]
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
}

impl Output {
    pub fn size(self) -> (u32, u32) {
        match self {
            Output::P720 => (1280, 720),
            Output::P1080 => (1920, 1080),
        }
    }
}

/// Global hotkeys, as accelerator strings such as `Ctrl+Alt+=`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(default)]
pub struct Hotkeys {
    pub zoom_in: String,
    pub zoom_out: String,
    pub reset: String,
    pub pan_left: String,
    pub pan_right: String,
    pub pan_up: String,
    pub pan_down: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys {
            zoom_in: "Ctrl+Alt+=".into(),
            zoom_out: "Ctrl+Alt+-".into(),
            reset: "Ctrl+Alt+0".into(),
            pan_left: "Ctrl+Alt+Left".into(),
            pan_right: "Ctrl+Alt+Right".into(),
            pan_up: "Ctrl+Alt+Up".into(),
            pan_down: "Ctrl+Alt+Down".into(),
        }
    }
}

impl Hotkeys {
    /// Every action name, in display order.
    pub const ACTIONS: [&'static str; 7] =
        ["zoom_in", "zoom_out", "reset", "pan_left", "pan_right", "pan_up", "pan_down"];

    fn slot(&mut self, action: &str) -> Option<&mut String> {
        Some(match action {
            "zoom_in" => &mut self.zoom_in,
            "zoom_out" => &mut self.zoom_out,
            "reset" => &mut self.reset,
            "pan_left" => &mut self.pan_left,
            "pan_right" => &mut self.pan_right,
            "pan_up" => &mut self.pan_up,
            "pan_down" => &mut self.pan_down,
            _ => return None,
        })
    }

    pub fn get(&self, action: &str) -> Option<&str> {
        Some(match action {
            "zoom_in" => &self.zoom_in,
            "zoom_out" => &self.zoom_out,
            "reset" => &self.reset,
            "pan_left" => &self.pan_left,
            "pan_right" => &self.pan_right,
            "pan_up" => &self.pan_up,
            "pan_down" => &self.pan_down,
            _ => return None,
        })
    }

    /// Returns false when `action` is not a known action name.
    pub fn set(&mut self, action: &str, accelerator: String) -> bool {
        match self.slot(action) {
            Some(s) => {
                *s = accelerator;
                true
            }
            None => false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Settings {
    /// The chosen camera's device path; `None` means "first camera found".
    pub camera_id: Option<String>,
    pub view: View,
    pub output: Output,
    pub hotkeys: Hotkeys,
    pub start_with_windows: bool,
}

impl Settings {
    /// Parses settings, filling anything missing with defaults. Text that
    /// cannot be read at all gives the defaults, so a damaged settings file
    /// never stops the app from starting.
    pub fn from_json_or_default(s: &str) -> Settings {
        let mut settings: Settings = serde_json::from_str(s).unwrap_or_default();
        settings.view = settings.view.clamped();
        settings
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings are plain data")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_design() {
        let s = Settings::default();
        assert_eq!(s.camera_id, None);
        assert_eq!(s.view, View::new());
        assert_eq!(s.output, Output::P720);
        assert!(!s.start_with_windows);
        assert_eq!(s.hotkeys.zoom_in, "Ctrl+Alt+=");
        assert_eq!(s.hotkeys.zoom_out, "Ctrl+Alt+-");
        assert_eq!(s.hotkeys.reset, "Ctrl+Alt+0");
        assert_eq!(s.hotkeys.pan_left, "Ctrl+Alt+Left");
        assert_eq!(s.hotkeys.pan_right, "Ctrl+Alt+Right");
        assert_eq!(s.hotkeys.pan_up, "Ctrl+Alt+Up");
        assert_eq!(s.hotkeys.pan_down, "Ctrl+Alt+Down");
        assert_eq!((ZOOM_STEP, PAN_STEP), (0.1, 0.1));
    }

    #[test]
    fn output_sizes() {
        assert_eq!(Output::P720.size(), (1280, 720));
        assert_eq!(Output::P1080.size(), (1920, 1080));
    }

    #[test]
    fn settings_roundtrip() {
        let mut s = Settings::default();
        s.camera_id = Some(r"\\?\usb#vid_046d&pid_094c".into());
        s.view = View { zoom: 2.5, pan_x: -0.25, pan_y: 0.75 };
        s.output = Output::P1080;
        s.hotkeys.reset = "Ctrl+Shift+R".into();
        s.start_with_windows = true;
        assert_eq!(Settings::from_json_or_default(&s.to_json()), s);
    }

    #[test]
    fn garbage_json_gives_defaults() {
        for bad in ["{not json", "", "[]", "null", "42", r#"{"view":"sideways"}"#] {
            assert_eq!(Settings::from_json_or_default(bad), Settings::default(), "{bad}");
        }
    }

    #[test]
    fn partial_json_fills_defaults() {
        let s = Settings::from_json_or_default(r#"{"view":{"zoom":2.0}}"#);
        assert_eq!(s.view, View { zoom: 2.0, pan_x: 0.0, pan_y: 0.0 });
        assert_eq!(s.hotkeys, Hotkeys::default());
        let s = Settings::from_json_or_default(r#"{"hotkeys":{"reset":"F9"},"some_future_field":1}"#);
        assert_eq!(s.hotkeys.reset, "F9");
        assert_eq!(s.hotkeys.zoom_in, "Ctrl+Alt+=");
    }

    #[test]
    fn out_of_range_view_is_clamped_on_load() {
        let s = Settings::from_json_or_default(r#"{"view":{"zoom":99,"pan_x":-7}}"#);
        assert_eq!(s.view, View { zoom: 4.0, pan_x: -1.0, pan_y: 0.0 });
    }

    #[test]
    fn hotkeys_get_and_set_by_action_name() {
        let mut h = Hotkeys::default();
        assert_eq!(Hotkeys::ACTIONS.len(), 7);
        for a in Hotkeys::ACTIONS {
            assert!(h.get(a).is_some(), "{a}");
        }
        assert!(h.set("pan_up", "F8".into()));
        assert_eq!(h.pan_up, "F8");
        assert!(!h.set("self_destruct", "F1".into()));
        assert_eq!(h.get("nope"), None);
    }
}

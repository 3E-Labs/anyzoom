//! Platform-neutral core of anyzoom: no OS calls, tested on any platform.

pub mod view;

pub use view::{crop_rect, Rect, View, ZOOM_MAX, ZOOM_MIN};

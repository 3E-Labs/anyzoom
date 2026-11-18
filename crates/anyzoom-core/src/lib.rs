//! Platform-neutral core of anyzoom: no OS calls, tested on any platform.

pub mod scale;
pub mod view;

pub use view::{crop_rect, Rect, View, ZOOM_MAX, ZOOM_MIN};
pub use scale::{fill_nv12, nv12_len, scale_nv12, Nv12, ScaleError};

//! Platform-neutral core of anyzoom: no OS calls, tested on any platform.

pub mod bus;
pub mod card;
pub mod ease;
pub mod mode;
pub mod pace;
pub mod scale;
pub mod settings;
pub mod view;

pub use view::{crop_rect, Rect, View, ZOOM_MAX, ZOOM_MIN};
pub use scale::{fill_nv12, nv12_len, scale_nv12, Nv12, ScaleError};
pub use ease::Animator;
pub use settings::{Hotkeys, Output, Settings, PAN_STEP, ZOOM_STEP};
pub use card::{draw_card, Card};

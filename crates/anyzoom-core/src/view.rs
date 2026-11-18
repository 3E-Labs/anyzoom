//! Zoom/pan state and the source rectangle it selects.

pub const ZOOM_MIN: f32 = 1.0;
pub const ZOOM_MAX: f32 = 4.0;

/// What part of the camera image is shown. `pan_*` run from -1.0 (flush
/// left/top) to 1.0 (flush right/bottom); 0.0 is centred.
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct View {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for View {
    fn default() -> Self {
        Self::new()
    }
}

impl View {
    pub const fn new() -> Self {
        View { zoom: 1.0, pan_x: 0.0, pan_y: 0.0 }
    }

    /// Brings every field into range. A view holding any non-finite number
    /// is replaced by the default view.
    pub fn clamped(self) -> Self {
        if !(self.zoom.is_finite() && self.pan_x.is_finite() && self.pan_y.is_finite()) {
            return View::new();
        }
        View {
            zoom: self.zoom.clamp(ZOOM_MIN, ZOOM_MAX),
            pan_x: self.pan_x.clamp(-1.0, 1.0),
            pan_y: self.pan_y.clamp(-1.0, 1.0),
        }
    }

    pub fn zoom_by(self, delta: f32) -> Self {
        View { zoom: self.zoom + delta, ..self }.clamped()
    }

    pub fn pan_by(self, dx: f32, dy: f32) -> Self {
        View { pan_x: self.pan_x + dx, pan_y: self.pan_y + dy, ..self }.clamped()
    }
}

/// A rectangle in source pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

fn even_floor(v: u32) -> u32 {
    v & !1
}

/// Source-pixel rectangle to show: 16:9, inside the source, all four values
/// even. A source smaller than 2x2 yields an empty rectangle.
pub fn crop_rect(src_w: u32, src_h: u32, view: View) -> Rect {
    if src_w < 2 || src_h < 2 {
        return Rect { x: 0, y: 0, w: 0, h: 0 };
    }
    let view = view.clamped();

    // Largest 16:9 area that fits in the source; pan moves inside it.
    let (base_w, base_h) = if src_w as u64 * 9 >= src_h as u64 * 16 {
        ((src_h as u64 * 16 / 9) as u32, src_h)
    } else {
        (src_w, (src_w as u64 * 9 / 16) as u32)
    };
    let base_w = even_floor(base_w).max(2);
    let base_h = even_floor(base_h).max(2);
    let base_x = even_floor((src_w - base_w) / 2);
    let base_y = even_floor((src_h - base_h) / 2);

    let w = even_floor((base_w as f32 / view.zoom) as u32).clamp(2, base_w);
    let h = even_floor((base_h as f32 / view.zoom) as u32).clamp(2, base_h);

    let place = |slack: u32, pan: f32| -> u32 {
        let off = (slack as f32 * (pan + 1.0) * 0.5) as u32;
        even_floor(off.min(slack))
    };
    Rect {
        x: base_x + place(base_w - w, view.pan_x),
        y: base_y + place(base_h - h, view.pan_y),
        w,
        h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(zoom: f32, pan_x: f32, pan_y: f32) -> View {
        View { zoom, pan_x, pan_y }
    }

    #[test]
    fn crop_full_frame_at_1x() {
        assert_eq!(crop_rect(1920, 1080, View::new()), Rect { x: 0, y: 0, w: 1920, h: 1080 });
    }

    #[test]
    fn crop_2x_centred() {
        assert_eq!(crop_rect(1920, 1080, v(2.0, 0.0, 0.0)), Rect { x: 480, y: 270, w: 960, h: 540 });
    }

    #[test]
    fn crop_1_5x_is_1280x720() {
        let r = crop_rect(1920, 1080, v(1.5, 0.0, 0.0));
        assert!(r.w == 1280 && r.h == 720, "{r:?}");
    }

    #[test]
    fn crop_4x_bottom_right_stays_inside() {
        let r = crop_rect(1920, 1080, v(4.0, 1.0, 1.0));
        assert!(r.x + r.w == 1920 && r.y + r.h == 1080, "{r:?}");
    }

    #[test]
    fn crop_4_3_source_fills_16_9() {
        assert_eq!(crop_rect(640, 480, View::new()), Rect { x: 0, y: 60, w: 640, h: 360 });
    }

    #[test]
    fn crop_values_always_even() {
        for (sw, sh) in [(1920u32, 1080u32), (640, 480)] {
            let mut zoom = 1.0f32;
            while zoom <= 4.0 {
                for px in [-1.0f32, -0.33, 0.5, 1.0] {
                    for py in [-1.0f32, -0.33, 0.5, 1.0] {
                        let r = crop_rect(sw, sh, v(zoom, px, py));
                        assert!(
                            r.x % 2 == 0 && r.y % 2 == 0 && r.w % 2 == 0 && r.h % 2 == 0,
                            "odd value {r:?} at zoom {zoom} pan {px},{py}"
                        );
                        assert!(r.w >= 2 && r.h >= 2, "{r:?}");
                        assert!(r.x + r.w <= sw && r.y + r.h <= sh, "outside {r:?} in {sw}x{sh}");
                    }
                }
                zoom += 0.07;
            }
        }
    }

    #[test]
    fn crop_of_tiny_or_odd_source_does_not_panic() {
        for (sw, sh) in [(0u32, 0u32), (1, 1), (3, 3), (161, 121), (4096, 16)] {
            let r = crop_rect(sw, sh, v(4.0, 1.0, -1.0));
            assert!(r.x + r.w <= sw.max(2) && r.y + r.h <= sh.max(2), "{r:?} in {sw}x{sh}");
        }
    }

    #[test]
    fn zoom_by_clamps() {
        assert_eq!(View::new().zoom_by(10.0).zoom, 4.0);
        assert_eq!(View::new().zoom_by(-10.0).zoom, 1.0);
    }

    #[test]
    fn pan_clamps_and_nan_resets() {
        let p = View::new().pan_by(5.0, -5.0);
        assert_eq!((p.pan_x, p.pan_y), (1.0, -1.0));
        assert_eq!(v(f32::NAN, 0.0, 0.0).clamped(), View::new());
        assert_eq!(v(2.0, f32::NAN, f32::INFINITY).clamped(), View::new());
    }
}

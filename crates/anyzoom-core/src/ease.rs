//! Smooth movement of the view towards a target.

use crate::view::View;

/// Seconds for the view to cover about 63% of the remaining distance.
pub const EASE_TIME_CONSTANT: f32 = 0.12;
/// Closer than this to the target counts as arrived.
const ARRIVED: f32 = 0.0005;

/// Eases the rendered view towards the view the user asked for.
#[derive(Clone, Copy, Debug)]
pub struct Animator {
    current: View,
    target: View,
}

impl Animator {
    pub fn new(v: View) -> Self {
        let v = v.clamped();
        Animator { current: v, target: v }
    }

    pub fn set_target(&mut self, v: View) {
        self.target = v.clamped();
    }

    /// Jumps straight to `v` with no animation.
    pub fn snap(&mut self, v: View) {
        *self = Animator::new(v);
    }

    pub fn target(&self) -> View {
        self.target
    }

    /// Advances by `dt` seconds (exponential approach) and returns the view
    /// to render. A negative or non-finite `dt` leaves the view where it is.
    pub fn step(&mut self, dt: f32) -> View {
        if !(dt.is_finite() && dt > 0.0) {
            return self.current;
        }
        let alpha = 1.0 - (-dt / EASE_TIME_CONSTANT).exp();
        let approach = |from: f32, to: f32| {
            let next = from + (to - from) * alpha;
            if (to - next).abs() < ARRIVED { to } else { next }
        };
        self.current = View {
            zoom: approach(self.current.zoom, self.target.zoom),
            pan_x: approach(self.current.pan_x, self.target.pan_x),
            pan_y: approach(self.current.pan_y, self.target.pan_y),
        };
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zoom(z: f32) -> View {
        View { zoom: z, pan_x: 0.0, pan_y: 0.0 }
    }

    #[test]
    fn animator_converges() {
        let mut a = Animator::new(View::new());
        a.set_target(zoom(2.0));
        let mut last = View::new();
        for _ in 0..60 {
            last = a.step(1.0 / 30.0);
        }
        assert!((last.zoom - 2.0).abs() < 0.001, "{last:?}");
    }

    #[test]
    fn animator_never_overshoots() {
        let mut a = Animator::new(View::new());
        a.set_target(zoom(2.0));
        let mut prev = 1.0;
        for _ in 0..120 {
            let z = a.step(1.0 / 30.0).zoom;
            assert!(z >= prev && z <= 2.0, "{prev} -> {z}");
            prev = z;
        }
    }

    #[test]
    fn first_step_moves_but_does_not_arrive() {
        let mut a = Animator::new(View::new());
        a.set_target(zoom(2.0));
        let z = a.step(1.0 / 30.0).zoom;
        assert!(z > 1.1 && z < 1.5, "{z}");
    }

    #[test]
    fn huge_dt_lands_on_target() {
        let mut a = Animator::new(View::new());
        a.set_target(View { zoom: 3.0, pan_x: 0.5, pan_y: -0.5 });
        assert_eq!(a.step(1000.0), View { zoom: 3.0, pan_x: 0.5, pan_y: -0.5 });
    }

    #[test]
    fn bad_dt_does_not_move_or_poison() {
        let mut a = Animator::new(View::new());
        a.set_target(zoom(2.0));
        assert_eq!(a.step(f32::NAN), View::new());
        assert_eq!(a.step(-1.0), View::new());
    }

    #[test]
    fn target_is_clamped_and_snap_jumps() {
        let mut a = Animator::new(View::new());
        a.set_target(zoom(50.0));
        assert_eq!(a.target().zoom, 4.0);
        a.snap(zoom(2.5));
        assert_eq!(a.step(0.0), zoom(2.5));
        assert_eq!(a.target(), zoom(2.5));
    }
}

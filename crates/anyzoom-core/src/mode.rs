//! Choosing which of a camera's video modes to capture in.

/// How a mode's pictures are encoded, in order of preference when two modes
/// are otherwise equal. A USB webcam's own formats come first: on Windows an
/// NV12 mode offered next to an MJPEG one of the same size is usually made
/// by the system from the MJPEG stream, and on some cameras (Logitech Brio
/// 100) that copy arrives with black bars over both sides of the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Format {
    Other,
    Nv12,
    Yuy2,
    Mjpeg,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub index: u32,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub format: Format,
}

/// Higher is better: at least ~30 fps first, then the largest picture that is
/// not above 1080p, then the frame rate nearest 30, then the format.
pub fn mode_score(m: &Mode) -> (bool, bool, u64, i64, Format) {
    let area = m.width as u64 * m.height as u64;
    let fits = m.width <= 1920 && m.height <= 1080;
    (m.fps >= 29.0, fits, if fits { area } else { u64::MAX - area }, -((m.fps - 30.0).abs() * 100.0) as i64, m.format)
}

/// The best usable mode, or `None` when the camera offers none.
pub fn best_mode(modes: impl IntoIterator<Item = Mode>) -> Option<Mode> {
    let mut best: Option<Mode> = None;
    for mode in modes {
        if mode.width >= 2 && mode.height >= 2 && best.as_ref().is_none_or(|b| mode_score(&mode) > mode_score(b)) {
            best = Some(mode);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(index: u32, width: u32, height: u32, fps: f64, format: Format) -> Mode {
        Mode { index, width, height, fps, format }
    }

    #[test]
    fn prefers_1080p_at_30_over_smaller_and_slower() {
        let best = best_mode([
            m(0, 1280, 720, 30.0, Format::Mjpeg),
            m(1, 1920, 1080, 5.0, Format::Yuy2),
            m(2, 1920, 1080, 30.0, Format::Mjpeg),
            m(3, 640, 480, 30.0, Format::Yuy2),
        ]);
        assert_eq!(best.map(|b| b.index), Some(2));
    }

    #[test]
    fn prefers_the_cameras_own_mjpeg_over_a_system_made_nv12_copy() {
        // The Brio 100 as Windows lists it: NV12 first, MJPEG right after.
        let best = best_mode([m(325, 1920, 1080, 30.0, Format::Nv12), m(326, 1920, 1080, 30.0, Format::Mjpeg)]);
        assert_eq!(best.map(|b| b.index), Some(326));
    }

    #[test]
    fn a_larger_nv12_mode_still_beats_a_smaller_mjpeg_one() {
        let best = best_mode([m(0, 1280, 720, 30.0, Format::Mjpeg), m(1, 1920, 1080, 30.0, Format::Nv12)]);
        assert_eq!(best.map(|b| b.index), Some(1));
    }

    #[test]
    fn above_1080p_only_the_smallest_is_taken() {
        let best = best_mode([m(0, 3840, 2160, 30.0, Format::Mjpeg), m(1, 2560, 1440, 30.0, Format::Mjpeg)]);
        assert_eq!(best.map(|b| b.index), Some(1));
    }

    #[test]
    fn among_equal_modes_the_first_listed_wins() {
        let best = best_mode([m(4, 1920, 1080, 30.0, Format::Mjpeg), m(9, 1920, 1080, 30.0, Format::Mjpeg)]);
        assert_eq!(best.map(|b| b.index), Some(4));
    }

    #[test]
    fn modes_without_a_picture_are_ignored() {
        assert_eq!(best_mode([m(0, 0, 0, 30.0, Format::Mjpeg), m(1, 1, 1, 30.0, Format::Mjpeg)]), None);
    }
}

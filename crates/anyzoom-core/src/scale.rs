//! NV12 crop-and-scale.

use crate::view::Rect;

/// A borrowed NV12 image: the Y plane (`height` rows) followed by the
/// interleaved UV plane (`height / 2` rows), both `stride` bytes per row.
#[derive(Clone, Copy)]
pub struct Nv12<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScaleError {
    /// Zero or odd dimensions, or a stride narrower than the width.
    BadSize,
    SrcTooSmall,
    DstTooSmall,
    /// The crop is empty, odd, or reaches outside the source.
    CropOutside,
}

impl std::fmt::Display for ScaleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ScaleError {}

/// Bytes in a tightly packed NV12 image.
pub fn nv12_len(w: u32, h: u32) -> usize {
    w as usize * h as usize * 3 / 2
}

pub fn fill_nv12(dst: &mut [u8], w: u32, h: u32, y: u8, u: u8, v: u8) {
    let ylen = (w as usize * h as usize).min(dst.len());
    let end = nv12_len(w, h).min(dst.len());
    dst[..ylen].fill(y);
    for p in dst[ylen..end].chunks_exact_mut(2) {
        p[0] = u;
        p[1] = v;
    }
}

/// Per-output-column sample positions: left tap offset, right tap offset
/// (both in bytes within a row) and the right tap's weight out of 256.
struct Taps {
    a: Vec<u32>,
    b: Vec<u32>,
    f: Vec<u16>,
}

/// Maps `dst_n` output samples onto `src_n` input samples (pixel centres
/// aligned). Returns (index, next index, weight of next out of 256).
fn tap(d: u32, dst_n: u32, src_n: u32) -> (u32, u32, u32) {
    // position in 1/256 source samples: (d + 0.5) * src/dst - 0.5
    let pos = ((2 * d as i64 + 1) * src_n as i64 * 256) / (2 * dst_n as i64) - 128;
    let pos = pos.clamp(0, (src_n as i64 - 1) * 256);
    let i = (pos >> 8) as u32;
    (i, (i + 1).min(src_n - 1), (pos & 255) as u32)
}

fn taps(dst_n: u32, src_n: u32, bytes_per_sample: u32) -> Taps {
    let mut t = Taps { a: Vec::with_capacity(dst_n as usize), b: Vec::with_capacity(dst_n as usize), f: Vec::with_capacity(dst_n as usize) };
    for d in 0..dst_n {
        let (i, j, f) = tap(d, dst_n, src_n);
        t.a.push(i * bytes_per_sample);
        t.b.push(j * bytes_per_sample);
        t.f.push(f as u16);
    }
    t
}

/// Scales one plane. `N` is bytes per sample (1 for Y, 2 for interleaved UV).
/// `src` starts at the crop's first row; `x0` is the crop's first byte in a row.
#[allow(clippy::too_many_arguments)]
fn scale_plane<const N: usize>(
    src: &[u8],
    stride: usize,
    x0: usize,
    src_w: u32,
    src_h: u32,
    dst: &mut [u8],
    dst_w: u32,
    dst_h: u32,
) {
    let row_bytes = dst_w as usize * N;
    if src_w == dst_w && src_h == dst_h {
        for (r, out) in dst.chunks_exact_mut(row_bytes).take(dst_h as usize).enumerate() {
            let s = r * stride + x0;
            out.copy_from_slice(&src[s..s + row_bytes]);
        }
        return;
    }
    let t = taps(dst_w, src_w, N as u32);
    let src_row_bytes = src_w as usize * N;
    for (r, out) in dst.chunks_exact_mut(row_bytes).take(dst_h as usize).enumerate() {
        let (ya, yb, fy) = tap(r as u32, dst_h, src_h);
        let sa = ya as usize * stride + x0;
        let sb = yb as usize * stride + x0;
        let top = &src[sa..sa + src_row_bytes];
        let bot = &src[sb..sb + src_row_bytes];
        let ify = 256 - fy;
        for (((px, &a), &b), &f) in out.chunks_exact_mut(N).zip(&t.a).zip(&t.b).zip(&t.f) {
            let (a, b, f) = (a as usize, b as usize, f as u32);
            let ifx = 256 - f;
            for c in 0..N {
                let u = top[a + c] as u32 * ifx + top[b + c] as u32 * f;
                let l = bot[a + c] as u32 * ifx + bot[b + c] as u32 * f;
                px[c] = ((u * ify + l * fy + 32768) >> 16) as u8;
            }
        }
    }
}

/// Bilinear-scales the `crop` area of `src` into `dst`, a tightly packed NV12
/// image of `dst_w` x `dst_h`. Nothing is written when an error is returned.
pub fn scale_nv12(src: &Nv12, crop: Rect, dst: &mut [u8], dst_w: u32, dst_h: u32) -> Result<(), ScaleError> {
    let even = |v: u32| v >= 2 && v % 2 == 0;
    if !even(dst_w) || !even(dst_h) || !even(src.width) || !even(src.height) || src.stride < src.width {
        return Err(ScaleError::BadSize);
    }
    let (stride, sh) = (src.stride as usize, src.height as usize);
    // The last row of each plane only needs `width` bytes, not a full stride.
    let need = stride * (sh + sh / 2 - 1) + src.width as usize;
    if src.data.len() < need {
        return Err(ScaleError::SrcTooSmall);
    }
    let inside = even(crop.w)
        && even(crop.h)
        && crop.x % 2 == 0
        && crop.y % 2 == 0
        && crop.x.checked_add(crop.w).is_some_and(|r| r <= src.width)
        && crop.y.checked_add(crop.h).is_some_and(|b| b <= src.height);
    if !inside {
        return Err(ScaleError::CropOutside);
    }
    if dst.len() < nv12_len(dst_w, dst_h) {
        return Err(ScaleError::DstTooSmall);
    }

    let (dst_y, dst_uv) = dst.split_at_mut(dst_w as usize * dst_h as usize);
    let y_plane = &src.data[crop.y as usize * stride..];
    scale_plane::<1>(y_plane, stride, crop.x as usize, crop.w, crop.h, dst_y, dst_w, dst_h);
    let uv_plane = &src.data[stride * sh + (crop.y as usize / 2) * stride..];
    scale_plane::<2>(uv_plane, stride, crop.x as usize, crop.w / 2, crop.h / 2, dst_uv, dst_w / 2, dst_h / 2);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an NV12 buffer with the given stride; `y(col,row)` and `uv(col,row) -> (u,v)`.
    fn make(w: u32, h: u32, stride: u32, y: impl Fn(u32, u32) -> u8, uv: impl Fn(u32, u32) -> (u8, u8)) -> Vec<u8> {
        let mut d = vec![0xEEu8; (stride * h * 3 / 2) as usize];
        for r in 0..h {
            for c in 0..w {
                d[(r * stride + c) as usize] = y(c, r);
            }
        }
        for r in 0..h / 2 {
            for c in 0..w / 2 {
                let (u, v) = uv(c, r);
                let o = (stride * h + r * stride + c * 2) as usize;
                d[o] = u;
                d[o + 1] = v;
            }
        }
        d
    }

    #[test]
    fn nv12_len_is_one_and_a_half_bytes_per_pixel() {
        assert_eq!(nv12_len(1280, 720), 1_382_400);
    }

    #[test]
    fn identity_copy() {
        let (w, h, stride) = (64u32, 36u32, 80u32);
        let data = make(w, h, stride, |c, r| (c * 3 + r * 5) as u8, |c, r| ((c * 7 + r) as u8, (c + r * 11) as u8));
        let src = Nv12 { data: &data, width: w, height: h, stride };
        let mut dst = vec![0u8; nv12_len(w, h)];
        scale_nv12(&src, Rect { x: 0, y: 0, w, h }, &mut dst, w, h).unwrap();
        let packed = make(w, h, w, |c, r| (c * 3 + r * 5) as u8, |c, r| ((c * 7 + r) as u8, (c + r * 11) as u8));
        assert_eq!(dst, packed);
    }

    #[test]
    fn flat_colour_stays_flat() {
        let (w, h) = (320u32, 180u32);
        let data = make(w, h, w, |_, _| 90, |_, _| (100, 200));
        let src = Nv12 { data: &data, width: w, height: h, stride: w };
        let mut dst = vec![0u8; nv12_len(w, h)];
        scale_nv12(&src, Rect { x: 80, y: 44, w: 160, h: 90 }, &mut dst, w, h).unwrap();
        let ylen = (w * h) as usize;
        assert!(dst[..ylen].iter().all(|&b| b == 90));
        assert!(dst[ylen..].chunks(2).all(|p| p[0] == 100 && p[1] == 200));
    }

    #[test]
    fn left_half_crop() {
        // Y = column / 4 so a 640-wide ramp fits in a byte (0..=159).
        let (w, h) = (640u32, 360u32);
        let data = make(w, h, w, |c, _| (c / 4) as u8, |_, _| (128, 128));
        let src = Nv12 { data: &data, width: w, height: h, stride: w };
        let mut dst = vec![0u8; nv12_len(w, h)];
        scale_nv12(&src, Rect { x: 0, y: 0, w: 320, h: 180 }, &mut dst, w, h).unwrap();
        assert!(dst[0] < 2, "first {}", dst[0]);
        let last = dst[(w - 1) as usize] as i32;
        assert!((last - 79).abs() <= 1, "last {last}");
        let mid = dst[(w / 2) as usize] as i32;
        assert!((mid - 40).abs() <= 1, "mid {mid}");
    }

    #[test]
    fn chroma_follows_the_crop() {
        // U ramps left to right; cropping the right half must give high U values.
        let (w, h) = (640u32, 360u32);
        let data = make(w, h, w, |_, _| 50, |c, _| ((c / 2) as u8, 7));
        let src = Nv12 { data: &data, width: w, height: h, stride: w };
        let mut dst = vec![0u8; nv12_len(w, h)];
        scale_nv12(&src, Rect { x: 320, y: 180, w: 320, h: 180 }, &mut dst, w, h).unwrap();
        let uv = &dst[(w * h) as usize..];
        assert!((uv[0] as i32 - 80).abs() <= 1, "first U {}", uv[0]);
        assert_eq!(uv[1], 7);
        assert!((uv[(w - 2) as usize] as i32 - 159).abs() <= 1);
    }

    #[test]
    fn upscale_640x360_to_1280x720_no_panic() {
        let data = make(640, 360, 640, |c, r| (c ^ r) as u8, |c, r| (c as u8, r as u8));
        let src = Nv12 { data: &data, width: 640, height: 360, stride: 640 };
        let mut dst = vec![0u8; nv12_len(1280, 720)];
        scale_nv12(&src, Rect { x: 0, y: 0, w: 640, h: 360 }, &mut dst, 1280, 720).unwrap();
        // smallest legal crop, bottom-right corner
        scale_nv12(&src, Rect { x: 638, y: 358, w: 2, h: 2 }, &mut dst, 1280, 720).unwrap();
    }

    #[test]
    fn rejects_short_dst() {
        let data = make(64, 36, 64, |_, _| 0, |_, _| (0, 0));
        let src = Nv12 { data: &data, width: 64, height: 36, stride: 64 };
        let mut dst = vec![0u8; nv12_len(64, 36) - 1];
        assert_eq!(scale_nv12(&src, Rect { x: 0, y: 0, w: 64, h: 36 }, &mut dst, 64, 36), Err(ScaleError::DstTooSmall));
    }

    #[test]
    fn rejects_crop_outside() {
        let data = make(64, 36, 64, |_, _| 0, |_, _| (0, 0));
        let src = Nv12 { data: &data, width: 64, height: 36, stride: 64 };
        let mut dst = vec![0u8; nv12_len(64, 36)];
        for crop in [
            Rect { x: 2, y: 0, w: 64, h: 36 },
            Rect { x: 0, y: 2, w: 64, h: 36 },
            Rect { x: 0, y: 0, w: 0, h: 0 },
            Rect { x: u32::MAX - 1, y: 0, w: 4, h: 4 },
        ] {
            assert_eq!(scale_nv12(&src, crop, &mut dst, 64, 36), Err(ScaleError::CropOutside), "{crop:?}");
        }
    }

    #[test]
    fn rejects_short_src_and_bad_sizes() {
        let data = vec![0u8; 100];
        let src = Nv12 { data: &data, width: 64, height: 36, stride: 64 };
        let mut dst = vec![0u8; nv12_len(64, 36)];
        assert_eq!(scale_nv12(&src, Rect { x: 0, y: 0, w: 64, h: 36 }, &mut dst, 64, 36), Err(ScaleError::SrcTooSmall));
        let ok = make(64, 36, 64, |_, _| 0, |_, _| (0, 0));
        let src = Nv12 { data: &ok, width: 64, height: 36, stride: 64 };
        assert_eq!(scale_nv12(&src, Rect { x: 0, y: 0, w: 64, h: 36 }, &mut dst, 0, 36), Err(ScaleError::BadSize));
        assert_eq!(scale_nv12(&src, Rect { x: 0, y: 0, w: 64, h: 36 }, &mut dst, 63, 36), Err(ScaleError::BadSize));
        let narrow = Nv12 { data: &ok, width: 64, height: 36, stride: 32 };
        assert_eq!(scale_nv12(&narrow, Rect { x: 0, y: 0, w: 64, h: 36 }, &mut dst, 64, 36), Err(ScaleError::BadSize));
    }

    #[test]
    fn fill_sets_all_three_planes() {
        let mut d = vec![0u8; nv12_len(8, 4)];
        fill_nv12(&mut d, 8, 4, 10, 20, 30);
        assert!(d[..32].iter().all(|&b| b == 10));
        assert!(d[32..].chunks(2).all(|p| p == [20, 30]));
    }

    #[test]
    #[ignore = "timing; run in release"]
    fn bench_1080p_to_720p() {
        let data = make(1920, 1080, 1920, |c, r| (c + r) as u8, |c, r| (c as u8, r as u8));
        let src = Nv12 { data: &data, width: 1920, height: 1080, stride: 1920 };
        let mut dst = vec![0u8; nv12_len(1280, 720)];
        let n = 100;
        let t = std::time::Instant::now();
        for i in 0..n {
            let crop = Rect { x: (i % 50) * 2, y: 0, w: 1800, h: 1012 };
            scale_nv12(&src, crop, &mut dst, 1280, 720).unwrap();
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        println!("scale 1080p->720p: {ms:.2} ms/frame");
        assert!(ms < 8.0, "{ms} ms");
    }
}

//! Placeholder pictures the virtual camera shows when it has no live video.

use crate::scale::fill_nv12;

const PAPER: (u8, u8, u8) = (0xFA, 0xFA, 0xF8);
const INK: (u8, u8, u8) = (0x0B, 0x0B, 0x0C);
const COBALT: (u8, u8, u8) = (0x1F, 0x3C, 0xFF);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Card {
    /// anyzoom is running but the physical camera is unplugged or busy.
    Unavailable,
    /// No frames are arriving from the app (yet).
    Starting,
}

impl Card {
    pub fn text(self) -> &'static str {
        match self {
            Card::Unavailable => "ANYZOOM // CAMERA UNAVAILABLE",
            Card::Starting => "ANYZOOM // STARTING",
        }
    }
}

/// 8-bit RGB to video-range BT.601 Y, U, V.
pub fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let y = 16 + ((66 * r + 129 * g + 25 * b + 128) >> 8);
    let u = 128 + ((-38 * r - 74 * g + 112 * b + 128) >> 8);
    let v = 128 + ((112 * r - 94 * g - 18 * b + 128) >> 8);
    (y.clamp(0, 255) as u8, u.clamp(0, 255) as u8, v.clamp(0, 255) as u8)
}

/// A 5x7 glyph: seven rows, bit 4 is the leftmost pixel. Unknown characters are blank.
pub(crate) fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '/' => [0b00001, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b10000],
        _ => [0; 7],
    }
}

/// Draws the card into `dst`, a tightly packed NV12 image of `w` x `h`.
/// Anything that would fall outside the picture or the buffer is skipped.
pub fn draw_card(dst: &mut [u8], w: u32, h: u32, card: Card) {
    let (py, pu, pv) = rgb_to_yuv(PAPER.0, PAPER.1, PAPER.2);
    fill_nv12(dst, w, h, py, pu, pv);
    if w < 2 || h < 2 {
        return;
    }
    let (w, h) = (w as usize, h as usize);
    let even = |v: usize| v & !1;

    // cobalt bar above the text
    let (cy, cu, cv) = rgb_to_yuv(COBALT.0, COBALT.1, COBALT.2);
    let (bx, by) = (even(w * 8 / 100), even(h * 44 / 100));
    let (bw, bh) = (even((w * 6 / 100).max(2)), even((h / 90).max(2)));
    for y in by..(by + bh).min(h) {
        for x in bx..(bx + bw).min(w) {
            if let Some(p) = dst.get_mut(y * w + x) {
                *p = cy;
            }
            let uv = w * h + (y / 2) * w + even(x);
            if let Some(p) = dst.get_mut(uv..uv + 2) {
                p[0] = cu;
                p[1] = cv;
            }
        }
    }

    // text, in ink (its chroma is the same neutral grey as the paper's)
    let (iy, _, _) = rgb_to_yuv(INK.0, INK.1, INK.2);
    let text = card.text();
    let mut scale = (w / 320).max(1);
    while scale > 1 && text.len() * 6 * scale > w * 80 / 100 {
        scale -= 1;
    }
    let (tx, ty) = (bx, even(h * 48 / 100));
    for (i, ch) in text.chars().enumerate() {
        let rows = glyph(ch);
        for (gy, bits) in rows.iter().enumerate() {
            for gx in 0..5 {
                if bits & (0b10000 >> gx) == 0 {
                    continue;
                }
                let (x0, y0) = (tx + (i * 6 + gx) * scale, ty + gy * scale);
                for y in y0..(y0 + scale).min(h) {
                    for x in x0..(x0 + scale).min(w) {
                        if let Some(p) = dst.get_mut(y * w + x) {
                            *p = iy;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::nv12_len;

    fn draw(w: u32, h: u32, card: Card) -> Vec<u8> {
        let mut d = vec![0u8; nv12_len(w, h)];
        draw_card(&mut d, w, h, card);
        d
    }

    #[test]
    fn colours_convert_to_video_range() {
        assert_eq!(rgb_to_yuv(0, 0, 0), (16, 128, 128));
        assert_eq!(rgb_to_yuv(255, 255, 255), (235, 128, 128));
        let (y, u, v) = rgb_to_yuv(0x1F, 0x3C, 0xFF); // cobalt: dark, strongly blue
        assert!(y < 100 && u > 200 && v < 128, "{y} {u} {v}");
    }

    #[test]
    fn background_is_paper_and_text_is_ink() {
        let (w, h) = (1280u32, 720u32);
        let d = draw(w, h, Card::Unavailable);
        let (paper, _, _) = rgb_to_yuv(0xFA, 0xFA, 0xF8);
        let (ink, _, _) = rgb_to_yuv(0x0B, 0x0B, 0x0C);
        let y = &d[..(w * h) as usize];
        assert_eq!(y[0], paper);
        assert_eq!(y[y.len() - 1], paper);
        let ink_px = y.iter().filter(|&&b| b == ink).count();
        assert!(ink_px > 2000, "only {ink_px} ink pixels");
        assert!(ink_px < y.len() / 20, "text should be small");
    }

    #[test]
    fn has_a_cobalt_bar() {
        let (w, h) = (1280u32, 720u32);
        let d = draw(w, h, Card::Starting);
        let (_, cu, cv) = rgb_to_yuv(0x1F, 0x3C, 0xFF);
        let n = d[(w * h) as usize..].chunks(2).filter(|p| p[0] == cu && p[1] == cv).count();
        assert!(n > 50, "{n} cobalt chroma samples");
    }

    #[test]
    fn the_two_cards_differ() {
        assert_ne!(draw(1280, 720, Card::Unavailable), draw(1280, 720, Card::Starting));
    }

    #[test]
    fn text_stays_inside_the_picture() {
        for (w, h) in [(1280u32, 720u32), (1920, 1080), (640, 360)] {
            let d = draw(w, h, Card::Unavailable);
            let (paper, _, _) = rgb_to_yuv(0xFA, 0xFA, 0xF8);
            for row in 0..h {
                for col in (w - w / 20)..w {
                    assert_eq!(d[(row * w + col) as usize], paper, "{w}x{h} at {col},{row}");
                }
            }
        }
    }

    #[test]
    fn odd_sizes_and_short_buffers_do_not_panic() {
        for (w, h) in [(2u32, 2u32), (64, 36), (16, 400), (400, 16), (0, 0)] {
            draw(w, h, Card::Unavailable);
        }
        let mut short = vec![0u8; 100];
        draw_card(&mut short, 1280, 720, Card::Starting);
    }

    #[test]
    fn every_letter_used_has_a_glyph() {
        for card in [Card::Unavailable, Card::Starting] {
            for ch in card.text().chars() {
                let g = glyph(ch);
                assert!(ch == ' ' || g.iter().any(|&row| row != 0), "no glyph for {ch:?}");
            }
        }
        assert_eq!(Card::Unavailable.text(), "ANYZOOM // CAMERA UNAVAILABLE");
        assert_eq!(Card::Starting.text(), "ANYZOOM // STARTING");
    }
}

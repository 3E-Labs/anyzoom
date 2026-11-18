//! Layout of the memory shared between the app and the virtual camera.
//!
//! The app (producer) writes frames; the virtual camera (consumer) reads
//! them and writes back a "someone is watching" tick. Everything works on
//! plain byte slices so it can be tested anywhere; the Windows crates supply
//! the mapped memory. Both sides must use the same clock for `now_ms`
//! (on Windows: `GetTickCount64`).
//!
//! Header, little-endian, 64 bytes:
//! `magic[4] | width u32 | height u32 | seq u32 | producer_tick_ms u64 |
//! consumer_tick_ms u64 | status u32 | reserved`.
//! `seq` is odd while a frame is being written.

use crate::scale::nv12_len;

pub const BUS_MAGIC: [u8; 4] = *b"AZ02";
pub const BUS_HEADER: usize = 64;
pub const BUS_MAX_W: u32 = 1920;
pub const BUS_MAX_H: u32 = 1080;
/// The frame is live camera video.
pub const STATUS_LIVE: u32 = 0;
/// The physical camera is unplugged or in use elsewhere.
pub const STATUS_UNAVAILABLE: u32 = 1;
/// A frame older than this is no longer shown.
pub const PRODUCER_STALE_MS: u64 = 1000;
/// The consumer counts as watching for this long after its last tick.
pub const CONSUMER_ACTIVE_MS: u64 = 2000;

const OFF_W: usize = 4;
const OFF_H: usize = 8;
const OFF_SEQ: usize = 12;
const OFF_PRODUCER: usize = 16;
const OFF_CONSUMER: usize = 24;
const OFF_STATUS: usize = 32;

/// Size of the shared file: header plus the largest frame.
pub fn bus_file_len() -> usize {
    BUS_HEADER + nv12_len(BUS_MAX_W, BUS_MAX_H)
}

fn get_u32(map: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(map[off..off + 4].try_into().unwrap())
}

fn get_u64(map: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(map[off..off + 8].try_into().unwrap())
}

fn dims_ok(w: u32, h: u32) -> bool {
    (2..=BUS_MAX_W).contains(&w) && (2..=BUS_MAX_H).contains(&h) && w % 2 == 0 && h % 2 == 0
}

/// Publishes one frame. `fill` receives exactly the frame's bytes to write
/// into. Returns false, without calling `fill`, when the size is not allowed
/// or the map is too small.
pub fn write_frame(map: &mut [u8], w: u32, h: u32, status: u32, now_ms: u64, fill: impl FnOnce(&mut [u8])) -> bool {
    if !dims_ok(w, h) || map.len() < BUS_HEADER + nv12_len(w, h) {
        return false;
    }
    let writing = get_u32(map, OFF_SEQ) | 1;
    map[OFF_SEQ..OFF_SEQ + 4].copy_from_slice(&writing.to_le_bytes());
    map[..4].copy_from_slice(&BUS_MAGIC);
    map[OFF_W..OFF_W + 4].copy_from_slice(&w.to_le_bytes());
    map[OFF_H..OFF_H + 4].copy_from_slice(&h.to_le_bytes());
    map[OFF_STATUS..OFF_STATUS + 4].copy_from_slice(&status.to_le_bytes());
    fill(&mut map[BUS_HEADER..BUS_HEADER + nv12_len(w, h)]);
    map[OFF_PRODUCER..OFF_PRODUCER + 8].copy_from_slice(&now_ms.to_le_bytes());
    map[OFF_SEQ..OFF_SEQ + 4].copy_from_slice(&writing.wrapping_add(1).to_le_bytes());
    true
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReadResult {
    /// `out` holds a complete frame of this size.
    Frame { w: u32, h: u32, status: u32 },
    /// The producer has stopped, or no complete frame could be read.
    Stale,
    /// Not a frame file, or its header makes no sense.
    Invalid,
}

/// Copies a consistent frame into `out`, retrying a few times if the
/// producer is mid-write. Never reads outside `map`, whatever the header says.
pub fn read_frame(map: &[u8], now_ms: u64, out: &mut Vec<u8>) -> ReadResult {
    if map.len() < BUS_HEADER || map[..4] != BUS_MAGIC {
        return ReadResult::Invalid;
    }
    for _ in 0..3 {
        let seq = get_u32(map, OFF_SEQ);
        if seq % 2 == 1 {
            std::hint::spin_loop();
            continue;
        }
        let (w, h) = (get_u32(map, OFF_W), get_u32(map, OFF_H));
        if !dims_ok(w, h) || map.len() < BUS_HEADER + nv12_len(w, h) {
            return ReadResult::Invalid;
        }
        let status = get_u32(map, OFF_STATUS);
        let tick = get_u64(map, OFF_PRODUCER);
        out.clear();
        out.extend_from_slice(&map[BUS_HEADER..BUS_HEADER + nv12_len(w, h)]);
        if get_u32(map, OFF_SEQ) != seq {
            continue;
        }
        if now_ms.saturating_sub(tick) > PRODUCER_STALE_MS {
            return ReadResult::Stale;
        }
        return ReadResult::Frame { w, h, status };
    }
    ReadResult::Stale
}

/// Called by the virtual camera each time an app asks it for a frame.
pub fn touch_consumer(map: &mut [u8], now_ms: u64) {
    if map.len() >= BUS_HEADER {
        map[OFF_CONSUMER..OFF_CONSUMER + 8].copy_from_slice(&now_ms.to_le_bytes());
    }
}

/// True while some app is reading from the virtual camera.
pub fn consumer_active(map: &[u8], now_ms: u64) -> bool {
    if map.len() < BUS_HEADER {
        return false;
    }
    let tick = get_u64(map, OFF_CONSUMER);
    tick != 0 && now_ms.saturating_sub(tick) <= CONSUMER_ACTIVE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Vec<u8> {
        vec![0u8; bus_file_len()]
    }

    fn pattern(buf: &mut [u8]) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
    }

    #[test]
    fn file_len_holds_a_1080p_frame() {
        assert_eq!(bus_file_len(), 64 + 1920 * 1080 * 3 / 2);
    }

    #[test]
    fn roundtrip_720p() {
        let mut m = map();
        assert!(write_frame(&mut m, 1280, 720, 0, 5_000, pattern));
        let mut out = Vec::new();
        assert_eq!(read_frame(&m, 5_010, &mut out), ReadResult::Frame { w: 1280, h: 720, status: 0 });
        assert_eq!(out.len(), nv12_len(1280, 720));
        let mut want = vec![0u8; nv12_len(1280, 720)];
        pattern(&mut want);
        assert_eq!(out, want);
    }

    #[test]
    fn fill_gets_exactly_the_frame_bytes() {
        let mut m = map();
        let mut seen = 0;
        write_frame(&mut m, 1920, 1080, 0, 1, |b| seen = b.len());
        assert_eq!(seen, nv12_len(1920, 1080));
    }

    #[test]
    fn status_is_carried() {
        let mut m = map();
        write_frame(&mut m, 1280, 720, 1, 100, |_| {});
        let mut out = Vec::new();
        assert_eq!(read_frame(&m, 100, &mut out), ReadResult::Frame { w: 1280, h: 720, status: 1 });
    }

    #[test]
    fn stale_after_1s() {
        let mut m = map();
        write_frame(&mut m, 1280, 720, 0, 5_000, pattern);
        let mut out = Vec::new();
        assert!(matches!(read_frame(&m, 6_000, &mut out), ReadResult::Frame { .. }));
        assert_eq!(read_frame(&m, 6_001, &mut out), ReadResult::Stale);
        // a reader whose clock is behind the writer's still gets the frame
        assert!(matches!(read_frame(&m, 4_000, &mut out), ReadResult::Frame { .. }));
    }

    #[test]
    fn empty_file_and_bad_magic_are_invalid() {
        let mut m = map();
        let mut out = Vec::new();
        assert_eq!(read_frame(&m, 0, &mut out), ReadResult::Invalid);
        write_frame(&mut m, 1280, 720, 0, 10, pattern);
        m[0] = b'X';
        assert_eq!(read_frame(&m, 10, &mut out), ReadResult::Invalid);
    }

    #[test]
    fn oversized_dims_are_invalid() {
        let mut m = map();
        write_frame(&mut m, 1280, 720, 0, 10, pattern);
        m[4..8].copy_from_slice(&4000u32.to_le_bytes());
        m[8..12].copy_from_slice(&4000u32.to_le_bytes());
        let mut out = Vec::new();
        assert_eq!(read_frame(&m, 10, &mut out), ReadResult::Invalid);
        for (w, h) in [(0u32, 720u32), (1280, 0), (1281, 720), (u32::MAX, u32::MAX)] {
            m[4..8].copy_from_slice(&w.to_le_bytes());
            m[8..12].copy_from_slice(&h.to_le_bytes());
            assert_eq!(read_frame(&m, 10, &mut out), ReadResult::Invalid, "{w}x{h}");
        }
    }

    #[test]
    fn short_map_is_invalid() {
        let mut m = map();
        write_frame(&mut m, 1280, 720, 0, 10, pattern);
        let mut out = Vec::new();
        assert_eq!(read_frame(&m[..10], 10, &mut out), ReadResult::Invalid);
        assert_eq!(read_frame(&m[..BUS_HEADER + 100], 10, &mut out), ReadResult::Invalid);
        assert_eq!(read_frame(&[], 10, &mut out), ReadResult::Invalid);
    }

    #[test]
    fn write_refuses_bad_input_without_panicking() {
        let mut m = map();
        assert!(!write_frame(&mut m, 4000, 4000, 0, 10, |_| panic!("fill must not run")));
        assert!(!write_frame(&mut m, 1281, 720, 0, 10, |_| panic!("fill must not run")));
        let mut short = vec![0u8; 1000];
        assert!(!write_frame(&mut short, 1280, 720, 0, 10, |_| panic!("fill must not run")));
    }

    #[test]
    fn odd_seq_is_not_returned_as_frame() {
        let mut m = map();
        write_frame(&mut m, 1280, 720, 0, 10, pattern);
        let seq = u32::from_le_bytes(m[12..16].try_into().unwrap());
        assert_eq!(seq % 2, 0);
        m[12..16].copy_from_slice(&(seq + 1).to_le_bytes());
        let mut out = Vec::new();
        assert_eq!(read_frame(&m, 10, &mut out), ReadResult::Stale);
    }

    #[test]
    fn seq_advances_by_two_per_frame_and_wraps() {
        let mut m = map();
        write_frame(&mut m, 1280, 720, 0, 10, pattern);
        let a = u32::from_le_bytes(m[12..16].try_into().unwrap());
        write_frame(&mut m, 1280, 720, 0, 20, pattern);
        let b = u32::from_le_bytes(m[12..16].try_into().unwrap());
        assert_eq!(b, a + 2);
        m[12..16].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
        write_frame(&mut m, 1280, 720, 0, 30, pattern);
        assert_eq!(u32::from_le_bytes(m[12..16].try_into().unwrap()), 0);
    }

    #[test]
    fn consumer_tick_window() {
        let mut m = map();
        assert!(!consumer_active(&m, 50), "never touched");
        touch_consumer(&mut m, 10_000);
        assert!(consumer_active(&m, 10_000));
        assert!(consumer_active(&m, 12_000));
        assert!(!consumer_active(&m, 12_001));
        assert!(!consumer_active(&[], 0));
        touch_consumer(&mut [], 5); // must not panic
    }

    #[test]
    fn frames_do_not_disturb_the_consumer_tick() {
        let mut m = map();
        touch_consumer(&mut m, 10_000);
        write_frame(&mut m, 1280, 720, 0, 10_100, pattern);
        assert!(consumer_active(&m, 10_200));
    }
}

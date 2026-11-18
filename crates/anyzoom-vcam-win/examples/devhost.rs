//! Development host: starts the virtual camera and feeds it a moving test
//! pattern, so the camera component can be checked without a real webcam.
//!
//! devhost [seconds] [--freeze-after N] [--unavailable-after N] [--size 1080]

fn main() {
    #[cfg(windows)]
    win::run();
}

#[cfg(windows)]
mod win {
    use anyzoom_core::bus::{consumer_active, write_frame, STATUS_LIVE, STATUS_UNAVAILABLE};
    use anyzoom_vcam::{host::VirtualCamera, mapping::FrameMap, now_ms};
    use std::time::{Duration, Instant};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

    fn arg_after(args: &[String], flag: &str) -> Option<u64> {
        args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok())
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let seconds: u64 = args.first().and_then(|s| s.parse().ok()).unwrap_or(30);
        let freeze_after = arg_after(&args, "--freeze-after");
        let unavailable_after = arg_after(&args, "--unavailable-after");
        let (w, h) = if arg_after(&args, "--size") == Some(1080) { (1920u32, 1080u32) } else { (1280, 720) };

        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().expect("COM") };
        let mut map = FrameMap::create_producer().expect("frame file");
        println!("frame file ready, writable={}", map.writable());
        let camera = VirtualCamera::start().expect("virtual camera (is the component registered?)");
        println!("virtual camera started; running {seconds}s at {w}x{h}");

        let start = Instant::now();
        let mut frame = 0usize;
        let mut last_report = 0u64;
        while start.elapsed().as_secs() < seconds {
            let elapsed = start.elapsed().as_secs();
            let frozen = freeze_after.is_some_and(|n| elapsed >= n);
            let status = if unavailable_after.is_some_and(|n| elapsed >= n) { STATUS_UNAVAILABLE } else { STATUS_LIVE };
            let bytes = map.bytes_mut().expect("writable map");
            if !frozen {
                write_frame(bytes, w, h, status, now_ms(), |buf| {
                    let (w, h) = (w as usize, h as usize);
                    let (y, uv) = buf.split_at_mut(w * h);
                    for (row, line) in y.chunks_exact_mut(w).enumerate() {
                        for (col, px) in line.iter_mut().enumerate() {
                            *px = if ((col + frame * 4) / 80 + row / 80) % 2 == 0 { 200 } else { 60 };
                        }
                    }
                    for pair in uv.chunks_exact_mut(2) {
                        pair[0] = 90;
                        pair[1] = 200;
                    }
                });
                frame += 1;
            }
            if elapsed != last_report {
                last_report = elapsed;
                println!("t={elapsed}s consumer_active={} frozen={frozen} status={status}", consumer_active(map.bytes(), now_ms()));
            }
            std::thread::sleep(Duration::from_millis(33));
        }
        camera.remove();
        println!("done");
    }
}

//! grab <name-fragment> <out.nv12> : lists cameras, reads 30 frames from the
//! one whose name contains the fragment, and saves the last as raw NV12.

fn main() {
    #[cfg(windows)]
    {
        use anyzoom_capture_win::{list_cameras, Capture};
        let args: Vec<String> = std::env::args().skip(1).collect();
        let cameras = list_cameras().expect("list cameras");
        for c in &cameras {
            println!("camera: {} | {}", c.name, c.id);
        }
        let Some(fragment) = args.first() else { return };
        let Some(camera) = cameras.iter().find(|c| c.name.contains(fragment.as_str())) else {
            println!("open failed: {:?}", Capture::open("no-such-camera").err());
            return;
        };
        for line in anyzoom_capture_win::describe_modes(&camera.id).unwrap_or_default() {
            if line.contains("1920x1080") { println!("mode {line}"); }
        }
        let mut capture = match Capture::open(&camera.id) {
            Ok(c) => c,
            Err(e) => {
                println!("open failed: {e:?}");
                return;
            }
        };
        let (w, h) = capture.size();
        println!("size: {w}x{h}");
        let mut last = Vec::new();
        let mut got = 0;
        let mut tries = 0;
        while got < 30 && tries < 200 {
            tries += 1;
            match capture.read(1000, |frame| {
                last.clear();
                let (stride, w, h) = (frame.stride as usize, frame.width as usize, frame.height as usize);
                for row in 0..h + h / 2 {
                    last.extend_from_slice(&frame.data[row * stride..row * stride + w]);
                }
            }) {
                Ok(true) => got += 1,
                Ok(false) => {}
                Err(e) => {
                    println!("read failed: {e:?}");
                    return;
                }
            }
        }
        let (w, h) = capture.size();
        let luma = &last[..(w * h) as usize];
        let avg = luma.iter().map(|&b| b as u64).sum::<u64>() / luma.len().max(1) as u64;
        println!("frames: {got}, final size {w}x{h}, average Y of last frame: {avg}");
        if let Some(path) = args.get(1) {
            std::fs::write(path, &last).expect("write output");
        }
    }
}

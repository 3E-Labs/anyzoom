//! headless <camera-name-fragment|-> <zoom> <seconds> [--preview out.jpg]
//! Runs the whole pipeline without a window.

fn main() {
    #[cfg(windows)]
    {
        use anyzoom_core::{Settings, View};
        use anyzoom_engine::Engine;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let fragment = args.first().cloned().unwrap_or_else(|| "-".into());
        let zoom: f32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.0);
        let seconds: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
        let preview_out = args.iter().position(|a| a == "--preview").and_then(|i| args.get(i + 1)).cloned();

        let mut settings = Settings::default();
        if fragment != "-" {
            let cameras = anyzoom_capture_win::list_cameras().expect("list cameras");
            match cameras.iter().find(|c| c.name.contains(fragment.as_str())) {
                Some(c) => settings.camera_id = Some(c.id.clone()),
                None => settings.camera_id = Some(format!("missing:{fragment}")),
            }
        }
        let engine = Engine::start(settings).expect("engine");
        engine.set_target(View { zoom, pan_x: 0.0, pan_y: 0.0 });
        engine.set_preview_wanted(preview_out.is_some());
        for t in 0..seconds {
            std::thread::sleep(std::time::Duration::from_secs(1));
            let s = engine.status();
            println!(
                "t={t}s open={} watched={} fps={:.1} src={:?} error={:?} vcam_error={:?}",
                s.camera_open, s.consumer_active, s.fps, s.src_size, s.camera_error, s.vcam_error
            );
            if let (Some(path), Some((jpeg, crop, src))) = (&preview_out, engine.preview()) {
                std::fs::write(path, &jpeg).expect("write preview");
                println!("  preview {} bytes, crop {crop:?} of {src:?}", jpeg.len());
            }
        }
        engine.shutdown();
        println!("done");
    }
}

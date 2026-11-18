//! The running pipeline: reads the physical camera, applies the zoom, and
//! publishes the result to the virtual camera. Everything happens on one
//! worker thread; `Engine` is a cheap handle that talks to it.
#![cfg(windows)]

use anyzoom_capture_win::{list_cameras, Capture, CaptureError};
use anyzoom_core::bus::{consumer_active, write_frame, STATUS_LIVE, STATUS_UNAVAILABLE};
use anyzoom_core::{crop_rect, scale_nv12, Animator, Nv12, Output, Rect, Settings, View, PAN_STEP, ZOOM_STEP};
use anyzoom_vcam::{host::VirtualCamera, mapping::FrameMap, now_ms};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The physical camera is released this long after the last viewer goes away.
const CLOSE_AFTER: Duration = Duration::from_secs(3);
/// How often to try an unavailable camera again.
const RETRY_EVERY: Duration = Duration::from_secs(2);
/// How often the "camera unavailable" notice is refreshed for the virtual camera.
const NOTICE_EVERY: Duration = Duration::from_millis(500);
/// An open camera that delivers nothing for this long is treated as gone.
const SILENT_LIMIT: Duration = Duration::from_secs(3);
const PREVIEW_EVERY: Duration = Duration::from_millis(66);
const PREVIEW_WIDTH: u32 = 640;

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct EngineStatus {
    pub camera_open: bool,
    /// Why the physical camera cannot be used right now, if it cannot.
    pub camera_error: Option<String>,
    /// Some app is reading from the virtual camera.
    pub consumer_active: bool,
    pub src_size: Option<(u32, u32)>,
    pub fps: f32,
    /// The view the user asked for (the picture eases towards it).
    pub view: View,
    /// Why the virtual camera could not be started, if it could not.
    pub vcam_error: Option<String>,
}

#[derive(Debug)]
pub enum EngineError {
    Thread(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Thread(e) => write!(f, "could not start the video thread: {e}"),
        }
    }
}
impl std::error::Error for EngineError {}

/// A preview picture: JPEG bytes of the whole camera image, the part of the
/// camera image currently shown (in camera pixels), and the camera image size.
pub type Preview = (Vec<u8>, Rect, (u32, u32));

struct State {
    camera_id: Option<String>,
    /// Bumped whenever the chosen camera changes, so the worker reopens.
    camera_generation: u64,
    output: Output,
    target: View,
    preview_wanted: bool,
    status: EngineStatus,
    preview: Option<Preview>,
}

struct Shared {
    state: Mutex<State>,
    stop: AtomicBool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[derive(Clone)]
pub struct Engine {
    shared: Arc<Shared>,
    worker: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Engine {
    /// Creates the frame file, starts the virtual camera and the worker. A
    /// virtual camera that fails to start is reported in `status().vcam_error`
    /// and does not stop the preview from working.
    pub fn start(settings: Settings) -> Result<Engine, EngineError> {
        let view = settings.view.clamped();
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                camera_id: settings.camera_id,
                camera_generation: 0,
                output: settings.output,
                target: view,
                preview_wanted: false,
                status: EngineStatus { view, ..Default::default() },
                preview: None,
            }),
            stop: AtomicBool::new(false),
        });
        let for_worker = shared.clone();
        let worker = std::thread::Builder::new()
            .name("anyzoom-video".into())
            .spawn(move || Worker::run(for_worker))
            .map_err(|e| EngineError::Thread(e.to_string()))?;
        Ok(Engine { shared, worker: Arc::new(Mutex::new(Some(worker))) })
    }

    /// `None` means "the first camera found".
    pub fn set_camera(&self, id: Option<String>) {
        let mut s = self.shared.lock();
        if s.camera_id != id {
            s.camera_id = id;
            s.camera_generation += 1;
        }
    }

    pub fn set_output(&self, output: Output) {
        self.shared.lock().output = output;
    }

    pub fn set_target(&self, view: View) {
        let mut s = self.shared.lock();
        s.target = view.clamped();
        s.status.view = s.target;
    }

    fn change_target(&self, f: impl FnOnce(View) -> View) {
        let mut s = self.shared.lock();
        s.target = f(s.target).clamped();
        s.status.view = s.target;
    }

    /// One step is 0.1x; negative zooms out.
    pub fn nudge_zoom(&self, steps: i32) {
        self.change_target(|v| v.zoom_by(steps as f32 * ZOOM_STEP));
    }

    /// One step is a tenth of the way across; positive is right / down.
    pub fn nudge_pan(&self, dx: i32, dy: i32) {
        self.change_target(|v| v.pan_by(dx as f32 * PAN_STEP, dy as f32 * PAN_STEP));
    }

    pub fn reset(&self) {
        self.change_target(|_| View::new());
    }

    /// While true the physical camera stays on so the window can show it.
    pub fn set_preview_wanted(&self, on: bool) {
        let mut s = self.shared.lock();
        s.preview_wanted = on;
        if !on {
            s.preview = None;
        }
    }

    pub fn status(&self) -> EngineStatus {
        self.shared.lock().status.clone()
    }

    /// The latest preview picture, if the camera is delivering one.
    pub fn preview(&self) -> Option<Preview> {
        self.shared.lock().preview.clone()
    }

    /// Stops the worker, releases the physical camera and removes the virtual camera.
    pub fn shutdown(self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        let handle = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
}

struct Worker {
    shared: Arc<Shared>,
    map: Option<FrameMap>,
    capture: Option<Capture>,
    generation: u64,
    animator: Animator,
    last_frame: Instant,
    last_needed: Option<Instant>,
    next_open_try: Instant,
    last_notice: Option<Instant>,
    last_preview: Option<Instant>,
    fps_window: (Instant, u32),
    preview_scratch: Vec<u8>,
}

impl Worker {
    fn run(shared: Arc<Shared>) {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        let now = Instant::now();
        let view = shared.lock().target;
        let mut worker = Worker {
            shared,
            map: None,
            capture: None,
            generation: 0,
            animator: Animator::new(view),
            last_frame: now,
            last_needed: None,
            next_open_try: now,
            last_notice: None,
            last_preview: None,
            fps_window: (now, 0),
            preview_scratch: Vec::new(),
        };

        // The frame file must exist before the camera starts asking for frames.
        let mut vcam_error = None;
        match FrameMap::create_producer() {
            Ok(map) => worker.map = Some(map),
            Err(e) => vcam_error = Some(format!("could not create the frame file: {}", e.message())),
        }
        let camera = if vcam_error.is_none() {
            match VirtualCamera::start() {
                Ok(camera) => Some(camera),
                Err(e) => {
                    vcam_error = Some(format!("could not start the virtual camera: {} ({:#010X})", e.message(), e.code().0 as u32));
                    None
                }
            }
        } else {
            None
        };
        worker.shared.lock().status.vcam_error = vcam_error;

        while !worker.shared.stop.load(Ordering::SeqCst) {
            worker.tick();
        }
        worker.capture = None;
        if let Some(camera) = camera {
            camera.remove();
        }
    }

    fn close_camera(&mut self, error: Option<String>) {
        self.capture = None;
        let mut s = self.shared.lock();
        s.status.camera_open = false;
        s.status.camera_error = error;
        s.status.fps = 0.0;
        s.preview = None;
    }

    fn tick(&mut self) {
        let now = Instant::now();
        let (camera_id, generation, output, target, preview_wanted) = {
            let s = self.shared.lock();
            (s.camera_id.clone(), s.camera_generation, s.output, s.target, s.preview_wanted)
        };
        let watched = self.map.as_ref().is_some_and(|m| consumer_active(m.bytes(), now_ms()));
        self.shared.lock().status.consumer_active = watched;
        if preview_wanted || watched {
            self.last_needed = Some(now);
        }
        let wanted = self.last_needed.is_some_and(|t| now.duration_since(t) < CLOSE_AFTER);

        if generation != self.generation {
            self.generation = generation;
            self.close_camera(None);
            self.next_open_try = now;
        }
        if !wanted {
            // Nobody is looking: keep the physical camera (and its light) off.
            if self.capture.is_some() || self.shared.lock().status.camera_error.is_some() {
                self.close_camera(None);
            }
            self.next_open_try = now;
            std::thread::sleep(Duration::from_millis(100));
            return;
        }

        if self.capture.is_none() {
            if now >= self.next_open_try {
                self.next_open_try = now + RETRY_EVERY;
                match open_camera(camera_id.as_deref()) {
                    Ok(capture) => {
                        let size = capture.size();
                        self.capture = Some(capture);
                        self.animator.snap(self.animator.target());
                        self.last_frame = now;
                        self.fps_window = (now, 0);
                        let mut s = self.shared.lock();
                        s.status.camera_open = true;
                        s.status.camera_error = None;
                        s.status.src_size = Some(size);
                    }
                    Err(e) => self.close_camera(Some(e.to_string())),
                }
            }
            if self.capture.is_none() {
                self.publish_unavailable(output, now);
                std::thread::sleep(Duration::from_millis(50));
                return;
            }
        }

        self.animator.set_target(target);
        let dt = now.duration_since(self.last_frame);
        let want_preview = preview_wanted && self.last_preview.is_none_or(|t| now.duration_since(t) >= PREVIEW_EVERY);
        let (out_w, out_h) = output.size();
        let mut preview = None;
        let animator = &mut self.animator;
        let map = &mut self.map;
        let scratch = &mut self.preview_scratch;
        let Some(capture) = self.capture.as_mut() else { return };
        let read = capture.read(200, |frame| {
            let view = animator.step(dt.as_secs_f32());
            let crop = crop_rect(frame.width, frame.height, view);
            if let Some(bytes) = map.as_mut().and_then(|m| m.bytes_mut()) {
                write_frame(bytes, out_w, out_h, STATUS_LIVE, now_ms(), |dst| {
                    let _ = scale_nv12(frame, crop, dst, out_w, out_h);
                });
            }
            if want_preview {
                preview = encode_preview(frame, scratch).map(|jpeg| (jpeg, crop, (frame.width, frame.height)));
            }
        });
        match read {
            Ok(true) => {
                self.last_frame = now;
                self.fps_window.1 += 1;
                let elapsed = now.duration_since(self.fps_window.0);
                let mut s = self.shared.lock();
                if elapsed >= Duration::from_secs(1) {
                    s.status.fps = self.fps_window.1 as f32 / elapsed.as_secs_f32();
                    self.fps_window = (now, 0);
                }
                if let Some(preview) = preview {
                    s.preview = Some(preview);
                    self.last_preview = Some(now);
                }
            }
            Ok(false) => {
                if now.duration_since(self.last_frame) > SILENT_LIMIT {
                    self.close_camera(Some(CaptureError::Disconnected.to_string()));
                }
            }
            Err(e) => {
                self.close_camera(Some(e.to_string()));
                self.next_open_try = now + RETRY_EVERY;
            }
        }
    }

    /// Tells the virtual camera to show its "camera unavailable" card.
    fn publish_unavailable(&mut self, output: Output, now: Instant) {
        if self.last_notice.is_some_and(|t| now.duration_since(t) < NOTICE_EVERY) {
            return;
        }
        self.last_notice = Some(now);
        let (w, h) = output.size();
        if let Some(bytes) = self.map.as_mut().and_then(|m| m.bytes_mut()) {
            write_frame(bytes, w, h, STATUS_UNAVAILABLE, now_ms(), |_| {});
        }
    }
}

fn open_camera(id: Option<&str>) -> Result<Capture, CaptureError> {
    match id {
        Some(id) => Capture::open(id),
        None => {
            let first = list_cameras()?.into_iter().next().ok_or(CaptureError::NotFound)?;
            Capture::open(&first.id)
        }
    }
}

/// Shrinks the whole camera image to at most 640 px wide and encodes it as JPEG.
fn encode_preview(frame: &Nv12, scratch: &mut Vec<u8>) -> Option<Vec<u8>> {
    let (src_w, src_h) = (frame.width & !1, frame.height & !1);
    if src_w < 2 || src_h < 2 {
        return None;
    }
    let w = PREVIEW_WIDTH.min(src_w) & !1;
    let h = ((w as u64 * src_h as u64 / src_w as u64) as u32 & !1).max(2);
    scratch.resize(anyzoom_core::nv12_len(w, h), 0);
    scale_nv12(frame, Rect { x: 0, y: 0, w: src_w, h: src_h }, scratch, w, h).ok()?;

    let (w, h) = (w as usize, h as usize);
    let (luma, chroma) = scratch.split_at(w * h);
    let mut rgb = vec![0u8; w * h * 3];
    for row in 0..h {
        let uv_row = &chroma[(row / 2) * w..(row / 2) * w + w];
        for col in 0..w {
            let y = (luma[row * w + col] as i32 - 16) * 298;
            let u = uv_row[col & !1] as i32 - 128;
            let v = uv_row[(col & !1) + 1] as i32 - 128;
            let px = &mut rgb[(row * w + col) * 3..][..3];
            px[0] = ((y + 409 * v + 128) >> 8).clamp(0, 255) as u8;
            px[1] = ((y - 100 * u - 208 * v + 128) >> 8).clamp(0, 255) as u8;
            px[2] = ((y + 516 * u + 128) >> 8).clamp(0, 255) as u8;
        }
    }
    let mut jpeg = Vec::with_capacity(48 * 1024);
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 72);
    image::ImageEncoder::write_image(encoder, &rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8).ok()?;
    Some(jpeg)
}

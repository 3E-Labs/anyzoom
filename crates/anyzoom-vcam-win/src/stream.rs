//! The camera's single video stream: hands out one NV12 frame per request.

use crate::mapping::FrameMap;
use crate::now_ms;
use anyzoom_core::bus::{read_frame, touch_consumer, ReadResult, STATUS_LIVE};
use anyzoom_core::{draw_card, nv12_len, scale_nv12, Card, Nv12, Rect};
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::KernelStreaming::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;

/// Frame sizes the camera offers, default first.
pub(crate) const SIZES: [(u32, u32); 2] = [(1280, 720), (1920, 1080)];
/// Requests between attempts to (re)open the frame file: about one second.
const REOPEN_EVERY: u32 = 30;
/// One frame at 30 fps, in the 100 ns units Media Foundation uses.
const FRAME_100NS: u64 = 333_333;

/// Where frames come from, plus everything reused between requests.
#[derive(Default)]
struct Feed {
    requests: u32,
    map: Option<FrameMap>,
    /// Requests in a row that found no usable frame.
    misses: u32,
    scratch: Vec<u8>,
    /// The last complete live frame and its size, repeated when a request
    /// lands while the app is in the middle of writing the next one.
    good: Vec<u8>,
    good_size: Option<(u32, u32)>,
    card: Option<(u32, u32, Card, Vec<u8>)>,
}

impl Feed {
    /// Writes the next `w` x `h` frame into `dst`.
    fn fill(&mut self, dst: &mut [u8], w: u32, h: u32) {
        self.requests = self.requests.wrapping_add(1);
        // No file yet, or a long run of misses (the file may have been
        // replaced by a reinstall): look for it again about once a second.
        if (self.map.is_none() || self.misses >= REOPEN_EVERY) && self.requests % REOPEN_EVERY == 1 {
            self.map = FrameMap::open_consumer();
            self.misses = 0;
        }
        let now = now_ms();
        let mut card = Card::Starting;
        if let Some(map) = self.map.as_mut() {
            // Tell the app someone is watching, so it knows to run the real camera.
            if let Some(bytes) = map.bytes_mut() {
                touch_consumer(bytes, now);
            }
            match read_frame(map.bytes(), now, &mut self.scratch) {
                ReadResult::Frame { w: fw, h: fh, status } if status == STATUS_LIVE => {
                    self.misses = 0;
                    std::mem::swap(&mut self.scratch, &mut self.good);
                    self.good_size = Some((fw, fh));
                    if self.emit_good(dst, w, h) {
                        return;
                    }
                }
                ReadResult::Frame { .. } => {
                    self.misses = 0;
                    self.good_size = None;
                    card = Card::Unavailable;
                }
                ReadResult::Busy => {
                    if self.emit_good(dst, w, h) {
                        return;
                    }
                }
                ReadResult::Stale | ReadResult::Invalid => {
                    self.misses += 1;
                    self.good_size = None;
                }
            }
        }
        self.fill_card(dst, w, h, card);
    }

    /// Copies (or scales) the last good frame into `dst`. False if there is none.
    fn emit_good(&self, dst: &mut [u8], w: u32, h: u32) -> bool {
        let Some((gw, gh)) = self.good_size else { return false };
        if self.good.len() < nv12_len(gw, gh) {
            return false;
        }
        if (gw, gh) == (w, h) && self.good.len() == dst.len() {
            dst.copy_from_slice(&self.good);
            return true;
        }
        let src = Nv12 { data: &self.good, width: gw, height: gh, stride: gw };
        scale_nv12(&src, Rect { x: 0, y: 0, w: gw, h: gh }, dst, w, h).is_ok()
    }

    fn fill_card(&mut self, dst: &mut [u8], w: u32, h: u32, card: Card) {
        let fresh = !matches!(&self.card, Some((cw, ch, c, _)) if (*cw, *ch, *c) == (w, h, card));
        if fresh {
            let mut picture = vec![0u8; nv12_len(w, h)];
            draw_card(&mut picture, w, h, card);
            self.card = Some((w, h, card, picture));
        }
        if let Some((_, _, _, picture)) = &self.card {
            let n = picture.len().min(dst.len());
            dst[..n].copy_from_slice(&picture[..n]);
        }
    }
}

#[implement(IMFMediaStream2, IKsControl)]
pub(crate) struct Stream {
    queue: IMFMediaEventQueue,
    pub(crate) attrs: IMFAttributes,
    pub(crate) descriptor: IMFStreamDescriptor,
    pub(crate) source: Mutex<Option<IMFMediaSource>>,
    state: Mutex<MF_STREAM_STATE>,
    feed: Mutex<Feed>,
    pacer: Mutex<anyzoom_core::pace::Pacer>,
}

impl Stream {
    pub(crate) fn new(descriptor: IMFStreamDescriptor, attrs: IMFAttributes) -> Result<Stream> {
        Ok(Stream {
            queue: unsafe { MFCreateEventQueue()? },
            attrs,
            descriptor,
            source: Mutex::new(None),
            state: Mutex::new(MF_STREAM_STATE_STOPPED),
            feed: Mutex::new(Feed::default()),
            pacer: Mutex::new(Default::default()),
        })
    }

    pub(crate) fn start(&self) -> Result<()> {
        unsafe { self.queue.QueueEventParamVar(MEStreamStarted.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::from(0i64))? };
        *lock(&self.state) = MF_STREAM_STATE_RUNNING;
        Ok(())
    }

    pub(crate) fn stop(&self) -> Result<()> {
        unsafe { self.queue.QueueEventParamVar(MEStreamStopped.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::default())? };
        *lock(&self.state) = MF_STREAM_STATE_STOPPED;
        Ok(())
    }

    pub(crate) fn shutdown(&self) {
        *lock(&self.source) = None;
        *lock(&self.feed) = Feed::default(); // unmaps the frame file
        unsafe {
            let _ = self.queue.Shutdown();
        }
    }

    /// The frame size the consuming app selected; the default if it is not one we offer.
    fn frame_size(&self) -> (u32, u32) {
        let selected = unsafe {
            self.descriptor
                .GetMediaTypeHandler()
                .and_then(|handler| handler.GetCurrentMediaType())
                .and_then(|media_type| media_type.GetUINT64(&MF_MT_FRAME_SIZE))
        };
        match selected {
            Ok(packed) => {
                let size = ((packed >> 32) as u32, packed as u32);
                if SIZES.contains(&size) { size } else { SIZES[0] }
            }
            Err(_) => SIZES[0],
        }
    }
}

/// A poisoned lock only means another request panicked; the data is still usable.
pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl IMFMediaEventGenerator_Impl for Stream_Impl {
    fn GetEvent(&self, flags: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> Result<IMFMediaEvent> {
        unsafe { self.queue.GetEvent(flags.0) }
    }
    fn BeginGetEvent(&self, callback: Ref<'_, IMFAsyncCallback>, state: Ref<'_, IUnknown>) -> Result<()> {
        unsafe { self.queue.BeginGetEvent(callback.as_ref(), state.as_ref()) }
    }
    fn EndGetEvent(&self, result: Ref<'_, IMFAsyncResult>) -> Result<IMFMediaEvent> {
        unsafe { self.queue.EndGetEvent(result.as_ref()) }
    }
    fn QueueEvent(&self, met: u32, guid: *const GUID, hr: HRESULT, value: *const PROPVARIANT) -> Result<()> {
        unsafe { self.queue.QueueEventParamVar(met, guid, hr, value) }
    }
}

impl IMFMediaStream_Impl for Stream_Impl {
    fn GetMediaSource(&self) -> Result<IMFMediaSource> {
        lock(&self.source).clone().ok_or(MF_E_SHUTDOWN.into())
    }
    fn GetStreamDescriptor(&self) -> Result<IMFStreamDescriptor> {
        Ok(self.descriptor.clone())
    }
    fn RequestSample(&self, token: Ref<'_, IUnknown>) -> Result<()> {
        if lock(&self.source).is_none() {
            return Err(MF_E_SHUTDOWN.into());
        }
        if *lock(&self.state) != MF_STREAM_STATE_RUNNING {
            return Err(MF_E_MEDIA_SOURCE_WRONGSTATE.into());
        }
        // Consumers ask for the next frame the moment they get one; hold each
        // request back so the camera runs at its advertised 30 fps.
        let wait = lock(&self.pacer).wait(unsafe { MFGetSystemTime() } as u64, FRAME_100NS);
        if wait > 0 {
            std::thread::sleep(std::time::Duration::from_nanos(wait * 100));
        }
        let (w, h) = self.frame_size();
        let len = nv12_len(w, h);
        unsafe {
            let buffer = MFCreateMemoryBuffer(len as u32)?;
            let mut data = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None)?;
            let dst = std::slice::from_raw_parts_mut(data, len);
            // A panic must never unwind into the camera service.
            let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| lock(&self.feed).fill(dst, w, h)));
            if drawn.is_err() {
                anyzoom_core::fill_nv12(dst, w, h, 16, 128, 128);
            }
            buffer.Unlock()?;
            buffer.SetCurrentLength(len as u32)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(MFGetSystemTime())?;
            sample.SetSampleDuration(FRAME_100NS as i64)?;
            if let Some(token) = token.as_ref() {
                sample.SetUnknown(&MFSampleExtension_Token, token)?;
            }
            self.queue.QueueEventParamUnk(MEMediaSample.0 as u32, &GUID::zeroed(), S_OK, &sample)
        }
    }
}

impl IMFMediaStream2_Impl for Stream_Impl {
    fn SetStreamState(&self, value: MF_STREAM_STATE) -> Result<()> {
        if *lock(&self.state) == value {
            return Ok(());
        }
        if value == MF_STREAM_STATE_RUNNING {
            self.start()
        } else if value == MF_STREAM_STATE_STOPPED {
            self.stop()
        } else {
            Ok(())
        }
    }
    fn GetStreamState(&self) -> Result<MF_STREAM_STATE> {
        Ok(*lock(&self.state))
    }
}

/// The camera has no controls of its own: every property request is "not supported".
pub(crate) fn ks_not_found() -> Result<()> {
    Err(HRESULT::from_win32(ERROR_SET_NOT_FOUND.0).into())
}

impl IKsControl_Impl for Stream_Impl {
    fn KsProperty(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> {
        ks_not_found()
    }
    fn KsMethod(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> {
        ks_not_found()
    }
    fn KsEvent(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> {
        ks_not_found()
    }
}

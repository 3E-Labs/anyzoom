//! Finds webcams and reads video from one, through Media Foundation.
//!
//! This crate only ever *reads video*. It never sends a control or property
//! request to a camera: vendor-specific controls can sit next to firmware
//! functions, and a wrong write could damage a device.
#![cfg(windows)]

use anyzoom_core::mode::{best_mode, Format, Mode};
use anyzoom_core::Nv12;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, Once};
use std::time::Duration;
use windows::core::{implement, Ref, Result as WinResult, GUID, HRESULT, PWSTR};
use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_MULTITHREADED};

/// Name fragment of our own virtual camera, which must never be offered as an input.
const OWN_CAMERA: &str = "anyzoom Camera";
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CameraInfo {
    /// The device path (symbolic link): stable for a given camera and USB port.
    pub id: String,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CaptureError {
    /// No camera with that id is connected.
    NotFound,
    /// The camera exists but something else is using it, or Windows privacy
    /// settings block camera access.
    Busy,
    /// The camera was unplugged or stopped delivering video.
    Disconnected,
    Other(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::NotFound => write!(f, "camera not found"),
            CaptureError::Busy => write!(f, "camera is in use by another app or blocked by privacy settings"),
            CaptureError::Disconnected => write!(f, "camera disconnected"),
            CaptureError::Other(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for CaptureError {}

fn classify(e: &windows::core::Error) -> CaptureError {
    const SHARING_VIOLATION: HRESULT = HRESULT(0x80070020u32 as i32);
    const DEVICE_NOT_CONNECTED: HRESULT = HRESULT(0x8007048Fu32 as i32);
    const NO_SUCH_DEVICE: HRESULT = HRESULT(0x800701B1u32 as i32);
    const NOT_FOUND: HRESULT = HRESULT(0x80070490u32 as i32);
    const FILE_NOT_FOUND: HRESULT = HRESULT(0x80070002u32 as i32);
    let code = e.code();
    if code == MF_E_HW_MFT_FAILED_START_STREAMING || code == MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED || code == E_ACCESSDENIED || code == SHARING_VIOLATION {
        CaptureError::Busy
    } else if code == MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED || code == DEVICE_NOT_CONNECTED || code == NO_SUCH_DEVICE || code == MF_E_SHUTDOWN {
        CaptureError::Disconnected
    } else if code == NOT_FOUND || code == FILE_NOT_FOUND {
        CaptureError::NotFound
    } else {
        CaptureError::Other(format!("{} ({:#010X})", e.message(), code.0 as u32))
    }
}

/// Prepares the calling thread and starts Media Foundation (once per process).
fn init() {
    static MF: Once = Once::new();
    unsafe {
        // Already initialised, in whatever mode, is fine.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MF.call_once(|| {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });
    }
}

unsafe fn device_attributes(id: Option<&str>) -> WinResult<IMFAttributes> {
    let mut attrs = None;
    MFCreateAttributes(&mut attrs, 2)?;
    let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
    attrs.SetGUID(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID)?;
    if let Some(id) = id {
        attrs.SetString(&MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, &windows::core::HSTRING::from(id))?;
    }
    Ok(attrs)
}

unsafe fn string_attr(activate: &IMFActivate, key: &GUID) -> String {
    let mut text = PWSTR::null();
    let mut len = 0u32;
    if activate.GetAllocatedString(key, &mut text, &mut len).is_err() || text.is_null() {
        return String::new();
    }
    let s = text.to_string().unwrap_or_default();
    CoTaskMemFree(Some(text.0.cast()));
    s
}

/// Physical cameras only: our own virtual camera is left out.
pub fn list_cameras() -> Result<Vec<CameraInfo>, CaptureError> {
    init();
    unsafe {
        let attrs = device_attributes(None).map_err(|e| classify(&e))?;
        let mut devices: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        MFEnumDeviceSources(&attrs, &mut devices, &mut count).map_err(|e| classify(&e))?;
        let mut cameras = Vec::new();
        if !devices.is_null() {
            for i in 0..count as usize {
                // take ownership so each activation object is released
                if let Some(activate) = std::ptr::read(devices.add(i)) {
                    let name = string_attr(&activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME);
                    let id = string_attr(&activate, &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK);
                    if !id.is_empty() && !name.contains(OWN_CAMERA) {
                        cameras.push(CameraInfo { id, name });
                    }
                }
            }
            CoTaskMemFree(Some(devices.cast()));
        }
        Ok(cameras)
    }
}

type Delivery = (HRESULT, u32, Option<IMFSample>);

/// Receives frames from Media Foundation's worker threads.
#[implement(IMFSourceReaderCallback)]
struct Callback {
    tx: Mutex<Sender<Delivery>>,
}

impl IMFSourceReaderCallback_Impl for Callback_Impl {
    fn OnReadSample(&self, hr: HRESULT, _stream: u32, flags: u32, _timestamp: i64, sample: Ref<'_, IMFSample>) -> WinResult<()> {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send((hr, flags, sample.as_ref().cloned()));
        }
        Ok(())
    }
    fn OnFlush(&self, _stream: u32) -> WinResult<()> {
        Ok(())
    }
    fn OnEvent(&self, _stream: u32, _event: Ref<'_, IMFMediaEvent>) -> WinResult<()> {
        Ok(())
    }
}

/// Diagnostic: one line per video mode the camera offers, plus the type the
/// reader ends up delivering. Does not start the camera.
pub fn describe_modes(id: &str) -> Result<Vec<String>, CaptureError> {
    init();
    unsafe {
        (|| -> WinResult<Vec<String>> {
            let source = MFCreateDeviceSource(&device_attributes(Some(id))?)?;
            let reader = MFCreateSourceReaderFromMediaSource(&source, None)?;
            let mut out = Vec::new();
            let mut index = 0u32;
            while let Ok(t) = reader.GetNativeMediaType(VIDEO, index) {
                out.push(format!("{index}: {}", describe_type(&t)));
                index += 1;
            }
            let _ = source.Shutdown();
            Ok(out)
        })()
        .map_err(|e| classify(&e))
    }
}

unsafe fn describe_type(t: &IMFMediaType) -> String {
    let pair = |key| t.GetUINT64(key).map(|v| format!("{}x{}", v >> 32, v as u32)).unwrap_or_else(|_| "-".into());
    let blob = |key| {
        let mut buf = [0u8; 16];
        match t.GetBlob(key, &mut buf, None) {
            Ok(()) => format!("{:?}", buf.chunks(4).map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect::<Vec<_>>()),
            Err(_) => "-".into(),
        }
    };
    format!(
        "sub={:08X} size={} rate={} par={} geo={} min={} pan={} panscan_on={:?} range={:?}",
        t.GetGUID(&MF_MT_SUBTYPE).map(|g| g.data1).unwrap_or(0),
        pair(&MF_MT_FRAME_SIZE),
        pair(&MF_MT_FRAME_RATE),
        pair(&MF_MT_PIXEL_ASPECT_RATIO),
        blob(&MF_MT_GEOMETRIC_APERTURE),
        blob(&MF_MT_MINIMUM_DISPLAY_APERTURE),
        blob(&MF_MT_PAN_SCAN_APERTURE),
        t.GetUINT32(&MF_MT_PAN_SCAN_ENABLED).ok(),
        t.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE).ok(),
    )
}

/// An open camera delivering NV12 frames. Must be used on the thread that opened it.
pub struct Capture {
    source: IMFMediaSource,
    reader: IMFSourceReader,
    rx: Receiver<Delivery>,
    /// A frame has been requested and not yet delivered.
    pending: bool,
    width: u32,
    height: u32,
    stride: u32,
}

impl Capture {
    /// Opens the camera with the given id in the mode closest to 1920x1080 at
    /// 30 fps. Frames are delivered as NV12 whatever the camera's own format.
    pub fn open(id: &str) -> Result<Capture, CaptureError> {
        init();
        if !list_cameras()?.iter().any(|c| c.id == id) {
            return Err(CaptureError::NotFound);
        }
        unsafe { Self::open_inner(id).map_err(|e| classify(&e)) }
    }

    unsafe fn open_inner(id: &str) -> WinResult<Capture> {
        let source = MFCreateDeviceSource(&device_attributes(Some(id))?)?;
        let opened = Self::make_reader(&source);
        if opened.is_err() {
            let _ = source.Shutdown();
        }
        let (reader, rx, (width, height, stride)) = opened?;
        Ok(Capture { source, reader, rx, pending: false, width, height, stride })
    }

    unsafe fn make_reader(source: &IMFMediaSource) -> WinResult<(IMFSourceReader, Receiver<Delivery>, (u32, u32, u32))> {
        let (tx, rx) = channel();
        let callback: IMFSourceReaderCallback = Callback { tx: Mutex::new(tx) }.into();
        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 3)?;
        let attrs = attrs.ok_or_else(windows::core::Error::empty)?;
        // let Media Foundation decode MJPEG and convert YUY2 so we always get NV12
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
        attrs.SetUnknown(&MF_SOURCE_READER_ASYNC_CALLBACK, &callback)?;
        let reader = MFCreateSourceReaderFromMediaSource(source, &attrs)?;

        let mut modes = Vec::new();
        let mut index = 0u32;
        while let Ok(native) = reader.GetNativeMediaType(VIDEO, index) {
            let size = native.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0);
            let rate = native.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0);
            let (num, den) = ((rate >> 32) as u32, rate as u32);
            let format = match native.GetGUID(&MF_MT_SUBTYPE) {
                Ok(g) if g == MFVideoFormat_MJPG => Format::Mjpeg,
                Ok(g) if g == MFVideoFormat_YUY2 => Format::Yuy2,
                Ok(g) if g == MFVideoFormat_NV12 => Format::Nv12,
                _ => Format::Other,
            };
            modes.push(Mode { index, width: (size >> 32) as u32, height: size as u32, fps: if den == 0 { 0.0 } else { num as f64 / den as f64 }, format });
            index += 1;
        }
        let best = best_mode(modes).ok_or_else(|| windows::core::Error::new(MF_E_INVALIDMEDIATYPE, "camera offers no video modes"))?;
        let native = reader.GetNativeMediaType(VIDEO, best.index)?;
        reader.SetCurrentMediaType(VIDEO, None, &native)?;

        let wanted = MFCreateMediaType()?;
        wanted.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        wanted.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        wanted.SetUINT64(&MF_MT_FRAME_SIZE, ((best.width as u64) << 32) | best.height as u64)?;
        // video range (16-235), which is what consumers of a webcam expect
        wanted.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
        reader.SetCurrentMediaType(VIDEO, None, &wanted)?;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(VIDEO, true)?;
        if std::env::var_os("ANYZOOM_DEBUG").is_some() {
            eprintln!("native {}: {}", best.index, describe_type(&native));
            eprintln!("output: {}", describe_type(&reader.GetCurrentMediaType(VIDEO)?));
        }
        let layout = Self::layout(&reader)?;
        Ok((reader, rx, layout))
    }

    /// Width, height and bytes per row of the frames the reader delivers.
    unsafe fn layout(reader: &IMFSourceReader) -> WinResult<(u32, u32, u32)> {
        let current = reader.GetCurrentMediaType(VIDEO)?;
        let size = current.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let (width, height) = ((size >> 32) as u32, size as u32);
        let stride = current.GetUINT32(&MF_MT_DEFAULT_STRIDE).ok().filter(|s| *s >= width && *s < width * 2).unwrap_or(width);
        Ok((width, height, stride))
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Waits up to `timeout_ms` for the next frame and calls `f` with it while
    /// its buffer is locked. `Ok(false)` means no frame arrived in time.
    pub fn read(&mut self, timeout_ms: u32, f: impl FnOnce(&Nv12)) -> Result<bool, CaptureError> {
        unsafe {
            if !self.pending {
                self.reader.ReadSample(VIDEO, 0, None, None, None, None).map_err(|e| classify(&e))?;
                self.pending = true;
            }
            let (hr, flags, sample) = match self.rx.recv_timeout(Duration::from_millis(timeout_ms as u64)) {
                Ok(delivery) => delivery,
                Err(RecvTimeoutError::Timeout) => return Ok(false),
                Err(RecvTimeoutError::Disconnected) => return Err(CaptureError::Disconnected),
            };
            self.pending = false;
            if let Err(e) = hr.ok() {
                return Err(classify(&e));
            }
            let has = |flag: MF_SOURCE_READER_FLAG| flags & flag.0 as u32 != 0;
            if has(MF_SOURCE_READERF_ERROR) || has(MF_SOURCE_READERF_ENDOFSTREAM) {
                return Err(CaptureError::Disconnected);
            }
            if has(MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED) || has(MF_SOURCE_READERF_NATIVEMEDIATYPECHANGED) {
                (self.width, self.height, self.stride) = Self::layout(&self.reader).map_err(|e| classify(&e))?;
            }
            let Some(sample) = sample else { return Ok(false) }; // a gap in the stream, not a frame

            let buffer = sample.ConvertToContiguousBuffer().map_err(|e| classify(&e))?;
            let mut data = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut data, None, Some(&mut len)).map_err(|e| classify(&e))?;
            let (stride, rows) = (self.stride as usize, self.height as usize);
            // Y rows then UV rows, all `stride` wide; the last row may stop at `width`.
            let needed = stride * (rows + rows / 2 - 1) + self.width as usize;
            let usable = !data.is_null() && self.width >= 2 && self.height >= 2 && self.width % 2 == 0 && self.height % 2 == 0 && len as usize >= needed;
            if usable {
                let frame = Nv12 { data: std::slice::from_raw_parts(data, len as usize), width: self.width, height: self.height, stride: self.stride };
                f(&frame);
            }
            let _ = buffer.Unlock();
            Ok(usable)
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // Releases the camera (and its light) immediately rather than whenever
        // the last reference happens to go away.
        unsafe {
            let _ = self.reader.Flush(MF_SOURCE_READER_ALL_STREAMS.0 as u32);
            let _ = self.source.Shutdown();
        }
    }
}

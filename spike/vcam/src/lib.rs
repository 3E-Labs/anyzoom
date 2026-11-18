#![allow(non_snake_case, clippy::missing_safety_doc)]
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::KernelStreaming::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::*;
use windows::Win32::System::Memory::*;

pub const CLSID_SPIKE: GUID = GUID::from_u128(0x3e3e3e3e_a11c_4a00_9d00_616e797a6f6f);
const W: u32 = 1280;
const H: u32 = 720;
const FRAME_PATH: PCWSTR = w!("C:\\ProgramData\\anyzoom-spike\\frame.bin");
const HDR: usize = 16;

fn log(s: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("C:\\ProgramData\\anyzoom-spike\\vcam.log") {
        let _ = writeln!(f, "[{}] {}", std::process::id(), s);
    }
}

// ───────── shared frame (file-backed mapping written by the host app) ─────────
struct Shared { ptr: *const u8, len: usize }
unsafe impl Send for Shared {}
fn open_shared() -> Option<Shared> {
    unsafe {
        let f = CreateFileW(FRAME_PATH, FILE_GENERIC_READ.0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None).ok()?;
        let m = CreateFileMappingW(f, None, PAGE_READONLY, 0, 0, None);
        let _ = CloseHandle(f);
        let m = m.ok()?;
        let v = MapViewOfFile(m, FILE_MAP_READ, 0, 0, 0);
        let _ = CloseHandle(m);
        if v.Value.is_null() { return None; }
        Some(Shared { ptr: v.Value as *const u8, len: HDR + (W * H * 3 / 2) as usize })
    }
}

// ───────── stream ─────────
#[implement(IMFMediaStream2, IKsControl)]
struct Stream {
    queue: IMFMediaEventQueue,
    attrs: IMFAttributes,
    sd: IMFStreamDescriptor,
    source: Mutex<Option<IMFMediaSource>>,
    state: Mutex<MF_STREAM_STATE>,
    n: Mutex<u32>,
    shared: Mutex<Option<Shared>>,
}

impl Stream {
    fn start(&self) -> Result<()> {
        unsafe { self.queue.QueueEventParamVar(MEStreamStarted.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::from(0i64))?; }
        *self.state.lock().unwrap() = MF_STREAM_STATE_RUNNING;
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        unsafe { self.queue.QueueEventParamVar(MEStreamStopped.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::default())?; }
        *self.state.lock().unwrap() = MF_STREAM_STATE_STOPPED;
        Ok(())
    }
    fn fill(&self, dst: &mut [u8]) {
        let mut n = self.n.lock().unwrap();
        *n += 1;
        let mut sh = self.shared.lock().unwrap();
        if sh.is_none() && *n % 30 == 1 { *sh = open_shared(); if sh.is_some() { log("shared frame mapped"); } }
        if let Some(s) = sh.as_ref() {
            let src = unsafe { std::slice::from_raw_parts(s.ptr, s.len) };
            if &src[0..4] == b"AZ01" { dst.copy_from_slice(&src[HDR..]); return; }
        }
        // fallback pattern: vertical grey bars + moving white column
        let (w, h) = (W as usize, H as usize);
        let x0 = (*n as usize * 8) % w;
        for y in 0..h { for x in 0..w {
            dst[y * w + x] = if x >= x0 && x < x0 + 16 { 235 } else { 40 + ((x / 160) * 24) as u8 };
        } }
        for b in &mut dst[w * h..] { *b = 128; }
    }
}

impl IMFMediaEventGenerator_Impl for Stream_Impl {
    fn GetEvent(&self, f: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> Result<IMFMediaEvent> { unsafe { self.queue.GetEvent(f.0) } }
    fn BeginGetEvent(&self, cb: Ref<'_, IMFAsyncCallback>, st: Ref<'_, IUnknown>) -> Result<()> { unsafe { self.queue.BeginGetEvent(cb.as_ref(), st.as_ref()) } }
    fn EndGetEvent(&self, r: Ref<'_, IMFAsyncResult>) -> Result<IMFMediaEvent> { unsafe { self.queue.EndGetEvent(r.as_ref()) } }
    fn QueueEvent(&self, met: u32, g: *const GUID, hr: HRESULT, v: *const PROPVARIANT) -> Result<()> { unsafe { self.queue.QueueEventParamVar(met, g, hr, v) } }
}
impl IMFMediaStream_Impl for Stream_Impl {
    fn GetMediaSource(&self) -> Result<IMFMediaSource> { self.source.lock().unwrap().clone().ok_or(MF_E_SHUTDOWN.into()) }
    fn GetStreamDescriptor(&self) -> Result<IMFStreamDescriptor> { Ok(self.sd.clone()) }
    fn RequestSample(&self, token: Ref<'_, IUnknown>) -> Result<()> {
        unsafe {
            let len = W * H * 3 / 2;
            let buf = MFCreateMemoryBuffer(len)?;
            let mut p = std::ptr::null_mut();
            buf.Lock(&mut p, None, None)?;
            self.fill(std::slice::from_raw_parts_mut(p, len as usize));
            buf.Unlock()?;
            buf.SetCurrentLength(len)?;
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buf)?;
            sample.SetSampleTime(MFGetSystemTime())?;
            sample.SetSampleDuration(333_333)?;
            if let Some(t) = token.as_ref() { sample.SetUnknown(&MFSampleExtension_Token, t)?; }
            self.queue.QueueEventParamUnk(MEMediaSample.0 as u32, &GUID::zeroed(), S_OK, &sample)
        }
    }
}
impl IMFMediaStream2_Impl for Stream_Impl {
    fn SetStreamState(&self, v: MF_STREAM_STATE) -> Result<()> {
        log(&format!("SetStreamState {}", v.0));
        if *self.state.lock().unwrap() == v { return Ok(()); }
        if v == MF_STREAM_STATE_RUNNING { self.start() } else if v == MF_STREAM_STATE_STOPPED { self.stop() } else { Ok(()) }
    }
    fn GetStreamState(&self) -> Result<MF_STREAM_STATE> { Ok(*self.state.lock().unwrap()) }
}
fn ks_not_found() -> Result<()> { Err(HRESULT::from_win32(ERROR_SET_NOT_FOUND.0).into()) }
impl IKsControl_Impl for Stream_Impl {
    fn KsProperty(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> { ks_not_found() }
    fn KsMethod(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> { ks_not_found() }
    fn KsEvent(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> { ks_not_found() }
}

// ───────── source ─────────
#[implement(IMFMediaSourceEx, IMFGetService, IKsControl)]
struct Source {
    queue: IMFMediaEventQueue,
    attrs: IMFAttributes,
    pd: IMFPresentationDescriptor,
    stream: IMFMediaStream2,
    started_once: Mutex<bool>,
}

fn make_source(activate_attrs: &IMFAttributes) -> Result<IMFMediaSourceEx> {
    unsafe {
        let mt = MFCreateMediaType()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        mt.SetUINT64(&MF_MT_FRAME_SIZE, ((W as u64) << 32) | H as u64)?;
        mt.SetUINT32(&MF_MT_DEFAULT_STRIDE, W)?;
        mt.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        mt.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        mt.SetUINT64(&MF_MT_FRAME_RATE, (30u64 << 32) | 1)?;
        mt.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        mt.SetUINT32(&MF_MT_AVG_BITRATE, W * H * 12 * 30)?;
        let sd = MFCreateStreamDescriptor(0, &[Some(mt.clone())])?;
        sd.GetMediaTypeHandler()?.SetCurrentMediaType(&mt)?;
        let pd = MFCreatePresentationDescriptor(Some(&[Some(sd.clone())]))?;
        pd.SelectStream(0)?;

        let mut sattrs = None;
        MFCreateAttributes(&mut sattrs, 4)?;
        let sattrs = sattrs.unwrap();
        sattrs.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
        sattrs.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0)?;
        sattrs.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
        sattrs.SetUINT32(&MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MFFrameSourceTypes_Color.0 as u32)?;

        let stream: IMFMediaStream2 = Stream {
            queue: MFCreateEventQueue()?, attrs: sattrs, sd,
            source: Mutex::new(None), state: Mutex::new(MF_STREAM_STATE_STOPPED), n: Mutex::new(0), shared: Mutex::new(None),
        }.into();

        let mut attrs = None;
        MFCreateAttributes(&mut attrs, 8)?;
        let attrs = attrs.unwrap();
        activate_attrs.CopyAllItems(&attrs)?;

        let src: IMFMediaSourceEx = Source { queue: MFCreateEventQueue()?, attrs, pd, stream: stream.clone(), started_once: Mutex::new(false) }.into();
        let inner: &Stream = stream.as_impl();
        *inner.source.lock().unwrap() = Some(src.cast()?);
        Ok(src)
    }
}

impl IMFMediaEventGenerator_Impl for Source_Impl {
    fn GetEvent(&self, f: MEDIA_EVENT_GENERATOR_GET_EVENT_FLAGS) -> Result<IMFMediaEvent> { unsafe { self.queue.GetEvent(f.0) } }
    fn BeginGetEvent(&self, cb: Ref<'_, IMFAsyncCallback>, st: Ref<'_, IUnknown>) -> Result<()> { unsafe { self.queue.BeginGetEvent(cb.as_ref(), st.as_ref()) } }
    fn EndGetEvent(&self, r: Ref<'_, IMFAsyncResult>) -> Result<IMFMediaEvent> { unsafe { self.queue.EndGetEvent(r.as_ref()) } }
    fn QueueEvent(&self, met: u32, g: *const GUID, hr: HRESULT, v: *const PROPVARIANT) -> Result<()> { unsafe { self.queue.QueueEventParamVar(met, g, hr, v) } }
}
impl IMFMediaSource_Impl for Source_Impl {
    fn GetCharacteristics(&self) -> Result<u32> { Ok(MFMEDIASOURCE_IS_LIVE.0 as u32) }
    fn CreatePresentationDescriptor(&self) -> Result<IMFPresentationDescriptor> { unsafe { self.pd.Clone() } }
    fn Start(&self, _pd: Ref<'_, IMFPresentationDescriptor>, _fmt: *const GUID, _pos: *const PROPVARIANT) -> Result<()> {
        log("Source::Start");
        unsafe {
            let mut once = self.started_once.lock().unwrap();
            let met = if *once { MEUpdatedStream } else { MENewStream };
            *once = true;
            self.queue.QueueEventParamUnk(met.0 as u32, &GUID::zeroed(), S_OK, &self.stream)?;
            self.queue.QueueEventParamVar(MESourceStarted.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::from(0i64))?;
            let s: &Stream = self.stream.as_impl();
            s.start()
        }
    }
    fn Stop(&self) -> Result<()> {
        log("Source::Stop");
        unsafe {
            let s: &Stream = self.stream.as_impl();
            s.stop()?;
            self.queue.QueueEventParamVar(MESourceStopped.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::from(0i64))
        }
    }
    fn Pause(&self) -> Result<()> { Err(MF_E_INVALID_STATE_TRANSITION.into()) }
    fn Shutdown(&self) -> Result<()> {
        log("Source::Shutdown");
        unsafe {
            let s: &Stream = self.stream.as_impl();
            *s.source.lock().unwrap() = None;
            let _ = s.queue.Shutdown();
            let _ = self.queue.Shutdown();
        }
        Ok(())
    }
}
impl IMFMediaSourceEx_Impl for Source_Impl {
    fn GetSourceAttributes(&self) -> Result<IMFAttributes> { Ok(self.attrs.clone()) }
    fn GetStreamAttributes(&self, id: u32) -> Result<IMFAttributes> {
        if id != 0 { return Err(E_INVALIDARG.into()); }
        let s: &Stream = unsafe { self.stream.as_impl() };
        Ok(s.attrs.clone())
    }
    fn SetD3DManager(&self, _: Ref<'_, IUnknown>) -> Result<()> { Ok(()) }
}
impl IMFGetService_Impl for Source_Impl {
    fn GetService(&self, _: *const GUID, _: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        if !ppv.is_null() { unsafe { *ppv = std::ptr::null_mut(); } }
        Err(MF_E_UNSUPPORTED_SERVICE.into())
    }
}
impl IKsControl_Impl for Source_Impl {
    fn KsProperty(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> { ks_not_found() }
    fn KsMethod(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> { ks_not_found() }
    fn KsEvent(&self, _: *const KSIDENTIFIER, _: u32, _: *mut c_void, _: u32, _: *mut u32) -> Result<()> { ks_not_found() }
}

// ───────── activator ─────────
#[implement(IMFActivate)]
struct Activator { a: IMFAttributes, src: Mutex<Option<IMFMediaSourceEx>> }

impl IMFActivate_Impl for Activator_Impl {
    fn ActivateObject(&self, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        log("ActivateObject");
        unsafe {
            *ppv = std::ptr::null_mut();
            let mut g = self.src.lock().unwrap();
            if g.is_none() { *g = Some(make_source(&self.a).map_err(|e| { log(&format!("make_source failed {e:?}")); e })?); }
            g.as_ref().unwrap().query(riid, ppv).ok()
        }
    }
    fn ShutdownObject(&self) -> Result<()> { Ok(()) }
    fn DetachObject(&self) -> Result<()> { *self.src.lock().unwrap() = None; Ok(()) }
}
impl IMFAttributes_Impl for Activator_Impl {
    fn GetItem(&self, k: *const GUID, v: *mut PROPVARIANT) -> Result<()> { unsafe { self.a.GetItem(k, Some(v)) } }
    fn GetItemType(&self, k: *const GUID) -> Result<MF_ATTRIBUTE_TYPE> { unsafe { self.a.GetItemType(k) } }
    fn CompareItem(&self, k: *const GUID, v: *const PROPVARIANT) -> Result<BOOL> { unsafe { self.a.CompareItem(k, v) } }
    fn Compare(&self, t: Ref<'_, IMFAttributes>, m: MF_ATTRIBUTES_MATCH_TYPE) -> Result<BOOL> { unsafe { self.a.Compare(t.as_ref(), m) } }
    fn GetUINT32(&self, k: *const GUID) -> Result<u32> { unsafe { self.a.GetUINT32(k) } }
    fn GetUINT64(&self, k: *const GUID) -> Result<u64> { unsafe { self.a.GetUINT64(k) } }
    fn GetDouble(&self, k: *const GUID) -> Result<f64> { unsafe { self.a.GetDouble(k) } }
    fn GetGUID(&self, k: *const GUID) -> Result<GUID> { unsafe { self.a.GetGUID(k) } }
    fn GetStringLength(&self, k: *const GUID) -> Result<u32> { unsafe { self.a.GetStringLength(k) } }
    fn GetString(&self, k: *const GUID, s: PWSTR, n: u32, l: *mut u32) -> Result<()> { unsafe { self.a.GetString(k, std::slice::from_raw_parts_mut(s.0, n as usize), Some(l)) } }
    fn GetAllocatedString(&self, k: *const GUID, s: *mut PWSTR, l: *mut u32) -> Result<()> { unsafe { self.a.GetAllocatedString(k, s, l) } }
    fn GetBlobSize(&self, k: *const GUID) -> Result<u32> { unsafe { self.a.GetBlobSize(k) } }
    fn GetBlob(&self, k: *const GUID, b: *mut u8, n: u32, l: *mut u32) -> Result<()> { unsafe { self.a.GetBlob(k, std::slice::from_raw_parts_mut(b, n as usize), Some(l)) } }
    fn GetAllocatedBlob(&self, k: *const GUID, b: *mut *mut u8, l: *mut u32) -> Result<()> { unsafe { self.a.GetAllocatedBlob(k, b, l) } }
    fn GetUnknown(&self, k: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        unsafe { (Interface::vtable(&self.a).GetUnknown)(Interface::as_raw(&self.a), k, riid, ppv).ok() }
    }
    fn SetItem(&self, k: *const GUID, v: *const PROPVARIANT) -> Result<()> { unsafe { self.a.SetItem(k, v) } }
    fn DeleteItem(&self, k: *const GUID) -> Result<()> { unsafe { self.a.DeleteItem(k) } }
    fn DeleteAllItems(&self) -> Result<()> { unsafe { self.a.DeleteAllItems() } }
    fn SetUINT32(&self, k: *const GUID, v: u32) -> Result<()> { unsafe { self.a.SetUINT32(k, v) } }
    fn SetUINT64(&self, k: *const GUID, v: u64) -> Result<()> { unsafe { self.a.SetUINT64(k, v) } }
    fn SetDouble(&self, k: *const GUID, v: f64) -> Result<()> { unsafe { self.a.SetDouble(k, v) } }
    fn SetGUID(&self, k: *const GUID, v: *const GUID) -> Result<()> { unsafe { self.a.SetGUID(k, v) } }
    fn SetString(&self, k: *const GUID, v: &PCWSTR) -> Result<()> { unsafe { self.a.SetString(k, *v) } }
    fn SetBlob(&self, k: *const GUID, b: *const u8, n: u32) -> Result<()> { unsafe { self.a.SetBlob(k, std::slice::from_raw_parts(b, n as usize)) } }
    fn SetUnknown(&self, k: *const GUID, u: Ref<'_, IUnknown>) -> Result<()> { unsafe { self.a.SetUnknown(k, u.as_ref()) } }
    fn LockStore(&self) -> Result<()> { unsafe { self.a.LockStore() } }
    fn UnlockStore(&self) -> Result<()> { unsafe { self.a.UnlockStore() } }
    fn GetCount(&self) -> Result<u32> { unsafe { self.a.GetCount() } }
    fn GetItemByIndex(&self, i: u32, k: *mut GUID, v: *mut PROPVARIANT) -> Result<()> { unsafe { self.a.GetItemByIndex(i, k, Some(v)) } }
    fn CopyAllItems(&self, d: Ref<'_, IMFAttributes>) -> Result<()> { unsafe { self.a.CopyAllItems(d.as_ref()) } }
}

// ───────── class factory + exports ─────────
#[implement(IClassFactory)]
struct Factory;
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<'_, IUnknown>, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        log("CreateInstance");
        unsafe {
            *ppv = std::ptr::null_mut();
            if outer.is_some() { return Err(CLASS_E_NOAGGREGATION.into()); }
            let mut a = None;
            MFCreateAttributes(&mut a, 4)?;
            let act: IMFActivate = Activator { a: a.unwrap(), src: Mutex::new(None) }.into();
            act.query(riid, ppv).ok()
        }
    }
    fn LockServer(&self, _: BOOL) -> Result<()> { Ok(()) }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    log("DllGetClassObject");
    if ppv.is_null() { return E_POINTER; }
    *ppv = std::ptr::null_mut();
    if *rclsid != CLSID_SPIKE { return CLASS_E_CLASSNOTAVAILABLE; }
    let f: IClassFactory = Factory.into();
    f.query(riid, ppv)
}
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT { S_FALSE }

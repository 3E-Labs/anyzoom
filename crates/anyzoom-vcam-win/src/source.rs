//! The media source the camera service talks to: one video stream.

use crate::stream::{ks_not_found, lock, Stream, SIZES};
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::KernelStreaming::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;

#[implement(IMFMediaSourceEx, IMFGetService, IKsControl)]
pub(crate) struct Source {
    queue: IMFMediaEventQueue,
    attrs: IMFAttributes,
    presentation: IMFPresentationDescriptor,
    stream: IMFMediaStream2,
    started_once: Mutex<bool>,
}

fn nv12_type(w: u32, h: u32) -> Result<IMFMediaType> {
    unsafe {
        let t = MFCreateMediaType()?;
        t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        t.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64)?;
        t.SetUINT32(&MF_MT_DEFAULT_STRIDE, w)?;
        t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        t.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
        t.SetUINT64(&MF_MT_FRAME_RATE, (30u64 << 32) | 1)?;
        t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        t.SetUINT32(&MF_MT_AVG_BITRATE, w * h * 12 * 30)?;
        t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
        Ok(t)
    }
}

fn new_attributes(room: u32) -> Result<IMFAttributes> {
    let mut attrs = None;
    unsafe { MFCreateAttributes(&mut attrs, room)? };
    attrs.ok_or(E_OUTOFMEMORY.into())
}

pub(crate) fn make_source(activate_attrs: &IMFAttributes) -> Result<IMFMediaSourceEx> {
    unsafe {
        let mut types = Vec::new();
        for (w, h) in SIZES {
            types.push(Some(nv12_type(w, h)?));
        }
        let descriptor = MFCreateStreamDescriptor(0, &types)?;
        descriptor.GetMediaTypeHandler()?.SetCurrentMediaType(types[0].as_ref())?;
        let presentation = MFCreatePresentationDescriptor(Some(&[Some(descriptor.clone())]))?;
        presentation.SelectStream(0)?;

        let stream_attrs = new_attributes(4)?;
        stream_attrs.SetGUID(&MF_DEVICESTREAM_STREAM_CATEGORY, &PINNAME_VIDEO_CAPTURE)?;
        stream_attrs.SetUINT32(&MF_DEVICESTREAM_STREAM_ID, 0)?;
        stream_attrs.SetUINT32(&MF_DEVICESTREAM_FRAMESERVER_SHARED, 1)?;
        stream_attrs.SetUINT32(&MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES, MFFrameSourceTypes_Color.0 as u32)?;
        let stream: IMFMediaStream2 = Stream::new(descriptor, stream_attrs)?.into();

        let attrs = new_attributes(8)?;
        activate_attrs.CopyAllItems(&attrs)?;

        let source: IMFMediaSourceEx =
            Source { queue: MFCreateEventQueue()?, attrs, presentation, stream: stream.clone(), started_once: Mutex::new(false) }.into();
        let inner: &Stream = stream.as_impl();
        *lock(&inner.source) = Some(source.cast()?);
        Ok(source)
    }
}

impl Source {
    fn stream(&self) -> &Stream {
        unsafe { self.stream.as_impl() }
    }

    /// Adopts the frame size the caller selected in the descriptor it starts us with.
    unsafe fn adopt_selected_type(&self, presentation: Option<&IMFPresentationDescriptor>) -> Result<()> {
        let Some(presentation) = presentation else { return Ok(()) };
        let mut selected = BOOL::default();
        let mut descriptor = None;
        presentation.GetStreamDescriptorByIndex(0, &mut selected, &mut descriptor)?;
        let Some(descriptor) = descriptor else { return Ok(()) };
        if let Ok(media_type) = descriptor.GetMediaTypeHandler()?.GetCurrentMediaType() {
            self.stream().descriptor.GetMediaTypeHandler()?.SetCurrentMediaType(&media_type)?;
        }
        Ok(())
    }
}

impl IMFMediaEventGenerator_Impl for Source_Impl {
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

impl IMFMediaSource_Impl for Source_Impl {
    fn GetCharacteristics(&self) -> Result<u32> {
        Ok(MFMEDIASOURCE_IS_LIVE.0 as u32)
    }
    fn CreatePresentationDescriptor(&self) -> Result<IMFPresentationDescriptor> {
        unsafe { self.presentation.Clone() }
    }
    fn Start(&self, presentation: Ref<'_, IMFPresentationDescriptor>, _format: *const GUID, _position: *const PROPVARIANT) -> Result<()> {
        unsafe {
            self.adopt_selected_type(presentation.as_ref())?;
            let mut once = lock(&self.started_once);
            let event = if *once { MEUpdatedStream } else { MENewStream };
            *once = true;
            self.queue.QueueEventParamUnk(event.0 as u32, &GUID::zeroed(), S_OK, &self.stream)?;
            self.queue.QueueEventParamVar(MESourceStarted.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::from(0i64))?;
        }
        self.stream().start()
    }
    fn Stop(&self) -> Result<()> {
        self.stream().stop()?;
        unsafe { self.queue.QueueEventParamVar(MESourceStopped.0 as u32, &GUID::zeroed(), S_OK, &PROPVARIANT::from(0i64)) }
    }
    fn Pause(&self) -> Result<()> {
        Err(MF_E_INVALID_STATE_TRANSITION.into())
    }
    fn Shutdown(&self) -> Result<()> {
        self.stream().shutdown();
        unsafe {
            let _ = self.queue.Shutdown();
        }
        Ok(())
    }
}

impl IMFMediaSourceEx_Impl for Source_Impl {
    fn GetSourceAttributes(&self) -> Result<IMFAttributes> {
        Ok(self.attrs.clone())
    }
    fn GetStreamAttributes(&self, id: u32) -> Result<IMFAttributes> {
        if id != 0 {
            return Err(E_INVALIDARG.into());
        }
        Ok(self.stream().attrs.clone())
    }
    fn SetD3DManager(&self, _: Ref<'_, IUnknown>) -> Result<()> {
        Ok(())
    }
}

impl IMFGetService_Impl for Source_Impl {
    fn GetService(&self, _: *const GUID, _: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        if !ppv.is_null() {
            unsafe { *ppv = std::ptr::null_mut() };
        }
        Err(MF_E_UNSUPPORTED_SERVICE.into())
    }
}

impl IKsControl_Impl for Source_Impl {
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

//! COM plumbing: the class factory and the activation object the camera
//! service uses to create the media source.

use crate::source::make_source;
use crate::stream::lock;
use std::ffi::c_void;
use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::*;

#[implement(IMFActivate)]
struct Activator { a: IMFAttributes, src: Mutex<Option<IMFMediaSourceEx>> }

impl IMFActivate_Impl for Activator_Impl {
    fn ActivateObject(&self, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
        unsafe {
            *ppv = std::ptr::null_mut();
            let mut g = lock(&self.src);
            if g.is_none() { *g = Some(make_source(&self.a)?); }
            g.as_ref().unwrap().query(riid, ppv).ok()
        }
    }
    fn ShutdownObject(&self) -> Result<()> {
        // The source and its stream hold each other; only Shutdown lets go.
        if let Some(src) = lock(&self.src).take() { unsafe { let _ = src.Shutdown(); } }
        Ok(())
    }
    fn DetachObject(&self) -> Result<()> { *lock(&self.src) = None; Ok(()) }
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

#[implement(IClassFactory)]
pub(crate) struct Factory;
impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<'_, IUnknown>, riid: *const GUID, ppv: *mut *mut c_void) -> Result<()> {
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


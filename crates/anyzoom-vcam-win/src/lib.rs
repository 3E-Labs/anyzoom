//! anyzoom's virtual camera.
//!
//! Built two ways from one crate: as `anyzoom_vcam.dll`, which the Windows
//! camera service loads to produce the "anyzoom Camera" video, and as a
//! library the app links for the pieces both sides share (the frame file and
//! starting/removing the camera).
#![cfg(windows)]
#![allow(non_snake_case, clippy::missing_safety_doc)]

mod activate;
pub mod host;
pub mod mapping;
mod source;
mod stream;

use std::ffi::c_void;
use windows::core::{Interface, GUID, HRESULT};
use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, E_POINTER, S_FALSE};
use windows::Win32::System::Com::IClassFactory;

/// COM class of the virtual camera's media source.
pub const CLSID_ANYZOOM_VCAM: GUID = GUID::from_u128(0x3e3e3e3e_a11c_4a00_9d00_616e797a6f01);
pub const CLSID_ANYZOOM_VCAM_STR: &str = "{3E3E3E3E-A11C-4A00-9D00-616E797A6F01}";
/// Name other apps see (Windows appends " (Windows Virtual Camera)").
pub const CAMERA_NAME: &str = "anyzoom Camera";
/// Folder and file through which the app hands frames to the camera.
pub const FRAME_DIR: &str = r"C:\ProgramData\anyzoom";
pub const FRAME_PATH: &str = r"C:\ProgramData\anyzoom\frame.bin";

/// Milliseconds since Windows started: the clock both processes stamp frames with.
pub fn now_ms() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if ppv.is_null() || rclsid.is_null() || riid.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();
    if *rclsid != CLSID_ANYZOOM_VCAM {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = activate::Factory.into();
    factory.query(riid, ppv)
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

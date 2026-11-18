//! App side: makes the virtual camera appear in Windows and removes it again.

use crate::{CAMERA_NAME, CLSID_ANYZOOM_VCAM_STR};
use windows::core::{Result, HSTRING};
use windows::Win32::Media::MediaFoundation::{
    IMFVirtualCamera, MFCreateVirtualCamera, MFShutdown, MFStartup, MFVirtualCameraAccess_CurrentUser, MFVirtualCameraLifetime_Session,
    MFVirtualCameraType_SoftwareCameraSource, MFSTARTUP_FULL, MF_VERSION,
};

/// The running "anyzoom Camera". It exists for the current user only, and
/// only until `remove` is called or the process ends.
pub struct VirtualCamera(Option<IMFVirtualCamera>);

impl VirtualCamera {
    /// COM must already be initialised on the calling thread. Fails if the
    /// camera component has not been registered (the installer does that).
    pub fn start() -> Result<VirtualCamera> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
            let camera = MFCreateVirtualCamera(
                MFVirtualCameraType_SoftwareCameraSource,
                MFVirtualCameraLifetime_Session,
                MFVirtualCameraAccess_CurrentUser,
                &HSTRING::from(CAMERA_NAME),
                &HSTRING::from(CLSID_ANYZOOM_VCAM_STR),
                None,
            )
            .and_then(|camera| camera.Start(None).map(|()| camera));
            match camera {
                Ok(camera) => Ok(VirtualCamera(Some(camera))),
                Err(e) => {
                    let _ = MFShutdown();
                    Err(e)
                }
            }
        }
    }

    pub fn remove(mut self) {
        self.remove_now();
    }

    fn remove_now(&mut self) {
        if let Some(camera) = self.0.take() {
            unsafe {
                let _ = camera.Remove();
                let _ = camera.Shutdown();
                drop(camera);
                let _ = MFShutdown();
            }
        }
    }
}

impl Drop for VirtualCamera {
    fn drop(&mut self) {
        self.remove_now();
    }
}

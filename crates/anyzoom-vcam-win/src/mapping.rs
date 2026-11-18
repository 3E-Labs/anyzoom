//! The frame file, mapped into memory. The app creates and writes it; the
//! camera service (a different Windows account) maps the same file.

use crate::{FRAME_DIR, FRAME_PATH};
use anyzoom_core::bus::bus_file_len;
use windows::core::{Result, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    GetSecurityDescriptorDacl, GetTokenInformation, TokenUser, ACL, DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER,
};
use windows::Win32::Storage::FileSystem::{
    CreateDirectoryW, CreateFileW, GetFileSizeEx, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_ALWAYS, OPEN_EXISTING,
};
use windows::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_ALL_ACCESS, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READONLY,
    PAGE_READWRITE,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub struct FrameMap {
    ptr: *mut u8,
    len: usize,
    writable: bool,
}

// The pointer refers to a process-wide mapping, valid on any thread.
unsafe impl Send for FrameMap {}

impl FrameMap {
    /// Camera side: maps the existing file, read-write when the account is
    /// allowed to write and read-only otherwise. `None` until the app has
    /// created the file.
    pub fn open_consumer() -> Option<FrameMap> {
        map_file(true, false).or_else(|| map_file(false, false))
    }

    /// App side: creates the folder and file if needed, limits access to the
    /// current user, administrators and the camera service, and maps it.
    pub fn create_producer() -> Result<FrameMap> {
        let dir = HSTRING::from(FRAME_DIR);
        unsafe {
            let _ = CreateDirectoryW(&dir, None); // already existing is fine
            restrict_access(&dir)?;
        }
        map_file(true, true).ok_or_else(windows::core::Error::from_win32)
    }

    pub fn writable(&self) -> bool {
        self.writable
    }

    pub fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// `None` when the file could only be mapped read-only.
    pub fn bytes_mut(&mut self) -> Option<&mut [u8]> {
        self.writable.then(|| unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) })
    }
}

impl Drop for FrameMap {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.ptr.cast() });
        }
    }
}

fn map_file(write: bool, create: bool) -> Option<FrameMap> {
    let len = bus_file_len();
    unsafe {
        let access = if write { (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0 } else { FILE_GENERIC_READ.0 };
        let file = CreateFileW(
            &HSTRING::from(FRAME_PATH),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            if create { OPEN_ALWAYS } else { OPEN_EXISTING },
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
        .ok()?;
        let file = Owned(file);
        if !create {
            // never map more than the file holds: touching pages past its end would fault
            let mut size = 0i64;
            GetFileSizeEx(file.0, &mut size).ok()?;
            if (size as u64) < len as u64 {
                return None;
            }
        }
        // creating the mapping at `len` grows a new file to the full size
        let mapping = CreateFileMappingW(file.0, None, if write { PAGE_READWRITE } else { PAGE_READONLY }, 0, len as u32, PCWSTR::null()).ok()?;
        let mapping = Owned(mapping);
        let view = MapViewOfFile(mapping.0, if write { FILE_MAP_ALL_ACCESS } else { FILE_MAP_READ }, 0, 0, len);
        if view.Value.is_null() {
            return None;
        }
        Some(FrameMap { ptr: view.Value.cast(), len, writable: write })
    }
}

struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Replaces the folder's permissions (and, by inheritance, the frame file's):
/// full control for the current user, administrators and SYSTEM; modify for
/// the camera service account (LOCAL SERVICE); nobody else. Video frames pass
/// through this file, so other accounts on the PC must not be able to read it.
unsafe fn restrict_access(dir: &HSTRING) -> Result<()> {
    let mut token = HANDLE::default();
    OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
    let token = Owned(token);
    let mut needed = 0u32;
    let _ = GetTokenInformation(token.0, TokenUser, None, 0, &mut needed);
    let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)]; // u64 keeps TOKEN_USER aligned
    GetTokenInformation(token.0, TokenUser, Some(buf.as_mut_ptr().cast()), needed, &mut needed)?;
    let user = &*(buf.as_ptr() as *const TOKEN_USER);
    let mut sid_text = PWSTR::null();
    ConvertSidToStringSidW(user.User.Sid, &mut sid_text)?;
    let sid = sid_text.to_string().unwrap_or_default();
    let _ = LocalFree(Some(HLOCAL(sid_text.0.cast())));

    // FA = full access; 0x1301bf = modify; OICI = inherited by files and subfolders; P = no inherited entries.
    let sddl = HSTRING::from(format!("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;{sid})(A;OICI;0x1301bf;;;LS)"));
    let mut sd = PSECURITY_DESCRIPTOR::default();
    ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut sd, None)?;
    let mut present = Default::default();
    let mut defaulted = Default::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let got = GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted);
    let result = match got {
        Ok(()) => SetNamedSecurityInfoW(
            dir,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(dacl),
            None,
        )
        .ok(),
        Err(e) => Err(e),
    };
    let _ = LocalFree(Some(HLOCAL(sd.0)));
    result
}

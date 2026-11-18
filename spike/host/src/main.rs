// Creates the session virtual camera and writes moving frames into the shared file.
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Memory::*;

const W: usize = 1280; const H: usize = 720; const HDR: usize = 16;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let secs: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let write_frames = args.get(2).map(|s| s != "nopush").unwrap_or(true);
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        let cam = MFCreateVirtualCamera(MFVirtualCameraType_SoftwareCameraSource, MFVirtualCameraLifetime_Session, MFVirtualCameraAccess_CurrentUser,
            w!("anyzoom spike"), w!("{3E3E3E3E-A11C-4A00-9D00-616E797A6F6F}"), None)?;
        println!("created");
        cam.Start(None)?;
        println!("started; running {secs}s, write_frames={write_frames}");
        let total = HDR + W * H * 3 / 2;
        let mut view: *mut u8 = std::ptr::null_mut();
        if write_frames {
            let f = CreateFileW(w!("C:\\ProgramData\\anyzoom-spike\\frame.bin"), (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, None)?;
            let m = CreateFileMappingW(f, None, PAGE_READWRITE, 0, total as u32, None)?;
            view = MapViewOfFile(m, FILE_MAP_ALL_ACCESS, 0, 0, total).Value as *mut u8;
            println!("mapping ready: {}", !view.is_null());
        }
        let t0 = std::time::Instant::now();
        let mut n = 0usize;
        while t0.elapsed().as_secs() < secs {
            if !view.is_null() {
                let buf = std::slice::from_raw_parts_mut(view, total);
                let y = &mut buf[HDR..HDR + W * H];
                for row in 0..H { for col in 0..W {
                    // checkerboard that scrolls: unmistakably different from the DLL's bars
                    y[row * W + col] = if ((col + n * 4) / 80 + row / 80) % 2 == 0 { 200 } else { 60 };
                } }
                let uv = &mut buf[HDR + W * H..];
                for (i, b) in uv.iter_mut().enumerate() { *b = if i % 2 == 0 { 90 } else { 200 }; } // red-ish tint
                buf[0..4].copy_from_slice(b"AZ01");
                buf[4..8].copy_from_slice(&(n as u32).to_le_bytes());
                n += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(33));
        }
        cam.Remove()?;
        let _ = cam.Shutdown();
        MFShutdown()?;
    }
    println!("done");
    Ok(())
}

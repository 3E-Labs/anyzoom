# spike (throwaway)

Proves a Rust Media Foundation virtual camera, cross-compiled from WSL, shows
up on Windows 11 and can carry frames written by another process.
Not product code. No tests. Delete once the real crates exist.

## Result (2026-11-17): works

- Built in WSL with `cargo xwin build --release --target x86_64-pc-windows-msvc`
  (rustup target + `cargo-xwin`, no Windows toolchain, no sudo).
- DLL copied to `C:\ProgramData\anyzoom-spike\`, CLSID registered under HKLM
  (one UAC prompt), `vcamhost.exe` calls `MFCreateVirtualCamera` (session
  lifetime, current user).
- "anyzoom spike (Windows Virtual Camera)" appeared in the DirectShow device
  list and ffmpeg captured 1280x720 frames from it.
- The captured frames were the checkerboard written by `vcamhost.exe`, not the
  DLL's fallback bars: a file-backed mapping in `C:\ProgramData` is readable by
  the camera service account. No named kernel objects needed.
- Not proven here: the DLL's own log file was not written by the camera service
  (it likely cannot write to that folder); log elsewhere in the real build.
- Note: Windows appends " (Windows Virtual Camera)" to the friendly name.

<p align="center">
  <img src="app/src-tauri/icons/icon.png" width="96" alt="anyzoom icon: the 3E mark inside a camera viewfinder">
</p>

<h1 align="center">anyzoom</h1>

<p align="center">Zoom and pan for any webcam on Windows 11.</p>

<p align="center">
  <a href="https://github.com/3E-Labs/anyzoom/releases">Download</a> ·
  <a href="#install">Install</a> ·
  <a href="#using-it">Using it</a> ·
  <a href="#build-from-source">Build from source</a>
</p>

<p align="center">
  <a href="https://github.com/3E-Labs/anyzoom/actions/workflows/release.yml"><img src="https://github.com/3E-Labs/anyzoom/actions/workflows/release.yml/badge.svg" alt="Build status"></a>
  <img src="https://img.shields.io/badge/platform-Windows%2011-1F3CFF" alt="Platform: Windows 11">
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-MIT-1F3CFF" alt="Licence: MIT"></a>
</p>

<p align="center"><sub>An open source tool from <a href="https://github.com/3E-Labs">3E Labs</a></sub></p>

---

Many webcams have no zoom at all. anyzoom takes the picture from the camera
you already own, lets you zoom from 1× to 4× and move around inside it, and
offers the result to every other app as a camera called **anyzoom Camera**.
Pick that camera in Meet, Zoom, Teams, Discord or OBS and they get your framed
picture.

No driver, no account, no network. One small app and one Windows camera
component.

## Requirements

- Windows 11 (the virtual camera uses a feature Windows 10 does not have)
- Any webcam Windows can see

## Install

1. Download the zip from the [releases page](https://github.com/3E-Labs/anyzoom/releases) and unzip it anywhere.
2. Double-click `install.cmd`. Windows asks for administrator permission once,
   to register the virtual camera.
3. Open **anyzoom** from the Start Menu.
4. In your call app, choose **anyzoom Camera** as the camera.

To remove it, run `uninstall.cmd` from `C:\Program Files\anyzoom`.

### "Windows protected your PC"

This first release is not code-signed, so SmartScreen may warn when you run
it. Choose **More info**, then **Run anyway**. If you would rather not trust a
download, build it yourself from this source (see [Build from source](#build-from-source)).

## Using it

- Drag the slider, or scroll on the picture, to zoom.
- Drag the blue frame to choose which part of the picture is sent. Double-click
  it to reset.
- Hotkeys work while any app is in front:

  | Action | Default keys |
  | --- | --- |
  | Zoom in / out | `Ctrl+Alt+=` / `Ctrl+Alt+-` |
  | Reset | `Ctrl+Alt+0` |
  | Move | `Ctrl+Alt+` arrow keys |

  Click a hotkey in the window to change it. If another program already owns
  a combination, anyzoom tells you which one is not working.
- Closing the window keeps anyzoom running in the system tray, so your call
  keeps its picture. Right-click the tray icon and choose **Quit** to stop it.
- **Start with Windows** (off by default) starts anyzoom in the tray so the
  camera is there before your call starts.

### Good to know

- **anyzoom Camera only exists while anyzoom is running.** After Quit it
  disappears from other apps' camera lists.
- Your real camera is switched on only while the anyzoom window is showing its
  preview or another app is using anyzoom Camera. Otherwise its light is off.
- Only one program can use a physical camera at a time. If another app holds
  your webcam directly, anyzoom Camera shows a "camera unavailable" card until
  that app lets go.
- Zoom is digital: it crops and enlarges. With a 1080p webcam and the default
  720p output, zoom up to 1.5× loses no detail.
- The output is always 16:9. A 4:3 camera is cropped to fill it.
- One Windows account per PC: the file video passes through is private to the
  first account that runs anyzoom. For another account to use it, an
  administrator has to delete `C:\ProgramData\anyzoom` first (uninstalling
  does that).

## What it does not do

- macOS, Linux or Windows 10
- Face tracking, backgrounds, filters, recording
- Driving a camera's own optical or firmware zoom
- Anything with your microphone

## Privacy

anyzoom never connects to the network. It has no telemetry, no account and no
update check. Video goes from your camera to the app you choose, on your PC,
and nowhere else. Settings are stored in `%APPDATA%\anyzoom\settings.json`.

anyzoom only reads video from your camera. It never changes camera settings
or sends vendor-specific commands to the device.

## How it works

anyzoom reads frames from the webcam through Media Foundation, crops and
rescales them, and writes each result into a small file that is mapped into
memory. A component registered with Windows (`anyzoom_vcam.dll`, a Media
Foundation media source created with `MFCreateVirtualCamera`) runs inside the
Windows camera service, reads the same mapped file and hands the frames to
whichever app opened anyzoom Camera. No kernel driver is involved.

The frame file is limited to your account and the camera service. The camera
component checks every header field before it reads a frame, so a damaged or
altered file cannot make it read outside the file.

```
crates/anyzoom-core         zoom, crop, scaling, easing, settings, frame format (no OS calls, unit-tested)
crates/anyzoom-capture-win  finding and reading webcams
crates/anyzoom-vcam-win     the virtual camera component
crates/anyzoom-engine       the pipeline thread joining the three
app/                        the Tauri app and its React window
scripts/                    build, package, install, uninstall
```

## Build from source

On Windows, with Rust, Node 20+ and the Visual Studio C++ build tools:

```
cd app && npm ci && npm run build && cd ..
cargo build --release -p anyzoom-vcam-win --lib
cargo build --release -p anyzoom-app --features custom-protocol
```

Copy `target\release\anyzoom.exe` and `anyzoom_vcam.dll` next to the files in
`scripts\` and run `install.cmd`.

On Linux or WSL you can cross-compile with `cargo-xwin`: see
`scripts/build.sh` and `scripts/package.sh`.

Tests: `cargo test` (core logic, any platform) and `npm test` in `app/`.
The Windows-only parts are checked by hand with
[docs/manual-test-checklist.md](docs/manual-test-checklist.md).

## Security

anyzoom installs a component that runs inside the Windows camera service.
Please report security problems privately, not in a public issue: see
[SECURITY.md](SECURITY.md).

## Licence

MIT. See [LICENSE](LICENSE). The bundled fonts (Space Grotesk, Geist, Geist
Mono) are under the SIL Open Font License.

---

<sub><b>3E Labs</b> is a technology lab in New York. We make open source tools
anyone can use, datasets technical teams can rely on, and custom software for
companies and institutions. <a href="https://github.com/3E-Labs">More from 3E Labs</a>.
3E Labs is the public name of 3E Studios LLC.</sub>

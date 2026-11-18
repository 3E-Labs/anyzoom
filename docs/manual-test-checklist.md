# Manual test checklist

The Windows-only parts cannot be unit-tested. Run this list on a Windows 11 PC
with a webcam before a release. `ffmpeg` is used to look at what the virtual
camera really sends:

```
ffmpeg -f dshow -i video="anyzoom Camera (Windows Virtual Camera)" -frames:v 1 shot.png
```

## Install

- [ ] `install.cmd` asks for admin once and reports success
- [ ] anyzoom appears in the Start Menu and starts from it
- [ ] A second launch brings the first window forward instead of opening another

## Virtual camera

- [ ] `ffmpeg -list_devices true -f dshow -i dummy` lists "anyzoom Camera (Windows Virtual Camera)"
- [ ] A frame grabbed at the default size is 1280×720 and shows the live picture
- [ ] A frame grabbed with `-video_size 1920x1080` is 1920×1080
- [ ] With the output set to 1080p in the window, both sizes still work
- [ ] The status line says IN USE while another app shows the camera, IDLE after

## Camera light

- [ ] Window hidden to tray, nothing using the camera: light is off
- [ ] Another app opens anyzoom Camera: light comes on, picture is live within about a second
- [ ] That app closes the camera: light goes off within about 5 seconds
- [ ] Window shown: light on. Window minimised or closed to tray: light off within about 5 seconds

## Zoom and pan

- [ ] Slider moves the zoom smoothly from 1.00× to 4.00×
- [ ] Mouse wheel on the picture zooms
- [ ] Dragging the frame pans; the frame cannot leave the picture
- [ ] Double-clicking the frame, and the Reset button, return to 1.00×
- [ ] What ffmpeg captures matches the frame shown in the window

## Hotkeys

- [ ] Each default hotkey works while another app is in front
- [ ] Rebinding one works immediately
- [ ] Binding a combination another program owns shows "Not working" under it; the rest still work
- [ ] Backspace while rebinding clears the hotkey

## Trouble

- [ ] Unplug the webcam mid-call: other apps see the "camera unavailable" card; replug and the picture returns
- [ ] Another app holding the physical webcam: card shown, window explains why
- [ ] A 4:3 camera (or a 640×480 mode): output is 16:9, filled, not stretched
- [ ] Put garbage in `%APPDATA%\anyzoom\settings.json`: anyzoom starts with defaults

## Tray, settings, exit

- [ ] Closing the window keeps the tray icon and the virtual camera
- [ ] Tray > Reset zoom works; tray > Open shows the window
- [ ] Tray > Quit removes anyzoom Camera from the device list
- [ ] Camera, zoom, pan, output size and hotkeys survive a restart
- [ ] Start with Windows on: after sign-in anyzoom is in the tray with no window

## Uninstall

- [ ] `uninstall.cmd` removes `C:\Program Files\anyzoom`, the Start Menu entry and `C:\ProgramData\anyzoom`
- [ ] `reg query "HKLM\SOFTWARE\Classes\CLSID\{3E3E3E3E-A11C-4A00-9D00-616E797A6F01}"` finds nothing

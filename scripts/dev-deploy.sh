#!/bin/bash
# Development only: copies freshly built Windows binaries from WSL to
# C:\ProgramData\anyzoom-dev. A DLL that the camera service still has loaded
# cannot be overwritten, so the old one is renamed out of the way first.
set -e
cd "$(dirname "$0")/.."
OUT=target/x86_64-pc-windows-msvc/release
DEV=/mnt/c/ProgramData/anyzoom-dev
mkdir -p "$DEV"
if [ -f "$DEV/anyzoom_vcam.dll" ] && ! cp "$OUT/anyzoom_vcam.dll" "$DEV/anyzoom_vcam.dll" 2>/dev/null; then
  mv "$DEV/anyzoom_vcam.dll" "$DEV/anyzoom_vcam.old-$(date +%s).dll"
fi
cp "$OUT/anyzoom_vcam.dll" "$DEV/"
for f in "$OUT"/examples/*.exe "$OUT"/*.exe; do
  case "$(basename "$f")" in *-????????????????.exe) continue;; esac
  [ -f "$f" ] && cp "$f" "$DEV/" || true
done
cp scripts/dev-register.ps1 "$DEV/"
ls "$DEV"

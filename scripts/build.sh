#!/bin/bash
# Builds everything for Windows from Linux/WSL:
#   - the web UI (app/dist)
#   - anyzoom.exe and anyzoom_vcam.dll (target/x86_64-pc-windows-msvc/release)
# Needs: Rust with the x86_64-pc-windows-msvc target, cargo-xwin, Node 20+,
# and clang-cl + llvm-rc on PATH (or unpacked under ~/.local/opt/llvm18).
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d "$HOME/.local/opt/llvm18/bin" ] && export PATH="$HOME/.local/opt/llvm18/bin:$PATH"
TARGET=x86_64-pc-windows-msvc

(cd app && { [ -d node_modules ] || npm ci; } && npm run build)
cargo xwin build --release --target $TARGET -p anyzoom-vcam-win -p anyzoom-capture-win -p anyzoom-engine --lib --examples
cargo xwin build --release --target $TARGET -p anyzoom-app --features custom-protocol
ls -la target/$TARGET/release/anyzoom.exe target/$TARGET/release/anyzoom_vcam.dll

#!/bin/bash
# Builds anyzoom and assembles the release zip in dist/.
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NAME="anyzoom-v$VERSION-windows-x64"
OUT=target/x86_64-pc-windows-msvc/release

scripts/build.sh
rm -rf "dist/$NAME" "dist/$NAME.zip"
mkdir -p "dist/$NAME"
cp "$OUT/anyzoom.exe" "$OUT/anyzoom_vcam.dll" "dist/$NAME/"
cp scripts/install.ps1 scripts/install.cmd scripts/uninstall.ps1 scripts/uninstall.cmd README.md LICENSE "dist/$NAME/"
(cd dist && python3 -c "import shutil,sys; shutil.make_archive(sys.argv[1], 'zip', '.', sys.argv[1])" "$NAME")
ls -la "dist/$NAME.zip"

#!/bin/bash
# Builds rust/target/PadBridge.app for macOS 10.13 and newer (Intel; one binary
# runs on the 2010 iMac and current Macs). Ad-hoc signed: macOS asks for the
# Accessibility permission again after every new build.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(git describe --tags --exact-match 2>/dev/null | sed 's/^v//' || true)"
VERSION="${VERSION:-$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"name":"padbridge","version":"\([^"]*\)".*/\1/p')}"
BUILD="$(git rev-list --count HEAD)"

export MACOSX_DEPLOYMENT_TARGET=10.13
# Keep the home directory (user name) out of paths compiled into the binary.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
cargo build --release -p padbridge

APP=target/PadBridge.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/padbridge "$APP/Contents/MacOS/PadBridge"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>         <string>PadBridge</string>
    <key>CFBundleIdentifier</key>         <string>io.github.throwingsnow.padbridge</string>
    <key>CFBundleName</key>               <string>PadBridge</string>
    <key>CFBundlePackageType</key>        <string>APPL</string>
    <key>CFBundleShortVersionString</key> <string>${VERSION}</string>
    <key>CFBundleVersion</key>            <string>${BUILD}</string>
    <key>LSMinimumSystemVersion</key>     <string>10.13</string>
    <key>NSHighResolutionCapable</key>    <true/>
</dict>
</plist>
PLIST

codesign --force --sign - "$APP" 2>/dev/null || echo "codesign skipped"
echo "Built $APP ($VERSION, build $BUILD)"

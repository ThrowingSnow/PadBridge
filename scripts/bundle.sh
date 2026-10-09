#!/bin/bash
# Builds PadBridge.app (release) into ./build.
# A real bundle is needed later for Accessibility permission (keyboard/mouse
# output) and login items; for quick tests `swift run PadBridgeApp` is enough.
set -euo pipefail
cd "$(dirname "$0")/.."

# Version comes from the latest git tag (v0.1.0 -> 0.1.0); "-dev" outside a tag.
VERSION="$(git describe --tags --exact-match 2>/dev/null | sed 's/^v//' || true)"
VERSION="${VERSION:-$(git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//')-dev}"
BUILD="$(git rev-list --count HEAD)"

swift build -c release --product PadBridgeApp
BIN="$(swift build -c release --show-bin-path)/PadBridgeApp"

APP=build/PadBridge.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/PadBridge"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>             <string>PadBridge</string>
    <key>CFBundleDisplayName</key>      <string>PadBridge</string>
    <key>CFBundleIdentifier</key>       <string>io.github.throwingsnow.padbridge</string>
    <key>CFBundleExecutable</key>       <string>PadBridge</string>
    <key>CFBundlePackageType</key>      <string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleVersion</key>          <string>${BUILD}</string>
    <key>LSMinimumSystemVersion</key>   <string>13.0</string>
    <key>NSHighResolutionCapable</key>  <true/>
</dict>
</plist>
PLIST

# Ad-hoc signature so Gatekeeper/TCC can identify the app locally.
codesign --force --sign - "$APP"
echo "Built $APP ($VERSION, build $BUILD)"

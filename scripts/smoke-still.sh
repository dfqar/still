#!/bin/sh
# Exercise a native window and real Metal presents. Exit 2 means no OS
# occlusion signal was available, with other checks recorded in the report.
set -eu

PROJECT_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
"$PROJECT_ROOT/scripts/build-rust.sh"
APP="$PROJECT_ROOT/build/still-smoke/StillHostSmoke.app"
EVIDENCE_DIR=${STILL_HOST_SMOKE_EVIDENCE_DIR:-"$PROJECT_ROOT/build/still-smoke/latest"}
mkdir -p "$APP/Contents/MacOS" "$EVIDENCE_DIR"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>app.still.host-smoke</string>
    <key>CFBundleExecutable</key><string>StillHostSmoke</string>
    <key>CFBundleName</key><string>Still Native Host Check</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

xcrun swiftc -swift-version 5 -target arm64-apple-macosx14.0 -parse-as-library \
  -import-objc-header "$PROJECT_ROOT/include/still.h" \
  "$PROJECT_ROOT/macOS/AppModel.swift" \
  "$PROJECT_ROOT/macOS/EngineController.swift" \
  "$PROJECT_ROOT/macOS/MetalHostView.swift" \
  "$PROJECT_ROOT/macOS/Tests/StillHostSmoke.swift" \
  "$PROJECT_ROOT/target/release/libstill_ffi.a" \
  -framework Metal -framework QuartzCore -framework AppKit -framework CoreGraphics \
  -framework Foundation -framework IOKit -framework IOSurface -lc++ \
  -o "$APP/Contents/MacOS/StillHostSmoke"
codesign --force --sign - "$APP"
export STILL_PROFILE_RUN=1
export STILL_PROFILE_WHEN_INACTIVE=0
exec "$APP/Contents/MacOS/StillHostSmoke" "$EVIDENCE_DIR"

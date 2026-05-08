#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
  cat >&2 <<'USAGE'
Usage: tools/package-macos.sh <binary-path> <version> <output-dir> [codesign-identity]

Creates:
  - LazyTime.app bundle
  - LazyTime-<version>.dmg (drag-and-drop with Applications symlink)

Examples:
  tools/package-macos.sh target/release/lazytime 0.3.0 dist
  tools/package-macos.sh target/release/lazytime 0.3.0 dist "-"
USAGE
  exit 1
fi

BINARY_PATH="$1"
VERSION="$2"
OUTPUT_DIR="$3"
CODESIGN_IDENTITY="${4:--}"

if [[ ! -f "$BINARY_PATH" ]]; then
  echo "Binary not found: $BINARY_PATH" >&2
  exit 1
fi

APP_NAME="LazyTime.app"
EXECUTABLE_NAME="LazyTime"
BUNDLE_ID="com.lazytime.app"

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
WORK_DIR="$ROOT_DIR/.build/macos-package"
APP_DIR="$WORK_DIR/$APP_NAME"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
ICON_SOURCE_PNG="$ROOT_DIR/icon_black.png"

ICONSET_DIR="$WORK_DIR/LazyTime.iconset"
ICON_FILE="$RESOURCES_DIR/LazyTime.icns"

mkdir -p "$OUTPUT_DIR"
rm -rf "$WORK_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR" "$ICONSET_DIR"

if [[ ! -f "$ICON_SOURCE_PNG" ]]; then
  echo "Icon source not found: $ICON_SOURCE_PNG" >&2
  exit 1
fi

cp "$BINARY_PATH" "$MACOS_DIR/$EXECUTABLE_NAME"
chmod +x "$MACOS_DIR/$EXECUTABLE_NAME"

cat > "$CONTENTS_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>LazyTime</string>
  <key>CFBundleExecutable</key>
  <string>${EXECUTABLE_NAME}</string>
  <key>CFBundleIconFile</key>
  <string>LazyTime.icns</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>LazyTime</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSAppleEventsUsageDescription</key>
  <string>LazyTime needs Automation permission to read active app/window details through System Events.</string>
  <key>NSHumanReadableCopyright</key>
  <string>Copyright (c) LazyTime contributors</string>
</dict>
</plist>
PLIST

declare -a SIZES=(16 32 64 128 256 512)
for s in "${SIZES[@]}"; do
  sips -z "$s" "$s" "$ICON_SOURCE_PNG" --out "$ICONSET_DIR/icon_${s}x${s}.png" >/dev/null
done
cp "$ICONSET_DIR/icon_32x32.png" "$ICONSET_DIR/icon_16x16@2x.png"
cp "$ICONSET_DIR/icon_64x64.png" "$ICONSET_DIR/icon_32x32@2x.png"
cp "$ICONSET_DIR/icon_256x256.png" "$ICONSET_DIR/icon_128x128@2x.png"
cp "$ICONSET_DIR/icon_512x512.png" "$ICONSET_DIR/icon_256x256@2x.png"
cp "$ICONSET_DIR/icon_512x512.png" "$ICONSET_DIR/icon_512x512@2x.png"

iconutil -c icns "$ICONSET_DIR" -o "$ICON_FILE"

if [[ -n "$CODESIGN_IDENTITY" ]]; then
  codesign --force --sign "$CODESIGN_IDENTITY" --timestamp=none "$APP_DIR"
fi

STAGE_DIR="$WORK_DIR/dmg-stage"
mkdir -p "$STAGE_DIR"
cp -R "$APP_DIR" "$STAGE_DIR/$APP_NAME"
ln -s /Applications "$STAGE_DIR/Applications"

DMG_FILE="$OUTPUT_DIR/LazyTime-${VERSION}.dmg"
rm -f "$DMG_FILE"
hdiutil create -volname "LazyTime" -srcfolder "$STAGE_DIR" -ov -format UDZO "$DMG_FILE"

echo "Created app bundle: $APP_DIR"
echo "Created dmg: $DMG_FILE"

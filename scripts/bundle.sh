#!/bin/sh
# Build release and install "/Applications/Westron.app".
# scripts/bundle.sh <binary> <out dir> packages a prebuilt binary without installing.
# Icon: assets/icon.svg is the source; scripts/render-icon.swift renders it, and AppIcon.icns is
# made from that (iconutil, sips).
# The app's binary is the only one: the `westron` CLI (a symlink in ~/.local/bin) runs it too.
set -e
cd "$(dirname "$0")/.."
bin="${1:-target/release/westron}"
app="${2:-/Applications}/Westron.app"
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
[ -n "$1" ] || cargo build --release -j 4
# Updated in place, not deleted: deleting it dropped it from Spotlight until macOS indexed it again.
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/westron.new"
mv -f "$app/Contents/MacOS/westron.new" "$app/Contents/MacOS/westron"
cp assets/AppIcon.icns "$app/Contents/Resources/"
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Westron</string>
  <key>CFBundleDisplayName</key><string>Westron</string>
  <key>CFBundleIdentifier</key><string>com.tarikalim.westron</string>
  <key>CFBundleExecutable</key><string>westron</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>LSEnvironment</key><dict><key>MallocSpaceEfficient</key><string>1</string></dict>
</dict></plist>
PLIST
codesign --force --deep -s - "$app"
# Updated in place, the bundle keeps its date and macOS keeps the cached icon; a new date refreshes it.
touch "$app"
if [ -z "$1" ]; then
  "$lsregister" -f "$app"
  mdimport "$app"
  mkdir -p "$HOME/.local/bin"
  ln -sf "$app/Contents/MacOS/westron" "$HOME/.local/bin/westron"
fi
echo "installed: $app"

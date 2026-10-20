#!/bin/sh
# Monta HyzeVM.app (macOS) a partir dos binários de release.
set -e
cd "$(dirname "$0")/.."
cargo build --release -p hyzevm-app -p hyzevm
APP="HyzeVM.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/SharedSupport" "$APP/Contents/Resources"
cp target/release/hyzevm-app "$APP/Contents/MacOS/HyzeVM"
cp target/release/hyzevm "$APP/Contents/SharedSupport/hyzevm"
# ícone .icns a partir do PNG
ISET=$(mktemp -d)/HyzeVM.iconset; mkdir -p "$ISET"
for s in 16 32 64 128 256 512; do
  magick app/icons/icon.png -resize ${s}x${s} "$ISET/icon_${s}x${s}.png"
  magick app/icons/icon.png -resize $((s*2))x$((s*2)) "$ISET/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$ISET" -o "$APP/Contents/Resources/HyzeVM.icns"
cat > "$APP/Contents/Info.plist" <<'PL'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>HyzeVM</string>
<key>CFBundleDisplayName</key><string>HyzeVM</string>
<key>CFBundleIdentifier</key><string>sh.hyzevm.app</string>
<key>CFBundleExecutable</key><string>HyzeVM</string>
<key>CFBundleIconFile</key><string>HyzeVM</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSMinimumSystemVersion</key><string>12.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
</dict></plist>
PL
codesign --force --deep --sign - "$APP"
echo "ok: $APP"

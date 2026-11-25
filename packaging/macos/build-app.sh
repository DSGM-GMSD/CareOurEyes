#!/bin/bash
# Arma CareOurEyes.app (universal: Apple Silicon e Intel) y el .dmg.
# Uso (en un Mac, desde la raíz del repositorio): packaging/macos/build-app.sh 2.0.0-alpha.2
set -euo pipefail
VERSION="${1:?versión}"
NUMERIC="${VERSION%%-*}"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

APP=dist/CareOurEyes.app
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/CareOurEyes" \
  target/aarch64-apple-darwin/release/CareOurEyes \
  target/x86_64-apple-darwin/release/CareOurEyes
sed -e "s/@VERSION@/${VERSION}/" -e "s/@NUMERIC@/${NUMERIC}/" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# Icono .icns a partir del PNG de 512 px.
ICONSET="$(mktemp -d)/careoureyes.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256; do
  sips -z $s $s assets/icon.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) assets/icon.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
cp assets/icon.png "$ICONSET/icon_512x512.png"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/careoureyes.icns"

# Firma ad hoc (sin cuenta de desarrollador): necesaria para que abra en Apple Silicon.
codesign --force --deep --sign - "$APP"

# Imagen de disco con acceso directo a Aplicaciones.
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "CareOurEyes" -srcfolder "$STAGE" -ov -format UDZO "dist/CareOurEyes-${VERSION}-macos.dmg"
ls -l dist

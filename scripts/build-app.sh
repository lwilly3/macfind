#!/bin/bash
# Construit macfind.app dans dist/ :
#   - compilation release du binaire Rust et de l'interface SwiftUI ;
#   - binaire `macfind` embarqué dans Contents/MacOS, à côté de l'interface ;
#   - Info.plist, signature ad hoc (sans compte développeur Apple).
#
# Usage : scripts/build-app.sh             # construit dist/macfind.app
#         scripts/build-app.sh --install   # puis l'installe dans /Applications
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL=0
if [ "${1:-}" = "--install" ]; then
    INSTALL=1
fi
APP="$ROOT/dist/macfind.app"
CONTENTS="$APP/Contents"
MACOS_DIR="$CONTENTS/MacOS"

# Source unique de la version : Cargo.toml.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
# Numéro de build : nombre de commits, pour distinguer deux bundles de même version.
BUILD="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
echo "== Version $VERSION (build $BUILD)"

echo "== Compilation du binaire macfind"
cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"

echo "== Compilation de l'interface"
swift build -c release --package-path "$ROOT/app"
INTERFACE="$(swift build -c release --package-path "$ROOT/app" --show-bin-path)/MacfindApp"
[ -f "$INTERFACE" ] || { echo "Interface introuvable : $INTERFACE" >&2; exit 1; }

echo "== Structure du bundle"
rm -rf "$APP"
mkdir -p "$MACOS_DIR" "$CONTENTS/Resources"
cp "$INTERFACE" "$MACOS_DIR/MacfindApp"
cp "$ROOT/target/release/macfind" "$MACOS_DIR/macfind"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key><string>fr</string>
	<key>CFBundleExecutable</key><string>MacfindApp</string>
	<key>CFBundleIdentifier</key><string>io.github.lwilly3.macfind</string>
	<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
	<key>CFBundleName</key><string>macfind</string>
	<key>CFBundleDisplayName</key><string>macfind</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>$VERSION</string>
	<key>CFBundleVersion</key><string>$BUILD</string>
	<key>LSMinimumSystemVersion</key><string>14.0</string>
	<key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
	<key>NSHighResolutionCapable</key><true/>
	<key>NSPrincipalClass</key><string>NSApplication</string>
	<key>NSLocalNetworkUsageDescription</key><string>macfind interroge vos routeurs et balaie le réseau local pour retrouver les équipements.</string>
	<key>NSHumanReadableCopyright</key><string>Licence MIT.</string>
</dict>
</plist>
PLIST
printf 'APPL????' > "$CONTENTS/PkgInfo"

echo "== Signature ad hoc"
codesign --force --sign - "$MACOS_DIR/macfind"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP" && echo "  signature : OK"
echo "Bundle prêt : $APP ($(du -sh "$APP" | awk '{print $1}'))"

if [ "$INSTALL" = "1" ]; then
    echo "== Installation dans /Applications"
    TARGET="/Applications/macfind.app"
    osascript -e 'tell application id "io.github.lwilly3.macfind" to quit' >/dev/null 2>&1 || true
    rm -rf "$TARGET"
    cp -R "$APP" "$TARGET"
    echo "Installé : $TARGET"
else
    echo "Lancer avec : open \"$APP\""
    echo "Installer   : scripts/build-app.sh --install"
fi

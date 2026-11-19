#!/bin/bash
# Construit macfind.app dans dist/ :
#   - compilation release du binaire Rust et de l'interface SwiftUI ;
#   - binaire `macfind` embarqué dans Contents/MacOS, à côté de l'interface ;
#   - Info.plist, icône, signature ad hoc (sans compte développeur Apple).
#
# Usage : scripts/build-app.sh [options]
#   (sans option)   construit dist/macfind.app pour ce Mac
#   --install       puis l'installe dans /Applications
#   --universal     binaire universel (Apple Silicon et Intel) ; demande la
#                   cible Rust de l'autre architecture :
#                   rustup target add aarch64-apple-darwin x86_64-apple-darwin
#   --dmg           crée aussi dist/macfind-<version>.dmg
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
INSTALL=0
UNIVERSAL=0
DMG=0
for option in "$@"; do
    case "$option" in
        --install) INSTALL=1 ;;
        --universal) UNIVERSAL=1 ;;
        --dmg) DMG=1 ;;
        *) echo "Option inconnue : $option" >&2; exit 2 ;;
    esac
done
APP="$ROOT/dist/macfind.app"
CONTENTS="$APP/Contents"
MACOS_DIR="$CONTENTS/MacOS"

# Source unique de la version : Cargo.toml.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
# Numéro de build : nombre de commits, pour distinguer deux bundles de même version.
BUILD="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
echo "== Version $VERSION (build $BUILD)"

if [ "$UNIVERSAL" = "1" ]; then
    echo "== Compilation du binaire macfind (universel)"
    for cible in aarch64-apple-darwin x86_64-apple-darwin; do
        cargo build --release --locked --manifest-path "$ROOT/Cargo.toml" --target "$cible"
    done
    BINAIRE="$ROOT/target/macfind-universel"
    lipo -create -output "$BINAIRE" \
        "$ROOT/target/aarch64-apple-darwin/release/macfind" \
        "$ROOT/target/x86_64-apple-darwin/release/macfind"
    ARCHS=(--arch arm64 --arch x86_64)
else
    echo "== Compilation du binaire macfind"
    cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"
    BINAIRE="$ROOT/target/release/macfind"
    ARCHS=()
fi

echo "== Compilation de l'interface"
# ${ARCHS[@]+…} : un tableau vide est une erreur sous `set -u` avec bash 3.2.
swift build -c release --package-path "$ROOT/app" ${ARCHS[@]+"${ARCHS[@]}"}
INTERFACE="$(swift build -c release --package-path "$ROOT/app" ${ARCHS[@]+"${ARCHS[@]}"} --show-bin-path)/MacfindApp"
[ -f "$INTERFACE" ] || { echo "Interface introuvable : $INTERFACE" >&2; exit 1; }

echo "== Structure du bundle"
rm -rf "$APP"
mkdir -p "$MACOS_DIR" "$CONTENTS/Resources"
cp "$INTERFACE" "$MACOS_DIR/MacfindApp"
cp "$BINAIRE" "$MACOS_DIR/macfind"
swift "$ROOT/scripts/make-icon.swift" "$CONTENTS/Resources/AppIcon.icns"

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
	<key>CFBundleIconFile</key><string>AppIcon</string>
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
echo "Bundle prêt : $APP ($(du -sh "$APP" | awk '{print $1}'), $(lipo -archs "$MACOS_DIR/macfind"))"

if [ "$DMG" = "1" ]; then
    echo "== Image disque"
    IMAGE="$ROOT/dist/macfind-$VERSION.dmg"
    PREPARATION="$(mktemp -d)"
    cp -R "$APP" "$PREPARATION/"
    # Raccourci vers Applications : l'installation se fait par glisser-déposer.
    ln -s /Applications "$PREPARATION/Applications"
    rm -f "$IMAGE"
    hdiutil create -volname "macfind $VERSION" -srcfolder "$PREPARATION" \
        -format UDZO -quiet "$IMAGE"
    rm -rf "$PREPARATION"
    shasum -a 256 "$IMAGE" | sed "s|$ROOT/dist/||" > "$IMAGE.sha256"
    echo "Image prête : $IMAGE ($(du -sh "$IMAGE" | awk '{print $1}'))"
fi

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

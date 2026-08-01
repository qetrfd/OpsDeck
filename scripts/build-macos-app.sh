#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

APP_NAME="OpsDeck"
BUNDLE_ID="com.opsdeck.desktop"

DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"

CONTENTS_DIR="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"
CLI_DIR="$RESOURCES_DIR/bin"

VERSION="$(
    awk '
        BEGIN {
            in_package = 0
        }

        /^\[package\]/ {
            in_package = 1
            next
        }

        /^\[/ {
            in_package = 0
        }

        in_package && /^version[[:space:]]*=/ {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' Cargo.toml
)"

if [[ -z "$VERSION" ]]; then
    echo "No se pudo leer la versión desde Cargo.toml" >&2
    exit 1
fi

echo
echo "Compilando OpsDeck $VERSION en modo release..."
echo

cargo build --release --bins

GUI_BINARY="$ROOT_DIR/target/release/opsdeck-desktop"
CLI_BINARY="$ROOT_DIR/target/release/opsdeck"

if [[ ! -x "$GUI_BINARY" ]]; then
    echo "No se encontró el binario gráfico:"
    echo "  $GUI_BINARY"
    exit 1
fi

if [[ ! -x "$CLI_BINARY" ]]; then
    echo "No se encontró el binario CLI:"
    echo "  $CLI_BINARY"
    exit 1
fi

rm -rf "$APP_BUNDLE"

mkdir -p "$MACOS_DIR"
mkdir -p "$CLI_DIR"

cp "$GUI_BINARY" "$MACOS_DIR/$APP_NAME"
cp "$CLI_BINARY" "$CLI_DIR/opsdeck"

chmod +x "$MACOS_DIR/$APP_NAME"
chmod +x "$CLI_DIR/opsdeck"

ICON_XML=""

if [[ -f "$ROOT_DIR/assets/OpsDeck.icns" ]]; then
    cp \
        "$ROOT_DIR/assets/OpsDeck.icns" \
        "$RESOURCES_DIR/OpsDeck.icns"

    ICON_XML=$'    <key>CFBundleIconFile</key>\n    <string>OpsDeck.icns</string>'
fi

cat > "$CONTENTS_DIR/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "https://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>es</string>

    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>

    <key>CFBundleExecutable</key>
    <string>$APP_NAME</string>

    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>

    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>

    <key>CFBundleName</key>
    <string>$APP_NAME</string>

    <key>CFBundlePackageType</key>
    <string>APPL</string>

    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>

    <key>CFBundleVersion</key>
    <string>$VERSION</string>

    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>

    <key>NSHighResolutionCapable</key>
    <true/>

    <key>NSHumanReadableCopyright</key>
    <string>OpsDeck</string>

$ICON_XML
</dict>
</plist>
EOF

printf 'APPL????' > "$CONTENTS_DIR/PkgInfo"

if command -v plutil >/dev/null 2>&1; then
    plutil -lint "$CONTENTS_DIR/Info.plist"
fi

if command -v xattr >/dev/null 2>&1; then
    xattr -cr "$APP_BUNDLE" || true
fi

if command -v codesign >/dev/null 2>&1; then
    echo
    echo "Aplicando firma local..."
    codesign \
        --force \
        --deep \
        --sign - \
        "$APP_BUNDLE"
fi

ZIP_PATH="$DIST_DIR/OpsDeck-$VERSION-macos.zip"

rm -f "$ZIP_PATH"

if command -v ditto >/dev/null 2>&1; then
    ditto \
        -c \
        -k \
        --sequesterRsrc \
        --keepParent \
        "$APP_BUNDLE" \
        "$ZIP_PATH"
else
    (
        cd "$DIST_DIR"

        zip \
            -qry \
            "$(basename "$ZIP_PATH")" \
            "$APP_NAME.app"
    )
fi

echo
echo "Aplicación creada:"
echo "  $APP_BUNDLE"

echo
echo "Archivo para distribución:"
echo "  $ZIP_PATH"

echo
echo "CLI incluida dentro de la aplicación:"
echo "  $CLI_DIR/opsdeck"

echo
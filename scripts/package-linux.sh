#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(
    cd "$(dirname "${BASH_SOURCE[0]}")/.."
    pwd
)"

cd "$ROOT_DIR"

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
    echo "No se pudo obtener la versión desde Cargo.toml."
    exit 1
fi

MACHINE_ARCH="$(uname -m)"

case "$MACHINE_ARCH" in
    x86_64)
        DEB_ARCH="amd64"
        PACKAGE_ARCH="x86_64"
        ;;

    aarch64 | arm64)
        DEB_ARCH="arm64"
        PACKAGE_ARCH="aarch64"
        ;;

    *)
        echo "Arquitectura no compatible: $MACHINE_ARCH"
        exit 1
        ;;
esac

DIST_DIR="$ROOT_DIR/dist-native"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
RELEASE_DIR="$TARGET_DIR/release"

CLI_BINARY="$RELEASE_DIR/opsdeck"
DESKTOP_BINARY="$RELEASE_DIR/opsdeck-desktop"

DEB_NAME="opsdeck_${VERSION}_${DEB_ARCH}"
DEB_ROOT="$DIST_DIR/$DEB_NAME"
DEB_FILE="$DIST_DIR/${DEB_NAME}.deb"

ARCHIVE_NAME="OpsDeck-${VERSION}-linux-${PACKAGE_ARCH}"
ARCHIVE_ROOT="$DIST_DIR/$ARCHIVE_NAME"
ARCHIVE_FILE="$DIST_DIR/${ARCHIVE_NAME}.tar.gz"

echo
echo "Compilando OpsDeck $VERSION para Linux..."
echo

cargo build \
    --locked \
    --release \
    --bins

if [[ ! -x "$CLI_BINARY" ]]; then
    echo "No se encontró:"
    echo "  $CLI_BINARY"
    exit 1
fi

if [[ ! -x "$DESKTOP_BINARY" ]]; then
    echo "No se encontró:"
    echo "  $DESKTOP_BINARY"
    exit 1
fi

rm -rf "$DEB_ROOT"
rm -rf "$ARCHIVE_ROOT"
rm -f "$DEB_FILE"
rm -f "$ARCHIVE_FILE"

mkdir -p "$DIST_DIR"

mkdir -p "$DEB_ROOT/DEBIAN"
mkdir -p "$DEB_ROOT/usr/bin"
mkdir -p "$DEB_ROOT/usr/share/applications"
mkdir -p "$DEB_ROOT/usr/share/doc/opsdeck"

install \
    -m 0755 \
    "$CLI_BINARY" \
    "$DEB_ROOT/usr/bin/opsdeck"

install \
    -m 0755 \
    "$DESKTOP_BINARY" \
    "$DEB_ROOT/usr/bin/opsdeck-desktop"

install \
    -m 0644 \
    "$ROOT_DIR/packaging/linux/opsdeck.desktop" \
    "$DEB_ROOT/usr/share/applications/opsdeck.desktop"

if [[ -f "$ROOT_DIR/README.md" ]]; then
    install \
        -m 0644 \
        "$ROOT_DIR/README.md" \
        "$DEB_ROOT/usr/share/doc/opsdeck/README.md"
fi

cat > "$DEB_ROOT/DEBIAN/control" <<EOF
Package: opsdeck
Version: $VERSION
Section: devel
Priority: optional
Architecture: $DEB_ARCH
Maintainer: OpsDeck Project <noreply@opsdeck.local>
Depends: git, ca-certificates, libgtk-3-0 | libgtk-3-0t64, libxkbcommon0
Description: Centro de control local para proyectos y deploys
 OpsDeck supervisa repositorios Git, ejecuta health checks,
 detecta riesgos y determina si un proyecto está listo para deploy.
EOF

cat > "$DEB_ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh

set -e

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database \
        /usr/share/applications \
        >/dev/null 2>&1 || true
fi

exit 0
EOF

cat > "$DEB_ROOT/DEBIAN/postrm" <<'EOF'
#!/bin/sh

set -e

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database \
        /usr/share/applications \
        >/dev/null 2>&1 || true
fi

exit 0
EOF

chmod 0755 "$DEB_ROOT/DEBIAN/postinst"
chmod 0755 "$DEB_ROOT/DEBIAN/postrm"

dpkg-deb \
    --build \
    --root-owner-group \
    "$DEB_ROOT" \
    "$DEB_FILE"

mkdir -p "$ARCHIVE_ROOT/bin"

install \
    -m 0755 \
    "$CLI_BINARY" \
    "$ARCHIVE_ROOT/bin/opsdeck"

install \
    -m 0755 \
    "$DESKTOP_BINARY" \
    "$ARCHIVE_ROOT/bin/opsdeck-desktop"

install \
    -m 0755 \
    "$ROOT_DIR/packaging/linux/install.sh" \
    "$ARCHIVE_ROOT/install.sh"

install \
    -m 0755 \
    "$ROOT_DIR/packaging/linux/uninstall.sh" \
    "$ARCHIVE_ROOT/uninstall.sh"

install \
    -m 0644 \
    "$ROOT_DIR/packaging/linux/opsdeck.desktop" \
    "$ARCHIVE_ROOT/opsdeck.desktop"

if [[ -f "$ROOT_DIR/README.md" ]]; then
    install \
        -m 0644 \
        "$ROOT_DIR/README.md" \
        "$ARCHIVE_ROOT/README.md"
fi

cat > "$ARCHIVE_ROOT/INSTALACION.txt" <<'EOF'
OPSDECK PARA LINUX

Instalación local:

  ./install.sh

La aplicación se instalará en:

  ~/.local/bin/opsdeck
  ~/.local/bin/opsdeck-desktop

Para desinstalar:

  ./uninstall.sh

Para desinstalar y eliminar los datos:

  ./uninstall.sh --purge
EOF

tar \
    -C "$DIST_DIR" \
    -czf "$ARCHIVE_FILE" \
    "$ARCHIVE_NAME"

rm -rf "$DEB_ROOT"
rm -rf "$ARCHIVE_ROOT"

echo
echo "Paquetes Linux generados:"
echo
echo "  $DEB_FILE"
echo "  $ARCHIVE_FILE"
echo
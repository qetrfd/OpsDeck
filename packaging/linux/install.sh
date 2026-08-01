#!/usr/bin/env bash

set -euo pipefail

PACKAGE_DIR="$(
    cd "$(dirname "${BASH_SOURCE[0]}")"
    pwd
)"

PREFIX="${OPSDECK_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
APPLICATIONS_DIR="$HOME/.local/share/applications"

CLI_SOURCE="$PACKAGE_DIR/bin/opsdeck"
DESKTOP_SOURCE="$PACKAGE_DIR/bin/opsdeck-desktop"
DESKTOP_FILE_SOURCE="$PACKAGE_DIR/opsdeck.desktop"

if [[ ! -x "$CLI_SOURCE" ]]; then
    echo "No se encontró el ejecutable CLI:"
    echo "  $CLI_SOURCE"
    exit 1
fi

if [[ ! -x "$DESKTOP_SOURCE" ]]; then
    echo "No se encontró la aplicación gráfica:"
    echo "  $DESKTOP_SOURCE"
    exit 1
fi

mkdir -p "$BIN_DIR"
mkdir -p "$APPLICATIONS_DIR"

install \
    -m 0755 \
    "$CLI_SOURCE" \
    "$BIN_DIR/opsdeck"

install \
    -m 0755 \
    "$DESKTOP_SOURCE" \
    "$BIN_DIR/opsdeck-desktop"

sed \
    -e "s|^Exec=.*|Exec=$BIN_DIR/opsdeck-desktop|" \
    -e "s|^TryExec=.*|TryExec=$BIN_DIR/opsdeck-desktop|" \
    "$DESKTOP_FILE_SOURCE" \
    > "$APPLICATIONS_DIR/opsdeck.desktop"

chmod 0644 \
    "$APPLICATIONS_DIR/opsdeck.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database \
        "$APPLICATIONS_DIR" \
        >/dev/null 2>&1 || true
fi

echo
echo "OpsDeck fue instalado correctamente."
echo
echo "Aplicación gráfica:"
echo "  $BIN_DIR/opsdeck-desktop"
echo
echo "CLI:"
echo "  $BIN_DIR/opsdeck"
echo
echo "Acceso del menú:"
echo "  $APPLICATIONS_DIR/opsdeck.desktop"
echo

case ":$PATH:" in
    *":$BIN_DIR:"*)
        echo "La CLI ya está disponible en PATH."
        ;;

    *)
        echo "Agrega esta línea a ~/.bashrc o ~/.zshrc:"
        echo
        echo "  export PATH=\"$BIN_DIR:\$PATH\""
        ;;
esac

echo
echo "Para iniciar OpsDeck ahora:"
echo
echo "  $BIN_DIR/opsdeck-desktop"
echo
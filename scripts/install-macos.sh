#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BUILD_SCRIPT="$ROOT_DIR/scripts/build-macos-app.sh"
SOURCE_APP="$ROOT_DIR/dist/OpsDeck.app"

APP_INSTALL_DIR="${OPSDECK_APP_DIR:-$HOME/Applications}"
CLI_INSTALL_DIR="${OPSDECK_BIN_DIR:-$HOME/.local/bin}"

INSTALLED_APP="$APP_INSTALL_DIR/OpsDeck.app"
INSTALLED_CLI="$CLI_INSTALL_DIR/opsdeck"

if [[ ! -d "$SOURCE_APP" ]]; then
    echo "No existe una compilación previa."
    echo "Construyendo OpsDeck..."

    "$BUILD_SCRIPT"
fi

mkdir -p "$APP_INSTALL_DIR"
mkdir -p "$CLI_INSTALL_DIR"

rm -rf "$INSTALLED_APP"

cp -R \
    "$SOURCE_APP" \
    "$INSTALLED_APP"

cp \
    "$SOURCE_APP/Contents/Resources/bin/opsdeck" \
    "$INSTALLED_CLI"

chmod +x "$INSTALLED_CLI"

if command -v xattr >/dev/null 2>&1; then
    xattr -cr "$INSTALLED_APP" || true
fi

echo
echo "OpsDeck fue instalado correctamente."

echo
echo "Aplicación:"
echo "  $INSTALLED_APP"

echo
echo "CLI:"
echo "  $INSTALLED_CLI"

echo

case ":${PATH}:" in
    *":$CLI_INSTALL_DIR:"*)
        echo "La CLI ya está disponible en PATH."
        ;;

    *)
        echo "Agrega esta línea a ~/.zshrc:"
        echo
        echo "  export PATH=\"$CLI_INSTALL_DIR:\$PATH\""
        echo
        echo "Después ejecuta:"
        echo
        echo "  source ~/.zshrc"
        ;;
esac

echo
echo "Para abrir la aplicación:"
echo
echo "  open \"$INSTALLED_APP\""
echo
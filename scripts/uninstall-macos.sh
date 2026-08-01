#!/usr/bin/env bash
set -euo pipefail

APP_INSTALL_DIR="${OPSDECK_APP_DIR:-$HOME/Applications}"
CLI_INSTALL_DIR="${OPSDECK_BIN_DIR:-$HOME/.local/bin}"

INSTALLED_APP="$APP_INSTALL_DIR/OpsDeck.app"
INSTALLED_CLI="$CLI_INSTALL_DIR/opsdeck"

PURGE_DATA=false

if [[ "${1:-}" == "--purge" ]]; then
    PURGE_DATA=true
elif [[ -n "${1:-}" ]]; then
    echo "Uso:"
    echo "  $0"
    echo "  $0 --purge"
    exit 1
fi

rm -rf "$INSTALLED_APP"
rm -f "$INSTALLED_CLI"

if [[ "$PURGE_DATA" == true ]]; then
    rm -rf "$HOME/.opsdeck"
fi

echo
echo "OpsDeck fue desinstalado."

echo
echo "Aplicación eliminada:"
echo "  $INSTALLED_APP"

echo
echo "CLI eliminada:"
echo "  $INSTALLED_CLI"

echo

if [[ "$PURGE_DATA" == true ]]; then
    echo "También se eliminaron los datos locales:"
    echo "  $HOME/.opsdeck"
else
    echo "Los proyectos, políticas e historial se conservaron en:"
    echo "  $HOME/.opsdeck"
fi

echo
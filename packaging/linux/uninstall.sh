#!/usr/bin/env bash

set -euo pipefail

PREFIX="${OPSDECK_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
APPLICATIONS_DIR="$HOME/.local/share/applications"
DATA_DIR="${OPSDECK_HOME:-$HOME/.opsdeck}"

PURGE_DATA=false

if [[ "${1:-}" == "--purge" ]]; then
    PURGE_DATA=true
elif [[ -n "${1:-}" ]]; then
    echo "Uso:"
    echo "  ./uninstall.sh"
    echo "  ./uninstall.sh --purge"
    exit 1
fi

rm -f "$BIN_DIR/opsdeck"
rm -f "$BIN_DIR/opsdeck-desktop"
rm -f "$APPLICATIONS_DIR/opsdeck.desktop"

if [[ "$PURGE_DATA" == true ]]; then
    rm -rf "$DATA_DIR"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database \
        "$APPLICATIONS_DIR" \
        >/dev/null 2>&1 || true
fi

echo
echo "OpsDeck fue desinstalado."
echo

if [[ "$PURGE_DATA" == true ]]; then
    echo "También se eliminaron los datos:"
    echo "  $DATA_DIR"
else
    echo "Los proyectos, políticas e historial se conservaron:"
    echo "  $DATA_DIR"
fi

echo
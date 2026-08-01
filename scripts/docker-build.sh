#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(
    cd "$(dirname "${BASH_SOURCE[0]}")/.."
    pwd
)"

IMAGE_NAME="${OPSDECK_IMAGE:-opsdeck:1.0.0}"

cd "$ROOT_DIR"

if ! command -v docker >/dev/null 2>&1; then
    echo "Docker no está instalado o no está disponible en PATH."
    exit 1
fi

echo
echo "Construyendo la imagen Docker de OpsDeck..."
echo "Imagen: $IMAGE_NAME"
echo

docker build \
    --pull \
    --tag "$IMAGE_NAME" \
    "$ROOT_DIR"

echo
echo "Imagen creada correctamente:"
echo "  $IMAGE_NAME"
echo
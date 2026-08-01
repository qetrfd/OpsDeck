#!/usr/bin/env bash

set -euo pipefail

IMAGE_NAME="${OPSDECK_IMAGE:-opsdeck:1.0.0}"
DATA_VOLUME="${OPSDECK_DATA_VOLUME:-opsdeck-data}"

print_usage() {
    echo
    echo "Uso:"
    echo
    echo "  ./scripts/docker-run.sh <ruta-proyecto>"
    echo
    echo "  ./scripts/docker-run.sh <ruta-proyecto> <comando> [argumentos]"
    echo
    echo "Ejemplos:"
    echo
    echo "  ./scripts/docker-run.sh /ruta/proyecto"
    echo "  ./scripts/docker-run.sh /ruta/proyecto status /workspace"
    echo "  ./scripts/docker-run.sh /ruta/proyecto diagnose /workspace"
    echo "  ./scripts/docker-run.sh /ruta/proyecto checklist /workspace"
    echo "  ./scripts/docker-run.sh /ruta/proyecto gate /workspace"
    echo
}

if ! command -v docker >/dev/null 2>&1; then
    echo "Docker no está instalado o no está disponible en PATH."
    exit 1
fi

if [ "$#" -lt 1 ]; then
    print_usage
    exit 1
fi

PROJECT_PATH="$1"
shift

if [ ! -d "$PROJECT_PATH" ]; then
    echo "La carpeta no existe:"
    echo "  $PROJECT_PATH"
    exit 1
fi

PROJECT_PATH="$(
    cd "$PROJECT_PATH"
    pwd
)"

if [ "$#" -eq 0 ]; then
    set -- status /workspace
fi

TTY_ARGUMENTS=()

if [ -t 0 ] && [ -t 1 ]; then
    TTY_ARGUMENTS=(-it)
fi

echo
echo "Ejecutando OpsDeck con Docker"
echo "Proyecto: $PROJECT_PATH"
echo "Imagen:   $IMAGE_NAME"
echo

docker run \
    --rm \
    "${TTY_ARGUMENTS[@]}" \
    --add-host \
    "host.docker.internal:host-gateway" \
    --env \
    "HOME=/data" \
    --env \
    "OPSDECK_CONTAINER=1" \
    --mount \
    "type=bind,source=$PROJECT_PATH,target=/workspace" \
    --mount \
    "type=volume,source=$DATA_VOLUME,target=/data/.opsdeck" \
    "$IMAGE_NAME" \
    "$@"
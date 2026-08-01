#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo
echo "1/6 Verificando formato..."
cargo fmt --check

echo
echo "2/6 Verificando compilación..."
cargo check --all-targets

echo
echo "3/6 Ejecutando Clippy..."
cargo clippy --all-targets -- -D warnings

echo
echo "4/6 Ejecutando pruebas..."
cargo test --all-targets

echo
echo "5/6 Compilando binarios release..."
cargo build --release --bins

echo
echo "6/6 Empaquetando aplicación macOS..."
"$ROOT_DIR/scripts/build-macos-app.sh"

echo
echo "OpsDeck superó la validación de release."
echo
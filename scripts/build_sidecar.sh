#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON_BIN="${PYTHON_BIN:-$ROOT_DIR/../.offshoredev/bin/python}"
OUT_DIR="$ROOT_DIR/src-tauri/resources/sidecar"
BUILD_TMP="$ROOT_DIR/sidecar/.build_tmp"

if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "Python interpreter not found at: $PYTHON_BIN" >&2
  echo "Set PYTHON_BIN=/path/to/python (with offshore-podcast and nuitka installed)." >&2
  exit 1
fi

echo "Building offshore sidecar with Nuitka using $PYTHON_BIN ..."
rm -rf "$BUILD_TMP" "$OUT_DIR"
mkdir -p "$BUILD_TMP" "$(dirname "$OUT_DIR")"

"$PYTHON_BIN" -m nuitka \
  --standalone \
  --assume-yes-for-downloads \
  --include-package=offshore \
  --include-package-data=offshore \
  --static-libpython=no \
  --output-dir="$BUILD_TMP" \
  --output-filename=offshore-sidecar \
  "$ROOT_DIR/sidecar/offshore_sidecar.py"

mv "$BUILD_TMP/offshore_sidecar.dist" "$OUT_DIR"
touch "$OUT_DIR/.keep"
rm -rf "$BUILD_TMP"

echo "Sidecar built at: $OUT_DIR/offshore-sidecar"

#!/usr/bin/env bash
# Build both Web variants into apps/web/dist:
#   webgpu.js / webgpu_bg.wasm  - WebGPU backend (preferred)
#   webgl2.js / webgl2_bg.wasm  - WebGL2 backend (fallback)
# index.html picks one at runtime.
set -euo pipefail
cd "$(dirname "$0")/../.."

out=apps/web/dist
wasm=target/wasm32-unknown-unknown/wasm-release/web.wasm
mkdir -p "$out"

for backend in webgpu webgl2; do
  cargo build -p web --target wasm32-unknown-unknown --profile wasm-release --features "$backend"
  wasm-bindgen --target web --no-typescript --out-dir "$out" --out-name "$backend" "$wasm"
  # Shrink further when Binaryen is installed; the build works without it.
  if command -v wasm-opt >/dev/null; then
    wasm-opt -Os --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
      --enable-mutable-globals --enable-reference-types --enable-multivalue \
      -o "$out/${backend}_bg.wasm" "$out/${backend}_bg.wasm"
  fi
done
cp apps/web/index.html "$out/"
mkdir -p "$out/fonts"
cp apps/web/fonts/* "$out/fonts/"
ls -l "$out"

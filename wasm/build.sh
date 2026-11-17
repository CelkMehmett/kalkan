#!/usr/bin/env bash
# kalkan.wasm'ı derler ve tek dosyalık demo sayfasını (dist/index.html) üretir.
# Gereken: rustup, python3. İsteğe bağlı: wasm-opt (binaryen) ile küçültme.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null
cargo build --release --target wasm32-unknown-unknown --features ffi --lib
cp target/wasm32-unknown-unknown/release/kalkan.wasm wasm/kalkan.wasm

if command -v wasm-opt >/dev/null; then
  wasm-opt -Oz --strip-debug wasm/kalkan.wasm -o wasm/kalkan.wasm
fi

python3 wasm/inline.py
echo "Hazır: wasm/kalkan.wasm ($(wc -c < wasm/kalkan.wasm) bayt), wasm/dist/index.html"
echo "Yerelde denemek için: python3 -m http.server -d wasm/dist 8000"

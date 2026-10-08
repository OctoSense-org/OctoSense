#!/usr/bin/env bash
# Builds Wasm Lab's functions into the app bundle (bundle/fns/wasmlab.wasm).
# Needs the target once: rustup target add wasm32-unknown-unknown
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cargo build --manifest-path "$here/Cargo.toml" --release --target wasm32-unknown-unknown -p wasmlab-functions
mkdir -p "$here/../bundle/fns"
cp "$here/target/wasm32-unknown-unknown/release/wasmlab_functions.wasm" "$here/../bundle/fns/wasmlab.wasm"
echo "bundle/fns/wasmlab.wasm: $(wc -c < "$here/../bundle/fns/wasmlab.wasm") bytes"

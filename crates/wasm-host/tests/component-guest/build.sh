#!/usr/bin/env bash
# Rebuilds the test components into ../fixtures with plain cargo.
# Needs: rustup target add wasm32-wasip2
set -euo pipefail
cd "$(dirname "$0")"
cargo build --locked --target wasm32-wasip2 --release
mkdir -p ../fixtures
for c in notes netprobe; do
  cp "target/wasm32-wasip2/release/$c.wasm" "../fixtures/$c.component.wasm"
done
ls -l ../fixtures/*.component.wasm

#!/bin/sh
# Build ordinary Home or the ROM's Home/Bridge pair from this repository.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec python3 "$ROOT/scripts/build-home.py" "$@"

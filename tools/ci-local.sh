#!/usr/bin/env bash
# Run the GitHub workflows' checks locally (tools/ci_local.py; docs/local-ci.md):
#
#   tools/ci-local.sh [--only desktop|phone|apps|rom|all] [--jobs N] [--keep-going] [--no-wait]
#   tools/ci-local.sh --linux-host ...   # ubuntu jobs on the Linux build host (~/.config/octosense/build.env)
#
# Prints PASS/FAIL/SKIPPED per step, writes target/ci-local/<timestamp>.log
# and target/ci-local/last.json, and exits non-zero on any failure.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# rustup's cargo when the shell that called us does not have it.
if ! command -v cargo >/dev/null 2>&1 && [ -x "$HOME/.cargo/bin/cargo" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
exec python3 "$here/ci_local.py" "$@"

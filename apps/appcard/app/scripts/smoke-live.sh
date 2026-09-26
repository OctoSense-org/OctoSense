#!/usr/bin/env bash
# scripts/smoke-live.sh — drive `tests/live_smoke.rs` against a running Octos
# UI Protocol server. The test is `#[ignore]`d, so we have to pass `--ignored`
# explicitly.
#
# Quick start:
#   1. Start an Octos server somewhere (see ../RUNNING.md § 2 for the recipe).
#   2. Export the bearer token in a way that does NOT touch shell history. Two
#      good options:
#
#        # (a) fish/bash history-skip via leading space (HISTCONTROL=ignorespace
#        #     or set -gx if you're on fish):
#         export OCTOS_LIVE_TOKEN='paste-token-here'
#
#        # (b) prompt for it without echoing:
#        read -s -p "OCTOS_LIVE_TOKEN: " OCTOS_LIVE_TOKEN; echo; export OCTOS_LIVE_TOKEN
#
#        # (c) read from a 0600 file you keep outside the repo:
#        export OCTOS_LIVE_TOKEN="$(cat ~/.config/octos-app/live-token)"
#
#   3. (optional) Override the URL/profile if not the defaults below:
#        export OCTOS_LIVE_URL='http://127.0.0.1:56831'
#        export OCTOS_LIVE_PROFILE='admin'
#
#   4. Run this script. It just forwards to `cargo test --ignored`.
#
# DO NOT commit the token. DO NOT inline it in this script. Treat any history
# entry that contains the literal token as compromised — rotate it.

set -euo pipefail

# Sensible defaults. Override by exporting before invoking the script.
: "${OCTOS_LIVE_URL:=http://127.0.0.1:56831}"
: "${OCTOS_LIVE_PROFILE:=admin}"

if [[ -z "${OCTOS_LIVE_TOKEN:-}" ]]; then
    cat >&2 <<'MSG'
error: OCTOS_LIVE_TOKEN not set.
       The test will skip (eprintln) without this. See the comment block at
       the top of scripts/smoke-live.sh for safe ways to export it.
MSG
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

export OCTOS_LIVE_URL OCTOS_LIVE_PROFILE OCTOS_LIVE_TOKEN

echo "smoke-live: URL=$OCTOS_LIVE_URL profile=$OCTOS_LIVE_PROFILE (token redacted)"
exec cargo test -p octos-app-transport --test live_smoke -- \
    --ignored --nocapture --test-threads=1

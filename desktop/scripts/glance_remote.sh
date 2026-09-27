#!/usr/bin/env bash
# The glance screen (OctoSense #63) on the desktop, driven through Makepad's
# remote (instrument) mode: no OS screenshots, only the app's own frames.
#
#   cargo build [--release] -p octosense && desktop/scripts/glance_remote.sh [artifacts-dir]
#
# Two hidden runs, each with its own OCTOSENSE_HOME, octos core dir and file
# vaults under the artifacts directory, and OCTOSENSE_GLANCE_DEMO=1 (the
# shell publishes a sample L0 News digest as `os.news` through the glance
# service at startup):
#
# 1. desktop: `--test-action glance` opens the glance panel (F9); the digest
#    card is laid out in it; a click on the card opens News.
# 2. phone layout: `--test-action page:-1` switches to the iOS shell on its
#    glance page; the digest card leads the feed; a tap opens News.
#
# Grabs are kept as evidence (desktop-glance.png, phone-glance.png).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${OCTOSENSE_BIN:-$ROOT/../target/release/octosense}
[ -x "$BIN" ] || BIN=$ROOT/../target/debug/octosense
WORK=${1:-$(mktemp -d -t octosense-glance)}
mkdir -p "$WORK/grabs"
echo "artifacts: $WORK"
echo "binary: $BIN"

PID=
PORT=
LOG=
cleanup() {
    if [ -n "$PID" ] && kill -0 "$PID" 2>/dev/null; then
        [ -n "$PORT" ] && curl -s "127.0.0.1:$PORT/quit" >/dev/null || true
        sleep 2
        kill "$PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

fail() { echo "FAIL: $*"; exit 1; }
pass() { echo "PASS: $*"; }

launch() {
    local name=$1; shift
    cleanup
    mkdir -p "$WORK/$name/home" "$WORK/$name/octos"
    LOG=$WORK/$name/host.log
    (cd "$WORK/$name" && env -u MAKEPAD_HOME -u MAKEPAD_WM_ROOT -u MAKEPAD_WM_THEME \
        OCTOSENSE_HOME="$WORK/$name/home" OCTOS_APP_CORE_DIR="$WORK/$name/octos" \
        OCTOSENSE_MAIL_VAULT=file OCTOSENSE_LLM_VAULT=file OCTOSENSE_GLANCE_DEMO=1 \
        MAKEPAD_HIDE_WINDOWS=1 "$BIN" --remote "$@" >"$LOG" 2>&1) &
    PID=$!
    PORT=
    for _ in $(seq 1 120); do
        PORT=$(sed -n 's/.*\[makepad-remote\] listening on 127\.0\.0\.1:\([0-9]*\) pid=.*/\1/p' "$LOG" | head -1)
        [ -n "$PORT" ] && break
        sleep 0.5
    done
    [ -n "$PORT" ] || fail "no remote port in $LOG"
}
wait_log() {
    for _ in $(seq 1 ${2:-40}); do grep -q "$1" "$LOG" && return 0; sleep 0.5; done
    fail "log never said: $1"
}
grab() {
    local png
    for _ in 1 2 3 4 5; do
        # A hidden window's grab can miss its frame: ask again.
        png=$(curl -s "127.0.0.1:$PORT/g?scale=0.5" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("png",""))' || true)
        [ -n "$png" ] && { cp "$png" "$WORK/grabs/$1.png"; return; }
        sleep 1
    done
    fail "no frame for $1"
}
click() { curl -s "127.0.0.1:$PORT/click?x=$1&y=$2&wait=1" >/dev/null || true; }

# 1. The desktop's glance panel.
launch desktop --test-action glance
wait_log "glance: os.news published digest"
pass "the demo digest was admitted (L0 check, caps) and published as os.news"
wait_log "wm: glance panel open"
wait_log "glance panel: 1 card(s) news@"
sleep 2
grab desktop-glance
# The card's rect, as the panel last laid it out (after measuring it).
RECT=$(grep "glance panel: 1 card(s) news@" "$LOG" | tail -1 | sed 's/.*news@//')
IFS=, read -r X Y W H <<<"$RECT"
[ "$H" -gt 72 ] && [ "$H" -le 260 ] || fail "tile height $H outside the tile range"
pass "the digest card is laid out in the glance panel at ${W}x${H}"
click $((X + W / 2)) $((Y + H / 2))
wait_log "wm: glance card opens news"
wait_log "wm: launched news"
pass "clicking the card opens News"
grep -q "panicked" "$LOG" && fail "panic in $LOG"

# 2. The phone layout's glance page.
launch phone --test-action page:-1
wait_log "glance: os.news published digest"
wait_log "wm: --test-action page -1"
sleep 4
grab phone-glance
# The glance column's first tile: under the 64-point header, 20 in.
click 200 230
wait_log "\[phone\] glance card opens news"
wait_log "wm: launched news"
pass "the phone glance page shows the digest first and a tap opens News"
grep -q "panicked" "$LOG" && fail "panic in $LOG"
echo "grabs: $WORK/grabs"

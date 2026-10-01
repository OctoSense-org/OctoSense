#!/usr/bin/env bash
# The email action card MVP on the desktop (apps/mail/docs/2026-10-01-email-
# action-card-plan.md), driven through Makepad's remote bridge in hidden
# windows: fake data, no mail read, no model called.
#
#   cargo build -p octosense && desktop/scripts/mail_card_remote.sh [frames-dir]
#
# OCTOSENSE_GLANCE_DEMO=mail publishes two L0 cards as `os.mail`, each with a
# toast. Two runs:
#
# 1. the toasts; the Ana Lee toast opens THAT card in the card window
#    (glance_sheet.rs), sized to the card, the agent's summary and
#    suggestion marked AI-written; Reply writes the agent's draft into a
#    multi-line field (marked); Send shows "Sent (demo)";
#    ✕ closes it; the UPS toast opens the shipping card; Track; Esc closes.
# 2. the Ana Lee card's Ask, an in-card chat with Mail's agent (`sys.chat`,
#    glance_chat.rs): the host's transcript; a typed question sent with the
#    arrow is recorded as the person's entry and the host appends the
#    agent's reply (the canned demo answer), marked AI-written.
#
# Every step is checked in the log (the card window logs each tap it
# carries out) and grabbed to <frames-dir>/NN-name.png. Each run ends with
# the bridge's /quit. Env: MAIL_CARD_PORT (default 18431), OCTOSENSE_HOME
# and MAKEPAD_HOME (default: under the frames dir; both are wiped).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${OCTOSENSE_BIN:-$ROOT/../target/debug/octosense}
OUT=${1:-$(mktemp -d -t octosense-mail-card)}
PORT=${MAIL_CARD_PORT:-18431}
HOME_DIR=${OCTOSENSE_HOME:-$OUT/home}
MP_HOME=${MAKEPAD_HOME:-$OUT/makepad-home}
mkdir -p "$OUT"
LOG=
echo "frames: $OUT"
echo "binary: $BIN"

quit() { curl -s "127.0.0.1:$PORT/quit" >/dev/null 2>&1 || true; sleep 2; }
trap quit EXIT
fail() { echo "FAIL: $*"; exit 1; }
pass() { echo "PASS: $*"; }

launch() {
    local name=$1
    LOG=$OUT/$name.log
    rm -rf "$HOME_DIR" "$MP_HOME"
    mkdir -p "$HOME_DIR" "$MP_HOME"
    (cd "$OUT" && env -u MAKEPAD_WM_ROOT -u MAKEPAD_WM_THEME \
        OCTOSENSE_HOME="$HOME_DIR" MAKEPAD_HOME="$MP_HOME" OCTOS_APP_CORE_DIR="$HOME_DIR/octos" \
        OCTOSENSE_MAIL_VAULT=file OCTOSENSE_LLM_VAULT=file OCTOSENSE_GLANCE_DEMO=mail \
        MAKEPAD_HIDE_WINDOWS=1 MAKEPAD_REMOTE="$PORT" "$BIN" >"$LOG" 2>&1 &)
    wait_log "listening on 127.0.0.1:$PORT" 120
}
wait_log() {
    for _ in $(seq 1 "${2:-30}"); do grep -q -- "$1" "$LOG" && return 0; sleep 0.5; done
    fail "log never said: $1"
}
grab() {
    local png
    for _ in 1 2 3 4 5; do
        png=$(curl -s "127.0.0.1:$PORT/g" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("png",""))' || true)
        # Layout points (the window is grabbed at its DPI).
        [ -n "$png" ] && { sips -Z 1400 "$png" --out "$OUT/$1.png" >/dev/null; echo "  frame $OUT/$1.png"; return; }
        sleep 1
    done
    fail "no frame for $1"
}
click() { curl -s "127.0.0.1:$PORT/click?x=$1&y=$2&wait=1" >/dev/null; sleep 1; }
type_text() { curl -s "127.0.0.1:$PORT/t?t=$(python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.argv[1]))' "$1")&wait=1" >/dev/null; sleep 1; }
key() { curl -s "127.0.0.1:$PORT/k?k=down&c=$1&wait=1" >/dev/null; curl -s "127.0.0.1:$PORT/k?k=up&c=$1&wait=1" >/dev/null; sleep 1; }
# The centre of the toast that opens card $1, from the stack's last layout.
toast_at() {
    local id rect
    id=$(sed -n "s/.*glance: toast \([0-9]*\) opens os.mail\/$1\$/\1/p" "$LOG" | tail -1)
    [ -n "$id" ] || fail "no toast for $1"
    rect=$(grep 'notifications: ' "$LOG" | tail -1 | tr ' ' '\n' | sed -n "s/^$id@//p")
    [ -n "$rect" ] || fail "toast $id is not on screen"
    IFS=, read -r X Y W H <<<"$rect"
    echo "$((X + W / 2)) $((Y + H / 2))"
}
# The card window's origin and its ✕, from its last layout line.
sheet() {
    local line
    line=$(grep "glance sheet: os.mail/$1 sheet@" "$LOG" | tail -1)
    [ -n "$line" ] || fail "the card window never laid out $1"
    IFS=, read -r SX SY _ _ <<<"$(echo "$line" | sed 's/.*sheet@\([0-9,]*\) .*/\1/')"
    IFS=, read -r CX CY <<<"$(echo "$line" | sed 's/.*close@\([0-9,]*\).*/\1/')"
}
# Click at an offset into the card window. The window sizes to its card and
# re-centres when the card changes, so its origin is read again each time
# (the card's layout inside the window is fixed).
at() { sleep 0.5; sheet "$CARD"; click $((SX + $1)) $((SY + $2)); }

# ---------------------------------------------------------------- run 1
launch run1
wait_log "glance: os.mail published ana-contract"
wait_log "glance: os.mail published ups-lamp"
wait_log "notifications: 2 toast(s)"
sleep 1
grab 01-toasts
pass "two fake Mail cards published as os.mail, one toast each"

read -r TX TY <<<"$(toast_at ana-contract)"
click "$TX" "$TY"
wait_log "wm: glance toast opens card os.mail/ana-contract"
wait_log "glance sheet: os.mail/ana-contract sheet@"
sleep 1
grab 02-mail-card
CARD=ana-contract
pass "the toast opened the Ana Lee card in the card window (not the panel)"

at 80 319 # Reply
wait_log "tap reply (applied true, relower true)"
grab 03-mail-reply-draft
at 147 326 # Send
wait_log "tap send (applied true, relower true)"
grab 04-mail-sent
pass "Reply showed the agent's draft, marked; Send moved the card to Sent (demo)"

sheet ana-contract
click "$CX" "$CY" # ✕
wait_log "glance sheet: closed os.mail/ana-contract"
grab 05-mail-closed

read -r TX TY <<<"$(toast_at ups-lamp)"
click "$TX" "$TY"
wait_log "wm: glance toast opens card os.mail/ups-lamp"
wait_log "glance sheet: os.mail/ups-lamp sheet@"
sleep 1
grab 06-shipping-card
CARD=ups-lamp
at 70 372 # Track
wait_log "tap track (applied true, relower true)"
grab 07-shipping-track
key Escape
wait_log "glance sheet: closed os.mail/ups-lamp"
grab 08-shipping-closed
pass "the UPS toast opened the shipping card; Track; Esc closed it"
grep -q "panicked" "$LOG" && fail "panic in $LOG"
quit

# ---------------------------------------------------------------- run 2
launch run2
wait_log "notifications: 2 toast(s)"
sleep 1
read -r TX TY <<<"$(toast_at ana-contract)"
click "$TX" "$TY"
wait_log "glance sheet: os.mail/ana-contract sheet@"
CARD=ana-contract
at 248 319 # Ask
wait_log "tap ask (applied true, relower true)"
grab 09-mail-ask-transcript
at 150 308 # the question field
type_text "When do they need the terms?"
wait_log "tap typing (applied true, relower false)"
grab 10-mail-ask-typed
at 315 308 # ➤
wait_log "tap submit (applied true, relower true)"
wait_log "glance: os.mail chat convo recorded"
sleep 1
grab 11-mail-ask-answered
pass "Ask showed the host's transcript; a sent question became the person's entry and the agent's reply followed"
key Escape
wait_log "glance sheet: closed os.mail/ana-contract"
grep -q "panicked" "$LOG" && fail "panic in $LOG"
echo "frames: $OUT"

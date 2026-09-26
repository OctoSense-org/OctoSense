#!/usr/bin/env bash
# The system apps on the desktop, driven through Makepad's remote
# (instrument) mode: no OS screenshots, only the app's own frames.
#
#   cargo build --release && scripts/system_apps_remote.sh [artifacts-dir]
#
# Runs the release binary hidden, with its own OCTOSENSE_HOME, octos core dir
# and file vaults under a temp directory, so neither ~/.octosense nor the login
# keychain is touched. Checks: the launcher lists every selected system app;
# Photos opens and draws (no network needed); Mail opens and its host-owned
# sign-in sheet appears (nobody signs in); each app runs in an isolate under
# its manifest's policy; bundles unpack under <home>/apps/.system/<os.id>/;
# AI providers (when selected) opens on its empty provider list; the log has
# no panic and no UI hang outside the known ones. Grabs are kept as evidence.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${OCTOSENSE_BIN:-$ROOT/target/release/octosense}
WORK=${1:-$(mktemp -d -t octosense-system-apps)}
mkdir -p "$WORK/home" "$WORK/octos" "$WORK/grabs"
LOG=$WORK/host.log
APPS=$(python3 -c 'import json,sys; print(" ".join(json.load(open(sys.argv[1]))["apps"]))' "$ROOT/system-apps.json")
echo "artifacts: $WORK"
echo "system apps: $APPS"

# From the temp directory: Maps' tile cache is written relative to the
# working directory (./local/tilecache_v4), not under the app's jail.
cd "$WORK"
env -u MAKEPAD_HOME -u MAKEPAD_WM_ROOT -u MAKEPAD_WM_THEME \
    OCTOSENSE_HOME="$WORK/home" OCTOS_APP_CORE_DIR="$WORK/octos" \
    OCTOSENSE_MAIL_VAULT=file OCTOSENSE_LLM_VAULT=file \
    MAKEPAD_HIDE_WINDOWS=1 "$BIN" --remote >"$LOG" 2>&1 &
PID=$!
PORT=
cleanup() {
    if kill -0 "$PID" 2>/dev/null; then
        [ -n "$PORT" ] && curl -s "127.0.0.1:$PORT/quit" >/dev/null || true
        sleep 2
        kill "$PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

fail() { echo "FAIL: $*"; exit 1; }
pass() { echo "PASS: $*"; }
for _ in $(seq 1 120); do
    PORT=$(sed -n 's/.*\[makepad-remote\] listening on 127\.0\.0\.1:\([0-9]*\) pid=.*/\1/p' "$LOG" | head -1)
    [ -n "$PORT" ] && break
    sleep 0.5
done
[ -n "$PORT" ] || fail "no remote port in $LOG"
get() { curl -fsS "127.0.0.1:$PORT/$1"; }
key() { get "k?k=press&c=$1&wait=1${2:-}" >/dev/null; }
grab() {
    local png
    for _ in 1 2 3 4 5; do
        png=$(get "g?scale=0.5" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("png",""))')
        [ -n "$png" ] && { cp "$png" "$WORK/grabs/$1.png"; return; }
        sleep 1
    done
    fail "no frame for $1"
}
# `/snap` texts, one per line, for a query.
snap() { get "snap?q=$(python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.argv[1]))' "$1")" \
    | python3 -c 'import json,sys; [print(e["i"], e["ty"]) for e in json.load(sys.stdin)["s"]]'; }
wait_log() {
    for _ in $(seq 1 ${2:-40}); do grep -q "$1" "$LOG" && return 0; sleep 0.5; done
    fail "log never said: $1"
}
# Type into the Start menu's search (KeyDown characters, as the smoke test does).
type_keys() {
    local text=$1 i c
    for ((i = 0; i < ${#text}; i++)); do
        c=${text:i:1}
        case $c in
            " ") key Space ;;
            -) key Minus ;;
            *) key "Key$(printf %s "$c" | tr a-z A-Z)" ;;
        esac
    done
}
launch() { key Space "&cmd=1"; type_keys "$1"; key Return; }
label_of() { # the manifest name of a selected app
    python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["name"])' \
        "$ROOT/$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["source"])' "$ROOT/system-apps.json")/$1/bundle/manifest.json"
}

for _ in $(seq 1 60); do curl -s "127.0.0.1:$PORT/snap?q=main_window" | grep -q '"s":\[{' && break; sleep 0.5; done
pass "desktop up on port $PORT (pid $PID)"

# 1. The launcher lists each system app (its short id, its manifest name).
for app in $APPS; do
    key Space "&cmd=1"
    type_keys "$app"
    grab "launcher-$app"
    key Escape
done
python3 - "$WORK/grabs" $APPS <<'EOF' || fail "launcher grabs missing"
import sys, pathlib
d = pathlib.Path(sys.argv[1])
assert all((d / f"launcher-{a}.png").stat().st_size > 10000 for a in sys.argv[2:])
EOF
grep -q 'wm: modules linked: .*"card"' "$LOG" || fail "the Card runner is not linked"
pass "launcher searched for: $APPS (grabs launcher-*.png); Card runner linked"

# 2. Photos: opens in the Card runner and draws its library.
launch photos
wait_log "card: os.photos running under"
for _ in $(seq 1 20); do snap "Collections" | grep -q "^card Splash" && break; sleep 0.5; done
snap "Collections" | grep -q "^card Splash" || fail "Photos drew nothing"
grab photos
pass "Photos opened: $(grep -o 'card: os.photos running under.*' "$LOG" | head -1)"

# 3. Mail: opens, and its sign-in is the host's sheet, not the app's.
launch mail
wait_log "card: os.mail running under"
sleep 2
grab mail-welcome
snap "Add account" | grep -q "^card Splash" || fail "Mail shows no Add account"
# The button is the Splash's; click its centre as drawn in the welcome pane.
read -r X Y < <(get "snap?q=card" | python3 -c '
import json,sys
cards=[e for e in json.load(sys.stdin)["s"] if e["i"]=="card" and "account = nil" in (e.get("t") or "")]
x,y,w,h=cards[-1]["r"]; print(x+w/2, y+h*0.567)')
get "click?x=$X&y=$Y&wait=1" >/dev/null
for _ in $(seq 1 20); do snap "Add a mail account" | grep -q "^sheet Splash" && break; sleep 0.5; done
snap "Add a mail account" | grep -q "^sheet Splash" || fail "Mail's sign-in sheet did not open"
grab mail-sheet
pass "Mail's host-owned sign-in sheet is up (not signed in)"

# 4. AI providers, when this build selects it: opens on an empty list.
if [[ " $APPS " == *" ai-providers "* ]]; then
    key Escape
    launch "ai providers"
    wait_log "card: os.ai-providers running under"
    sleep 2
    grab ai-providers
    NAME=$(label_of ai-providers)
    [ -z "$(ls -A "$WORK/octos" 2>/dev/null)" ] || echo "note: octos core dir is not empty"
    pass "$NAME opened: $(grep -o 'card: os.ai-providers running under.*' "$LOG" | head -1)"
fi

# 5. Containment: one isolate per app under its policy, `os.` ids, bundles
#    unpacked outside every jail under the temp home.
for app in photos mail; do
    [ -f "$WORK/home/apps/.system/os.$app/"*/manifest.json ] || fail "os.$app not unpacked under $WORK/home/apps/.system"
    [ -d "$WORK/home/apps/os.$app" ] || fail "os.$app has no jail"
done
pass "bundles unpacked under <home>/apps/.system/<os.id>/<pack hash>/, jails at <home>/apps/<os.id>"

# 6. The log: no panic; UI hangs only where known (see KNOWN below).
get "log?n=2000" >"$WORK/remote-log.json"
grep -q "panicked" "$LOG" "$WORK/remote-log.json" && fail "panic in the log"
# Known: the catalog's startup Makepad-source probe, and App Hub unpacking and
# hashing a system bundle on the UI thread the first time it opens.
KNOWN='makepad_source::makepad_root|octosense_appstore::system::prepare'
UNKNOWN=$(grep "\[ui-hang\]" "$LOG" | grep -Ev "$KNOWN" || true)
[ -z "$UNKNOWN" ] || { echo "$UNKNOWN" | cut -c1-400; fail "unexpected UI hang"; }
echo "note: $(grep -c "\[ui-hang\]" "$LOG" || true) known UI hang(s) (startup catalog probe, first-open bundle unpack)"
pass "no panic, no unexpected [ui-hang]"

# Finish: grab and quit, then confirm the process is gone.
get "gq?scale=0.5" >/dev/null
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 0.5; done
kill -0 "$PID" 2>/dev/null && fail "the shell did not exit after /gq"
trap - EXIT
pass "shell exited after /gq; grabs in $WORK/grabs"

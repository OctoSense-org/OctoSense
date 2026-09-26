#!/usr/bin/env bash
# AI providers' QR import from an image, and the phone QR it exports, on the
# desktop, driven through Makepad's remote (instrument) mode: no OS
# screenshots, only the app's own frames.
#
#   cargo build --release [--config …] --features mobile-apps
#   cargo build --release [--config …] -p octosense-llm-service --example read_qr_image
#   scripts/ai_providers_remote.sh [artifacts-dir]
#
# Runs the release binary hidden with its own OCTOSENSE_HOME, octos core dir
# and file vaults under a temp directory (neither ~/.octosense nor the login
# keychain is touched). Every key in it is fake. The flow:
#   1. AI providers opens; the host reports an image picker ("Import code
#      from image").
#   2. Import by DROP: the fixture QR-A (OctoSense-System-Apps
#      config/tests/fixtures/qr-a.png, PIN 7K3M-9QX2) dropped on the import
#      sheet through the native drop path (/drop), PIN typed, imported:
#      DeepSeek and Z.ai, keys masked, both in <core>/profiles/_main.json.
#   3. Add OpenAI on the host's sheet with the fake key sk-test-0000000000001234.
#   4. Import by PICKER: "Choose image" (OCTOSENSE_LLM_TEST_IMAGE answers with
#      the fixture instead of the open panel, which a hidden run cannot click
#      through), a wrong PIN is refused and changes nothing, the right one
#      replaces the list (OpenAI is gone).
#   5. Add OpenAI again, Show QR for phone, grab the sheet, read the QR out of
#      the grab (the service's own image search, rqrr) and open it with the
#      PIN on the sheet: it equals the saved set and keys.
#   6. No key text in /snap, /d, /log or the host log; no panic; /gq exits.
# Artifacts: grabs/, export.png (the export sheet at full scale) and
# export-pin.txt, for scanning on a phone.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${OCTOSENSE_BIN:-$ROOT/target/release/octosense}
READ_QR=${READ_QR_IMAGE:-$ROOT/target/release/examples/read_qr_image}
SRC=$(cd "$ROOT" && python3 -c 'import json; print(json.load(open("system-apps.json"))["source"])')
FIXTURE=$(cd "$ROOT/$SRC/ai-providers/config/tests/fixtures" && pwd)/qr-a.png
FIXTURE_PIN=7K3M-9QX2
FAKE_KEY=sk-test-0000000000001234
# Every key the run handles: none may show up in the UI tree or the logs.
SECRETS='sk-test-|zai-test-|0000000000001234'
WORK=${1:-$(mktemp -d -t octosense-ai-providers)}
mkdir -p "$WORK/home" "$WORK/octos" "$WORK/grabs"
LOG=$WORK/host.log
PROFILE=$WORK/octos/profiles/_main.json
[ -x "$BIN" ] || { echo "no $BIN: build it first"; exit 2; }
[ -x "$READ_QR" ] || { echo "no $READ_QR: cargo build --release -p octosense-llm-service --example read_qr_image"; exit 2; }
echo "artifacts: $WORK"

cd "$WORK"
: >"$LOG"
env -u MAKEPAD_HOME -u MAKEPAD_WM_ROOT -u MAKEPAD_WM_THEME \
    OCTOSENSE_HOME="$WORK/home" OCTOS_APP_CORE_DIR="$WORK/octos" \
    OCTOSENSE_MAIL_VAULT=file OCTOSENSE_LLM_VAULT=file \
    OCTOSENSE_LLM_TEST_IMAGE="$FIXTURE" \
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

fail() { echo "FAIL: $*"; [ -z "${HOLD_ON_FAIL:-}" ] || sleep "$HOLD_ON_FAIL"; exit 1; }
pass() { echo "PASS: $*"; }
for _ in $(seq 1 120); do
    PORT=$(sed -n 's/.*\[makepad-remote\] listening on 127\.0\.0\.1:\([0-9]*\) pid=.*/\1/p' "$LOG" | head -1)
    [ -n "$PORT" ] && break
    sleep 0.5
done
[ -n "$PORT" ] || fail "no remote port in $LOG"
get() { curl -fsS "127.0.0.1:$PORT/$1"; }
q() { python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.argv[1]))' "$1"; }
key() {
    local out
    out=$(curl -s "127.0.0.1:$PORT/k?k=press&c=$1&wait=1${2:-}")
    case $out in *'"err"'*) echo "note: key $1: $out" ;; esac
}
type_keys() {
    local text=$1 i c
    for ((i = 0; i < ${#text}; i++)); do
        c=${text:i:1}
        case $c in
            " ") key Space ;;
            *) key "Key$(printf %s "$c" | tr a-z A-Z)" ;;
        esac
    done
}
# Input waits for the next frame; a hidden window sometimes misses it and
# the remote answers "retry" although the input was delivered. What input
# did is checked through the profile, the log and snapshots, so a missed
# frame is only noted (retrying could click twice).
input() {
    local out
    out=$(curl -s "127.0.0.1:$PORT/$1&wait=1")
    case $out in *'"err"'*) echo "note: ${1%%\?*}: $out" ;; esac
}
text() { input "t?t=$(q "$1")"; }
click() { input "click?x=$1&y=$2"; }
scroll() { input "m?k=scroll&x=$1&y=$2&dy=$3"; }
# A grab at layout scale (one pixel a point); prints its path.
grab() {
    local png
    for _ in 1 2 3 4 5; do
        png=$(curl -s "127.0.0.1:$PORT/g?scale=${2:-0.5}" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("png",""))' || true)
        [ -n "$png" ] && { cp "$png" "$WORK/grabs/$1.png"; echo "$WORK/grabs/$1.png"; return; }
        sleep 1
    done
    fail "no frame for $1"
}
# The centre of the card widget whose text is exactly $1: "x y".
widget() {
    get "snap?q=$(q "$1")" | python3 -c '
import json,sys
hits=[e for e in json.load(sys.stdin)["s"] if e.get("t")==sys.argv[1] and e["r"][2]>0]
if hits: x,y,w,h=hits[-1]["r"]; print(x+w/2, y+h/2)' "$1"
}
wait_widget() {
    for _ in $(seq 1 ${2:-40}); do [ -n "$(widget "$1")" ] && return 0; sleep 0.5; done
    fail "never showed: $1"
}
# The host's sheet (its rect and its program) once it runs `$1`.
sheet_rect() {
    get "snap?q=$(q "$1")" | python3 -c '
import json,sys
hits=[e for e in json.load(sys.stdin)["s"] if e["i"]=="sheet" and sys.argv[1] in (e.get("t") or "")]
if hits: print(*hits[-1]["r"])' "$1"
}
wait_sheet() {
    local r
    for _ in $(seq 1 40); do r=$(sheet_rect "$1"); [ -n "$r" ] && { echo "$r"; return 0; }; sleep 0.5; done
    fail "no sheet running $1"
}
wait_log() {
    for _ in $(seq 1 ${2:-40}); do grep -q "$1" "$LOG" && return 0; sleep 0.5; done
    fail "log never said: $1"
}
# Sheets are Splash isolates whose widgets /snap does not list: find their
# controls in a grab instead. Prints "pill X Y" (the blue action at the top
# right: Import, Save, Close) and "full Y" / "half X Y" for each grey field
# or button, top to bottom.
cat >"$WORK/sheet.py" <<'EOF'
import sys, zlib, struct
def read_png(path):
    data = open(path, "rb").read()
    pos, chunks = 8, {}
    idat = b""
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + n]
        if kind == b"IHDR": w, h, depth, color = struct.unpack(">IIBB", body[:10])
        if kind == b"IDAT": idat += body
        pos += 12 + n
    assert depth == 8 and color in (2, 6), (depth, color)
    bpp = 4 if color == 6 else 3
    raw, stride, rows, prev = zlib.decompress(idat), w * bpp, [], bytearray(w * bpp)
    i = 0
    for _ in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride]); i += 1 + stride
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]; c = prev[x - bpp] if x >= bpp else 0
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + b) & 255
            elif f == 3: line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line)); prev = line
    return w, h, bpp, rows
path, sx, sy, sw, sh = sys.argv[1], *map(float, sys.argv[2:6])
w, h, bpp, rows = read_png(path)
px = lambda x, y: rows[y][x * bpp:x * bpp + 3]
x0, x1 = int(sx + 20), int(sx + sw - 20)
inner = x1 - x0
grey = lambda p: 200 <= p[0] <= 248 and abs(p[0] - p[1]) <= 2 and 3 <= p[2] - p[0] <= 7
blue = lambda p: p[0] < 60 and 100 <= p[1] <= 160 and p[2] > 200
# The pill: the blue columns tall enough to be a filled button.
top = [y for y in range(int(sy), int(sy + 110))]
cols = [x for x in range(x0, x1) if sum(blue(px(x, y)) for y in top) >= 20]
if cols:
    ys = [y for y in top if sum(blue(px(x, y)) for x in cols[::3]) >= len(cols[::3]) // 2]
    print("pill", (cols[0] + cols[-1]) // 2, (ys[0] + ys[-1]) // 2 if ys else int(sy + 60))
out, y = [], int(sy + 60)
count = lambda y: sum(grey(px(x, y)) for x in range(x0, x1, 2)) * 2
while y < int(min(sy + sh, h)):
    if count(y) >= 0.4 * inner:
        start = y
        while y < h and count(y) >= 0.4 * inner: y += 1
        if y - start >= 20:
            row, runs, run = start + 3, [], None
            for x in range(x0, x1):
                if grey(px(x, row)):
                    run = [x, x] if run is None else [run[0], x]
                elif run: runs.append(run); run = None
            if run: runs.append(run)
            runs = [r for r in runs if r[1] - r[0] >= 0.3 * inner]
            mid = (start + y) // 2
            if runs and runs[0][1] - runs[0][0] >= 0.85 * inner:
                if not any(l.startswith("full") for l in out):
                    # The note line just above the first full-width control:
                    # how far right its text reaches (0: no text).
                    ink = [x for x in range(x0, x1) for yy in range(start - 40, start - 6) if max(px(x, yy)) < 120]
                    out.append("note %d" % (max(ink) - sx if ink else 0))
                out.append("full %d" % mid)
            else:
                for r in runs: out.append("half %d %d" % ((r[0] + r[1]) // 2, mid))
    y += 1
print("\n".join(out))
EOF
# `controls NAME RECT…` grabs, then prints the sheet's controls.
controls() { local png; png=$(grab "$1"); shift; python3 "$WORK/sheet.py" "$png" "$@"; }
# The import sheet moved on to the PIN once its note reads "Code read from
# the image. Type the PIN shown beside it." (wider than "Choose a screenshot
# or photo of the code…" or none), with no script error on the way.
pin_prompt() {
    local C n
    for _ in $(seq 1 10); do
        sleep 1
        C=$(controls "$@")
        n=$(echo "$C" | awk '$1=="note"{print $2; exit}')
        [ "${n:-0}" -ge 420 ] && break
    done
    if grep -q "not found in prototype chain" "$LOG"; then
        grep "not found in prototype chain" "$LOG" | head -3
        fail "the import sheet's script failed"
    fi
    [ "${n:-0}" -ge 420 ] || return 1
    echo "$C"
}
nth_full() { awk -v n="$1" '$1=="full"{i++; if(i==n){print $2; exit}}'; }
pill() { awk '$1=="pill"{print $2, $3; exit}'; }
profile_families() {
    python3 -c '
import json,sys
llm=json.load(open(sys.argv[1]))["config"]["llm"]
print(" ".join(s["family_id"] for s in [llm.get("primary")]+llm.get("fallbacks",[]) if s))' "$PROFILE"
}

for _ in $(seq 1 60); do curl -s "127.0.0.1:$PORT/snap?q=main_window" | grep -q '"s":\[{' && break; sleep 0.5; done
pass "desktop up on port $PORT (pid $PID)"

# 1. AI providers opens and offers the image import.
key Space "&cmd=1"; type_keys "ai providers"; key Return
wait_log "card: os.ai-providers running under"
wait_widget "Import code from image"
pass "AI providers opened; the host has an image picker (Import code from image)"

# 2. Import by drop onto the sheet.
read -r X Y < <(widget "Import code from image"); click "$X" "$Y"
read -r SX SY SW SH < <(wait_sheet "llm.sheet.image")
sleep 1
DROP=$(curl -s --get --data-urlencode "path=$FIXTURE" --data-urlencode "x=$((SX + SW / 2))" \
    --data-urlencode "y=$((SY + SH / 2))" --data-urlencode wait=1 "127.0.0.1:$PORT/drop")
case $DROP in *'"drop_handled":true'*'"drag_response":"copy"'*) ;; *) fail "the sheet did not take the drop: $DROP" ;; esac
wait_log "llm: image dropped on the import sheet"
C=$(pin_prompt drop-read "$SX" "$SY" "$SW" "$SH") || fail "the sheet did not ask for the PIN after the drop"
PIN_Y=$(echo "$C" | nth_full 3); read -r PX PY < <(echo "$C" | pill)
[ -n "$PIN_Y" ] && [ -n "$PX" ] || { echo "$C"; fail "import sheet controls not found"; }
click $((SX + SW / 2)) "$PIN_Y"; text "$FIXTURE_PIN"; click "$PX" "$PY"
wait_widget "Imported DeepSeek · deepseek-chat, Z.ai · glm-4.6"
[ "$(profile_families)" = "deepseek zai" ] || fail "profile after the drop: $(profile_families)"
# The rows are drawn by the app's script (on_render), which /snap does not
# list: the masked keys are in the grab; the keys themselves in the profile.
keys_end_with() {
    python3 -c '
import json,sys
env=json.load(open(sys.argv[1]))["config"]["env_vars"]
want=dict(a.split("=") for a in sys.argv[2:])
sys.exit(0 if sorted(env)==sorted(want) and all(env[k].endswith(v) for k,v in want.items()) else 1)' "$PROFILE" "$@"
}
keys_end_with DEEPSEEK_API_KEY=0000 ZAI_API_KEY=0000 || fail "the imported keys are not in the profile"
grab imported-by-drop >/dev/null
pass "dropped QR-A imported: DeepSeek + Z.ai and their keys in _main.json (grab imported-by-drop shows them masked)"

# 3. Add OpenAI with a fake key on the host's sheet.
add_openai() {
    local X Y SX SY SW SH C KY PX PY
    read -r X Y < <(widget "Add provider"); click "$X" "$Y"
    read -r SX SY SW SH < <(wait_sheet "llm.sheet.submit")
    sleep 1
    # OpenAI is the second family: the first grid row's right button.
    read -r X Y < <(controls "$1-sheet" "$SX" "$SY" "$SW" "$SH" | awk '$1=="half"{i++; if(i==2){print $2, $3; exit}}')
    click "$X" "$Y"
    scroll $((SX + SW / 2)) $((SY + SH / 2)) 300; sleep 1
    KY=$(controls "$1-key" "$SX" "$SY" "$SW" "$SH" | nth_full 1)
    [ -n "$KY" ] || fail "no key field on the add sheet"
    click $((SX + SW / 2)) "$KY"; text "$FAKE_KEY"
    scroll $((SX + SW / 2)) $((SY + SH / 2)) -900; sleep 1
    read -r PX PY < <(controls "$1-save" "$SX" "$SY" "$SW" "$SH" | pill)
    click "$PX" "$PY"
    wait_widget "Added OpenAI · gpt-4o"
}
add_openai add1
[ "$(profile_families)" = "deepseek zai openai" ] || fail "profile after the add: $(profile_families)"
keys_end_with DEEPSEEK_API_KEY=0000 ZAI_API_KEY=0000 OPENAI_API_KEY=1234 || fail "OpenAI's key is not in the profile"
grab added-openai >/dev/null
pass "OpenAI added on the sheet with a fake key (grab added-openai)"

# 4. Import by the picker (test image), a wrong PIN first.
read -r X Y < <(widget "Import code from image"); click "$X" "$Y"
read -r SX SY SW SH < <(wait_sheet "llm.sheet.pick")
sleep 1
C=$(controls pick-sheet "$SX" "$SY" "$SW" "$SH")
CHOOSE_Y=$(echo "$C" | nth_full 1); PIN_Y=$(echo "$C" | nth_full 3); read -r PX PY < <(echo "$C" | pill)
click $((SX + SW / 2)) "$CHOOSE_Y"
pin_prompt picked "$SX" "$SY" "$SW" "$SH" >/dev/null || fail "the sheet did not ask for the PIN after the pick"
BEFORE=$(cat "$PROFILE")
click $((SX + SW / 2)) "$PIN_Y"; text "0000-0000"; click "$PX" "$PY"; sleep 4
[ -n "$(sheet_rect "llm.sheet.pick")" ] || fail "a wrong PIN closed the sheet"
[ "$(cat "$PROFILE")" = "$BEFORE" ] || fail "a wrong PIN changed the profile"
grab wrong-pin >/dev/null
click $((SX + SW / 2)) "$PIN_Y"
for _ in 1 2 3 4 5 6 7 8 9; do key Backspace; done
text "$FIXTURE_PIN"; click "$PX" "$PY"
for _ in $(seq 1 40); do [ "$(profile_families)" = "deepseek zai" ] && break; sleep 0.5; done
[ "$(profile_families)" = "deepseek zai" ] || fail "the picked image did not replace the list: $(profile_families)"
wait_widget "Imported DeepSeek · deepseek-chat, Z.ai · glm-4.6"
pass "picked image imported after a refused wrong PIN (the list is QR-A's again)"

# 5. The phone QR: add OpenAI again, show the code, read it from the grab.
add_openai add2
read -r X Y < <(widget "Show QR for phone"); click "$X" "$Y"
wait_sheet "countdown :=" >/dev/null
sleep 1
PIN=$(get "snap?q=$(q "pin := Label")" | python3 -c '
import json,re,sys
for e in json.load(sys.stdin)["s"]:
    m=re.search(r"pin := Label\{[^\n]*text: \"([A-Z0-9-]+)\"", e.get("t") or "")
    if m: print(m.group(1))' | tail -1)
[ -n "$PIN" ] || fail "no PIN on the export sheet"
EXPORT=$(grab export 1)
cp "$EXPORT" "$WORK/export.png"; echo "$PIN" >"$WORK/export-pin.txt"
READ=$("$READ_QR" "$EXPORT" "$PIN" "$WORK/octos") || fail "the exported QR does not match the profile: $READ"
echo "$READ" | grep -q '"same_as_profile":true' || fail "$READ"
echo "$READ" | grep -q '"family":"openai"' || fail "OpenAI missing from the code: $READ"
pass "export QR read back from the grab (rqrr) with PIN $PIN: same set and keys as _main.json"

# 6. No key text anywhere the UI or logs show; no panic.
get "snap?all=1" >"$WORK/snap.json"; get "d" >"$WORK/dump.txt"; get "log?n=5000" >"$WORK/remote-log.json"
if grep -Eq "$SECRETS" "$WORK/snap.json" "$WORK/dump.txt" "$WORK/remote-log.json" "$LOG"; then
    grep -Eo ".{40}($SECRETS).{10}" "$WORK/snap.json" "$WORK/dump.txt" "$WORK/remote-log.json" "$LOG" | head -5
    fail "key text in the UI tree or a log"
fi
grep -q "panicked" "$LOG" "$WORK/remote-log.json" && fail "panic in the log"
pass "no key text in /snap, /d, /log or the host log; no panic"

get "gq?scale=0.5" >/dev/null
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 0.5; done
kill -0 "$PID" 2>/dev/null && fail "the shell did not exit after /gq"
trap - EXIT
pass "shell exited after /gq; grabs in $WORK/grabs, export.png + export-pin.txt in $WORK"

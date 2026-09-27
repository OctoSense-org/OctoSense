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
# keychain is touched). Every key in it is fake, and the binary runs under
# sandbox-exec with outbound HTTPS denied: no fake key reaches a real
# provider, and a provider test fails as a network failure. A local fake
# endpoint (fake.py, 127.0.0.1) stands in for a provider that answers. The
# flow:
#   1. AI providers opens; the host reports an image picker ("Import code
#      from image").
#   W1. Add DeepSeek with the add wizard, step by step: 1 search the families
#      and choose DeepSeek, 2 the model pull-down (DeepSeek V4 Flash,
#      Recommended), 3 the route pull-down (Official API), 4 a fake key, 5
#      Test connection: the provider cannot be reached, Save stays disabled
#      and "Save without testing" saves it as the primary.
#   W2. Change its model with the card's model pull-down (DeepSeek V4 Pro).
#   W3. Add Z.ai as a fallback on another model (GLM-4.7) and route (a
#      custom OpenAI-compatible endpoint: the fake endpoint); Test connection
#      passes ("Connected"), and "Add as fallback" saves it.
#   W4. The saved-models list and _main.json say so; Test connection on a
#      row; then both are removed with the cards' Remove (tap twice).
#   2. Import into the empty list by PASTE: an old single-provider code
#      (DeepSeek deepseek-v4-flash, fake key ending fcb0) typed into the
#      import sheet's code field is saved as the primary.
#   3. Add OpenAI with the wizard (its default gpt-4o on the official route)
#      with the fake key sk-test-0000000000001234, saved without testing.
#   4. Import by DROP: the fixture QR-A (OctoSense-System-Apps
#      config/tests/fixtures/qr-a.png, PIN 7K3M-9QX2: DeepSeek deepseek-chat
#      and Z.ai glm-4.6) dropped on the import sheet through the native drop
#      path (/drop), PIN typed. An import only adds, so there is no confirm
#      step: the primary and OpenAI stay first, QR-A's providers are appended
#      as fallbacks, and its DeepSeek key goes to a slot of its own
#      (DEEPSEEK_2_API_KEY) so the primary's key is not overwritten.
#   5. Import QR-A again by PICKER: "Choose image" (OCTOSENSE_LLM_TEST_IMAGE
#      answers with the fixture instead of the open panel, which a hidden run
#      cannot click through); a wrong PIN is refused and changes nothing, the
#      right one changes nothing either ("Already saved").
#   6. Show QR for phone, grab the sheet, read the QR out of the grab (the
#      service's own image search, rqrr) and open it with the PIN on the
#      sheet: it equals the saved set and keys, both DeepSeek slots included.
#      The countdown
#      ticks, and at 0 the sheet closes by itself (OCTOSENSE_LLM_QR_SECONDS
#      shortens its five minutes to $QR_SECONDS s), with no Splash error in
#      the log.
#   7. No key text in /snap, /d, /log or the host log; no panic, no Splash
#      error; /gq exits.
# Artifacts: grabs/, export.png (the export sheet at full scale) and
# export-pin.txt, for scanning on a phone. The script-drawn controls (the
# wizard's fields, rows and pull-downs, the app's cards) are not in /snap:
# they are found in grabs by their colours (blocks.py).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${OCTOSENSE_BIN:-$ROOT/target/release/octosense}
READ_QR=${READ_QR_IMAGE:-$ROOT/target/release/examples/read_qr_image}
SRC=$(cd "$ROOT" && python3 -c 'import json; print(json.load(open("system-apps.json"))["source"])')
FIXTURE=$(cd "$ROOT/$SRC/ai-providers/config/tests/fixtures" && pwd)/qr-a.png
FIXTURE_PIN=7K3M-9QX2
FAKE_KEY=sk-test-0000000000001234
# The old single-provider code the empty list is filled from (its key is fake).
LEGACY_CODE='{"llm_family":"deepseek","llm_model":"deepseek-v4-flash","llm_key":"sk-test-legacy-00000000fcb0"}'
# The phone QR's lifetime in this run: long enough to grab and read the code.
QR_SECONDS=${QR_SECONDS:-20}
# What a Splash script error logs (the export sheet's countdown used to log
# these every second).
SPLASH_ERRORS='pop_stack_resolved|mes empty|splash host callback error|\[E\] splash'
# Every key the run handles: none may show up in the UI tree or the logs.
SECRETS='sk-test-|zai-test-|0000000000001234|0000000000005678'
ZAI_KEY=sk-test-0000000000005678
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
    OCTOSENSE_LLM_TEST_IMAGE="$FIXTURE" OCTOSENSE_LLM_QR_SECONDS="$QR_SECONDS" \
    MAKEPAD_HIDE_WINDOWS=1 sandbox-exec -p '(version 1)(allow default)(deny network-outbound (remote tcp "*:443"))' \
    "$BIN" --remote >"$LOG" 2>&1 &
PID=$!
PORT=
# A provider that answers: OpenAI-compatible chat completions for the Z.ai
# fake key, on 127.0.0.1 (the sandbox lets local traffic through).
cat >"$WORK/fake.py" <<'EOF'
import http.server, json, sys
KEY = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def answer(self, ok, body):
        data = json.dumps(body if ok else {"error": {"message": "Incorrect API key"}}).encode()
        self.send_response(200 if ok else 401)
        self.send_header("content-type", "application/json"); self.send_header("content-length", str(len(data)))
        self.end_headers(); self.wfile.write(data)
    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0)))
        self.answer(self.headers.get("authorization") == "Bearer " + KEY, {"choices": []})
    def do_GET(self):
        self.answer(self.headers.get("authorization") == "Bearer " + KEY, {"data": [{"id": "glm-4.7"}]})
    def log_message(self, *args): pass
server = http.server.HTTPServer(("127.0.0.1", 0), H)
print(server.server_address[1], flush=True)
server.serve_forever()
EOF
python3 "$WORK/fake.py" "$ZAI_KEY" >"$WORK/fake.port" 2>/dev/null &
FAKE=$!
disown "$FAKE"
cleanup() {
    kill "$FAKE" 2>/dev/null || true
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
# Blocks of one colour in a grab (the wizard's fields, rows, pull-downs and
# buttons, the cards' controls): "cx cy w h" per block, top to bottom.
# `blocks.py SHEET_PY PNG RRGGBB [MINCOUNT] [MINW]`: rows with at least
# MINCOUNT pixels of the colour (+-3; +-30 for text, MINCOUNT 1) make bands
# (a text band may pause for 3 rows); a band splits where the colour pauses
# for more than 12 px across (24 for text); narrower blocks than MINW drop.
cat >"$WORK/blocks.py" <<'EOF'
import sys
exec(open(sys.argv[1]).read().split("path, sx, sy")[0])
w, h, bpp, rows = read_png(sys.argv[2])
want = tuple(int(sys.argv[3][i:i + 2], 16) for i in (0, 2, 4))
mincount = int(sys.argv[4]) if len(sys.argv) > 4 else 40
minw = int(sys.argv[5]) if len(sys.argv) > 5 else 40
text = mincount <= 1
gap, tol = (24, 30) if text else (12, 3)
def hits(y):
    r = rows[y]
    return [x for x in range(w) if abs(r[x * bpp] - want[0]) <= tol and abs(r[x * bpp + 1] - want[1]) <= tol and abs(r[x * bpp + 2] - want[2]) <= tol]
bands, cur, quiet = [], None, 0
for y in range(h):
    xs = hits(y)
    if len(xs) >= mincount:
        if cur is None: cur = [y, y, set()]
        cur[1] = y; cur[2].update(xs); quiet = 0
    elif cur is not None:
        quiet += 1
        if quiet > (3 if text else 0): bands.append(cur); cur = None; quiet = 0
if cur: bands.append(cur)
out = []
for y0, y1, xs in bands:
    xs = sorted(xs); start = prev = xs[0]
    for x in xs[1:] + [10 ** 9]:
        if x - prev > gap:
            if prev - start + 1 >= minw and y1 - y0 + 1 >= (6 if text else 14):
                out.append(((start + prev) // 2, (y0 + y1) // 2, prev - start + 1, y1 - y0 + 1))
            start = x
        prev = x
for b in sorted(out, key=lambda b: (b[1], b[0])): print(*b)
EOF
# The wizard's and the cards' colours (bundle/main.splash and
# host-service/src/sheets.rs in OctoSense-System-Apps): pull-down, option
# row, chosen row, field, Test connection, Save without testing, accent,
# danger.
PULLDOWN=edf2fc; OPTION=f6f8fc; CHOSEN=e8f0fe; FIELD=f2f2f7; TEST=e3edff; BYPASS=fff1e0; BLUE=007aff; RED=ff3b30
in_png() { python3 "$WORK/blocks.py" "$WORK/sheet.py" "$1" "$2" "${3:-40}" "${4:-40}"; }
# `blocks NAME COLOUR [MINCOUNT] [MINW]`: grab, then the colour's blocks.
blocks() { local png; png=$(grab "$1"); in_png "$png" "$2" "${3:-40}" "${4:-40}"; }
nth() { awk -v n="$1" 'NR==n{print $1, $2; exit}'; }
# The app scrolls as one list: its top (the cards) or its end (the QR
# buttons, under the cards).
top_app() { scroll 548 400 -3000; sleep 1; }
bottom_app() { scroll 548 400 3000; sleep 1; }

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

# The rows are drawn by the app's script (on_render), which /snap does not
# list: the masked keys are in the grab; the keys themselves in the profile.
keys_end_with() {
    python3 -c '
import json,sys
env=json.load(open(sys.argv[1]))["config"]["env_vars"]
want=dict(a.split("=") for a in sys.argv[2:])
sys.exit(0 if sorted(env)==sorted(want) and all(env[k].endswith(v) for k,v in want.items()) else 1)' "$PROFILE" "$@"
}
# Each saved route's model and the key env var octos reads it from.
profile_routes() {
    python3 -c '
import json,sys
llm=json.load(open(sys.argv[1]))["config"]["llm"]
print(" ".join("%s:%s" % (s.get("model_id","-"), (s.get("route") or {}).get("api_key_env","-")) for s in [llm.get("primary")]+llm.get("fallbacks",[]) if s))' "$PROFILE"
}
# The import sheet closes by itself once the code is imported.
sheet_closes() {
    for _ in $(seq 1 40); do [ -z "$(sheet_rect "$1")" ] && return 0; sleep 0.5; done
    return 1
}

# The add wizard, driven like a person. `next_step NAME` grabs (NAME) and
# presses the footer's Next (or the save on step 5): the lowest blue button
# narrower than a field; greyed out, it is not blue, and the step fails.
next_step() {
    local X Y
    read -r X Y _ < <(blocks "$1" $BLUE 40 60 | awk '$3<400' | tail -1) || true
    [ -n "$Y" ] || fail "$1: Next is not enabled"
    click "$X" "$Y"; sleep 1.5
}
# `pick NAME N`: open the step's pull-down and take its option N.
pick() {
    local X Y W H X2 Y2
    read -r X Y W H _ < <(blocks "$1-closed" $PULLDOWN 40 400 | head -1) || true
    [ -n "$Y" ] || fail "$1: no pull-down on this step"
    click "$X" "$Y"; sleep 1.5
    read -r X2 Y2 _ < <(blocks "$1" $OPTION 40 400 | awk -v y=$((Y + H / 2)) '$2>y' | nth "$2") || true
    [ -n "$Y2" ] || fail "$1: the pull-down shows no option $2"
    click "$X2" "$Y2"; sleep 1
}
# `add_model NAME QUERY MODEL_N ROUTE_N KEY [BASE]`: the five steps. Step 1
# searches the families for QUERY and chooses the first; steps 2 and 3 take
# option MODEL_N / ROUTE_N of their pull-downs (0: keep the preselected
# one), and BASE goes into the custom route's base URL; step 4 takes KEY;
# step 5 tests, then saves (Save once the test passes, else Save without
# testing after a network failure); SAVED says which: tested or untested.
add_model() {
    local name=$1 query=$2 model_n=$3 route_n=$4 key=$5 base=${6:-} X Y before
    SAVED=
    before=$(cat "$PROFILE" 2>/dev/null || true)
    read -r X Y _ < <(widget "Add model") || true; click "$X" "$Y"
    read -r SX SY SW SH < <(wait_sheet "llm.sheet.fetch_models")
    sleep 1.5
    read -r X Y _ < <(blocks "$name-1-start" $FIELD 40 400 | nth 1) || true
    [ -n "$Y" ] || fail "$name: no search field on step 1"
    click "$X" "$Y"; text "$query"; sleep 1.5
    read -r X Y _ < <(blocks "$name-1-search" $OPTION 40 400 | nth 1) || true
    [ -n "$Y" ] || fail "$name: no family matches $query"
    click "$X" "$Y"; sleep 1.5
    [ -n "$(blocks "$name-1-family" $CHOSEN 40 400)" ] || fail "$name: the family row is not marked chosen"
    next_step "$name-1-next"
    [ "$model_n" -gt 0 ] && pick "$name-2-model" "$model_n"
    next_step "$name-2-next"
    [ "$route_n" -gt 0 ] && pick "$name-3-route" "$route_n"
    if [ -n "$base" ]; then
        read -r X Y _ < <(blocks "$name-3-custom" $FIELD 40 400 | nth 1) || true
        [ -n "$Y" ] || fail "$name: no base URL field"
        click "$X" "$Y"; text "$base"; sleep 1
    fi
    next_step "$name-3-next"
    read -r X Y _ < <(blocks "$name-4-key" $FIELD 40 400 | nth 1) || true
    [ -n "$Y" ] || fail "$name: no key field on step 4"
    click "$X" "$Y"; text "$key"; sleep 1
    next_step "$name-4-next"
    read -r X Y _ < <(blocks "$name-5-review" $TEST 40 400 | head -1) || true
    [ -n "$Y" ] || fail "$name: no Test connection on step 5"
    [ -z "$(blocks "$name-5-review" $BLUE 40 60 | awk '$3<400')" ] || fail "$name: Save is enabled before a test"
    click "$X" "$Y"
    for _ in $(seq 1 15); do
        sleep 2
        read -r X Y _ < <(blocks "$name-5-tested" $BLUE 40 60 | awk '$3<400' | tail -1) || true
        if [ -n "$Y" ]; then click "$X" "$Y"; SAVED=tested; break; fi
        read -r X Y _ < <(in_png "$WORK/grabs/$name-5-tested.png" $BYPASS 40 100 | head -1) || true
        if [ -n "$Y" ]; then click "$X" "$Y"; SAVED=untested; break; fi
    done
    [ -n "$Y" ] || fail "$name: the test neither passed nor offered Save without testing"
    # The result line: green "Connected · N ms", or red "Couldn't connect: …".
    if [ "$SAVED" = tested ]; then
        [ -n "$(in_png "$WORK/grabs/$name-5-tested.png" 248a3d 1 100)" ] || fail "$name: no green Connected line (grab $name-5-tested)"
    else
        [ -n "$(in_png "$WORK/grabs/$name-5-tested.png" c4281c 1 100)" ] || fail "$name: no red Couldn't connect line (grab $name-5-tested)"
    fi
    for _ in $(seq 1 20); do [ "$(cat "$PROFILE" 2>/dev/null || true)" != "$before" ] && return 0; sleep 0.5; done
    fail "$name: the save did not change the profile"
}
profile_json() { python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["config"].get(sys.argv[2]), sort_keys=True))' "$PROFILE" "$1"; }

# W1. DeepSeek with the wizard; the provider is unreachable (sandbox).
add_model w1 deepseek 1 1 "$FAKE_KEY"
[ "$SAVED" = untested ] || fail "DeepSeek was not saved without testing"
wait_widget "Added DeepSeek · deepseek-v4-flash"
sheet_closes "llm.sheet.fetch_models" || fail "the wizard stayed up after the save"
[ "$(profile_json llm)" = '{"fallbacks": [], "primary": {"family_id": "deepseek", "model_id": "deepseek-v4-flash"}}' ] \
    || fail "profile after the wizard: $(profile_json llm)"
keys_end_with DEEPSEEK_API_KEY=1234 || fail "the DeepSeek key is not in the profile"
pass "DeepSeek added with the wizard: family search, model pull-down (DeepSeek V4 Flash, Recommended), route pull-down (Official API), key, Test connection failed (unreachable), Save without testing (grabs w1-1-family, w1-2-model, w1-3-route, w1-4-next, w1-5-review, w1-5-tested)"

# W2. The card's model pull-down.
read -r X Y _ < <(blocks w2-list $PULLDOWN 40 400 | nth 1) || true
[ -n "$Y" ] || fail "no model pull-down on the DeepSeek card"
click "$X" "$Y"; sleep 2
read -r X Y _ < <(blocks w2-pulldown $OPTION 40 400 | nth 2) || true
[ -n "$Y" ] || fail "the card's model pull-down has no second model"
click "$X" "$Y"
wait_widget "DeepSeek now uses DeepSeek V4 Pro."
[ "$(profile_json llm)" = '{"fallbacks": [], "primary": {"family_id": "deepseek", "model_id": "deepseek-v4-pro"}}' ] \
    || fail "profile after the model change: $(profile_json llm)"
pass "the card's model pull-down listed DeepSeek's catalog models and switched to DeepSeek V4 Pro (grab w2-pulldown)"

# W3. Z.ai as a fallback: GLM-4.7 (the second model) on a custom
# OpenAI-compatible route (the route pull-down's option after Official API),
# the local fake endpoint: the test passes.
FAKE_PORT=$(head -1 "$WORK/fake.port")
[ -n "$FAKE_PORT" ] || fail "the fake endpoint did not start"
ZAI_BASE="http://127.0.0.1:$FAKE_PORT/v1"
add_model w3 "z.ai" 2 2 "$ZAI_KEY" "$ZAI_BASE"
[ "$SAVED" = tested ] || fail "Z.ai was not saved after a passing test"
wait_widget "Added Z.ai · glm-4.7"
sheet_closes "llm.sheet.fetch_models" || fail "the wizard stayed up after the Z.ai save"
[ "$(profile_json llm)" = '{"fallbacks": [{"family_id": "zai", "model_id": "glm-4.7", "route": {"api_type": "openai", "base_url": "'"$ZAI_BASE"'"}}], "primary": {"family_id": "deepseek", "model_id": "deepseek-v4-pro"}}' ] \
    || fail "profile after the Z.ai add: $(profile_json llm)"
keys_end_with DEEPSEEK_API_KEY=1234 ZAI_API_KEY=5678 || fail "the keys after the Z.ai add are wrong"
pass "Z.ai added as a fallback on GLM-4.7 and a custom OpenAI-compatible route; Test connection passed, Add as fallback saved it (grabs w3-2-model, w3-3-custom, w3-5-tested)"

# W4. The list shows both; Test connection on the fallback's row passes;
# remove both with the cards' Remove (tap twice).
# Both cards in view: the list scrolled past the header.
top_app; scroll 548 400 220; sleep 1
[ "$(blocks w4-saved-models $PULLDOWN 40 400 | wc -l | tr -d ' ')" = 2 ] || fail "the list does not show two model pull-downs (grab w4-saved-models)"
# Green text: the rows' key statuses, and "Connected" once the test passes.
GREEN=248a3d
read -r X Y _ < <(blocks w4-rows $TEST 40 60 | tail -1) || true
[ -n "$Y" ] || fail "no Test connection on the second row"
BEFORE=$(in_png "$WORK/grabs/w4-rows.png" $GREEN 1 60 | wc -l)
click "$X" "$Y"
for _ in $(seq 1 20); do sleep 1; [ "$(blocks w4-row-tested $GREEN 1 60 | wc -l)" -gt "$BEFORE" ] && break; done
[ "$(in_png "$WORK/grabs/w4-row-tested.png" $GREEN 1 60 | wc -l)" -gt "$BEFORE" ] || fail "the row's Test connection did not say Connected (grab w4-row-tested)"
for n in 1 2; do
    top_app
    read -r X Y _ < <(blocks "w4-remove-$n" $RED 1 30 | head -1) || true
    [ -n "$Y" ] || fail "no Remove on the first card (grab w4-remove-$n)"
    click "$X" "$Y"; sleep 1
    click "$X" "$Y"; sleep 2
done
python3 -c 'import json,sys; sys.exit(0 if "llm" not in json.load(open(sys.argv[1]))["config"] else 1)' "$PROFILE" || fail "the list is not empty after removing both: $(profile_json llm)"
top_app; grab w4-empty >/dev/null
pass "the list showed DeepSeek V4 Pro (primary) and Z.ai GLM-4.7 (fallback 1); the fallback's Test connection said Connected; both removed with Remove, tapped twice (grabs w4-saved-models, w4-row-tested, w4-empty)"

# 2. Import into the empty list by pasting an old single-provider code.
bottom_app; read -r X Y < <(widget "Import code from image"); click "$X" "$Y"
read -r SX SY SW SH < <(wait_sheet "llm.sheet.import")
sleep 1
C=$(controls paste-sheet "$SX" "$SY" "$SW" "$SH")
CODE_Y=$(echo "$C" | nth_full 2); read -r PX PY < <(echo "$C" | pill)
[ -n "$CODE_Y" ] && [ -n "$PX" ] || { echo "$C"; fail "import sheet controls not found"; }
click $((SX + SW / 2)) "$CODE_Y"; text "$LEGACY_CODE"; click "$PX" "$PY"
wait_widget "Saved DeepSeek · deepseek-v4-flash as primary."
sheet_closes "llm.sheet.import" || fail "the import sheet stayed up after the paste"
[ "$(profile_routes)" = "deepseek-v4-flash:-" ] || fail "profile after the paste: $(profile_routes)"
keys_end_with DEEPSEEK_API_KEY=fcb0 || fail "the pasted key is not in the profile"
grab imported-by-paste >/dev/null
pass "an old single-provider code pasted into the empty list: DeepSeek deepseek-v4-flash is the primary (grab imported-by-paste)"

# 3. Add OpenAI with a fake key on the host's sheet.
add_openai() {
    add_model "$1" openai 0 0 "$FAKE_KEY"
    [ "$SAVED" = untested ] || fail "OpenAI was not saved without testing"
    wait_widget "Added OpenAI · gpt-4o"
}
add_openai add1
[ "$(profile_families)" = "deepseek openai" ] || fail "profile after the add: $(profile_families)"
keys_end_with DEEPSEEK_API_KEY=fcb0 OPENAI_API_KEY=1234 || fail "OpenAI's key is not in the profile"
grab added-openai >/dev/null
pass "OpenAI added with the wizard (gpt-4o, official route), saved without testing (grab added-openai)"

# 4. Import QR-A by drop onto the sheet: appended, no confirm step.
bottom_app; read -r X Y < <(widget "Import code from image"); click "$X" "$Y"
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
wait_widget "Added DeepSeek · deepseek-chat, Z.ai · glm-4.6 as fallbacks."
sheet_closes "llm.sheet.image" || fail "the import sheet stayed up (a confirm step?)"
[ "$(profile_families)" = "deepseek openai deepseek zai" ] || fail "profile after the drop: $(profile_families)"
[ "$(profile_routes)" = "deepseek-v4-flash:- gpt-4o:- deepseek-chat:DEEPSEEK_2_API_KEY glm-4.6:-" ] || fail "routes after the drop: $(profile_routes)"
keys_end_with DEEPSEEK_API_KEY=fcb0 OPENAI_API_KEY=1234 DEEPSEEK_2_API_KEY=0000 ZAI_API_KEY=0000 \
    || fail "the keys after the drop are wrong (the primary's key must be untouched)"
grab imported-by-drop >/dev/null
pass "dropped QR-A appended as fallbacks without a confirm step: primary + OpenAI first, QR-A's DeepSeek in DEEPSEEK_2_API_KEY, the primary's key untouched (grab imported-by-drop)"

# 5. Import QR-A again by the picker (test image), a wrong PIN first: the same
# code again changes nothing.
pick_and_unlock() {
    local X Y C
    bottom_app; read -r X Y < <(widget "Import code from image"); click "$X" "$Y"
    read -r SX SY SW SH < <(wait_sheet "llm.sheet.pick")
    sleep 1
    C=$(controls "$1-sheet" "$SX" "$SY" "$SW" "$SH")
    CHOOSE_Y=$(echo "$C" | nth_full 1); PIN_Y=$(echo "$C" | nth_full 3); read -r PX PY < <(echo "$C" | pill)
    click $((SX + SW / 2)) "$CHOOSE_Y"
    pin_prompt "$1-picked" "$SX" "$SY" "$SW" "$SH" >/dev/null || fail "the sheet did not ask for the PIN after the pick"
}
pick_and_unlock pick1
BEFORE=$(cat "$PROFILE")
click $((SX + SW / 2)) "$PIN_Y"; text "0000-0000"; click "$PX" "$PY"; sleep 4
[ -n "$(sheet_rect "llm.sheet.pick")" ] || fail "a wrong PIN closed the sheet"
[ "$(cat "$PROFILE")" = "$BEFORE" ] || fail "a wrong PIN changed the profile"
grab wrong-pin >/dev/null
click $((SX + SW / 2)) "$PIN_Y"
for _ in 1 2 3 4 5 6 7 8 9; do key Backspace; done
text "$FIXTURE_PIN"; click "$PX" "$PY"
wait_widget "Already saved: DeepSeek · deepseek-chat, Z.ai · glm-4.6."
sheet_closes "llm.sheet.pick" || fail "the import sheet stayed up after the second import"
[ "$(cat "$PROFILE")" = "$BEFORE" ] || fail "importing QR-A again changed the profile"
grab reimported >/dev/null
pass "QR-A picked and imported again after a refused wrong PIN: nothing changed, no duplicates (grab reimported)"

# 6. The phone QR: show the code, read it from the grab.
LOG_MARK=$(( $(wc -l <"$LOG") + 1 ))
bottom_app; read -r X Y < <(widget "Show QR for phone"); click "$X" "$Y"
read -r SX SY SW SH < <(wait_sheet "countdown :=")
SHOWN=$(date +%s)
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
echo "$READ" | grep -q '"DEEPSEEK_2_API_KEY":"••••0000"' || fail "the second DeepSeek slot is missing from the code: $READ"
echo "$READ" | grep -q '"DEEPSEEK_API_KEY":"••••fcb0"' || fail "the primary's key is missing from the code: $READ"
pass "export QR read back from the grab (rqrr) with PIN $PIN: same set and keys as _main.json, both DeepSeek slots included"
# The countdown ticks: two grabs of the sheet two seconds apart differ (only
# the "Expires in" line, below the PIN, changes on it: scroll down to it).
same_sheet() {
    python3 - "$1" "$2" "$SX" "$SY" "$SW" "$SH" "$WORK/sheet.py" <<'PY'
import sys
exec(open(sys.argv[7]).read().split("path, sx, sy")[0])
a, b = read_png(sys.argv[1]), read_png(sys.argv[2])
sx, sy, sw, sh = map(int, map(float, sys.argv[3:7]))
rows = range(sy, min(sy + sh, a[1], b[1]))
diff = sum(a[3][y][sx * a[2]:(sx + sw) * a[2]] != b[3][y][sx * b[2]:(sx + sw) * b[2]] for y in rows)
sys.exit(0 if diff == 0 else 1)
PY
}
scroll $((SX + SW / 2)) $((SY + SH / 2)) 600; sleep 1
T1=$(grab countdown-1); sleep 2; T2=$(grab countdown-2)
same_sheet "$T1" "$T2" && fail "the countdown does not tick (grabs countdown-1 and countdown-2 are the same)"
# At 0 the sheet closes itself: the service's own close comes 5 s after the
# lifetime (counted from before the code was sealed), so a sheet gone within
# the lifetime + 2 s closed itself.
for _ in $(seq 1 $((QR_SECONDS * 2 + 20))); do [ -z "$(sheet_rect "countdown :=")" ] && break; sleep 0.5; done
CLOSED=$(( $(date +%s) - SHOWN ))
[ -z "$(sheet_rect "countdown :=")" ] || fail "the export sheet did not close after $QR_SECONDS s"
[ "$CLOSED" -le $((QR_SECONDS + 2)) ] || fail "the export sheet closed after ${CLOSED} s, not by its own countdown ($QR_SECONDS s)"
grab export-closed >/dev/null
if tail -n +"$LOG_MARK" "$LOG" | grep -Eq "$SPLASH_ERRORS"; then
    tail -n +"$LOG_MARK" "$LOG" | grep -E "$SPLASH_ERRORS" | head -5
    fail "Splash errors while the export sheet was up"
fi
pass "the countdown ticked and the sheet closed itself after ${CLOSED} s ($QR_SECONDS s lifetime); no Splash error during the export"

# 7. No key text anywhere the UI or logs show; no panic.
get "snap?all=1" >"$WORK/snap.json"; get "d" >"$WORK/dump.txt"; get "log?n=5000" >"$WORK/remote-log.json"
if grep -Eq "$SECRETS" "$WORK/snap.json" "$WORK/dump.txt" "$WORK/remote-log.json" "$LOG"; then
    grep -Eo ".{40}($SECRETS).{10}" "$WORK/snap.json" "$WORK/dump.txt" "$WORK/remote-log.json" "$LOG" | head -5
    fail "key text in the UI tree or a log"
fi
grep -q "panicked" "$LOG" "$WORK/remote-log.json" && fail "panic in the log"
if grep -Eq "$SPLASH_ERRORS" "$LOG"; then grep -E "$SPLASH_ERRORS" "$LOG" | head -5; fail "Splash errors in the host log"; fi
pass "no key text in /snap, /d, /log or the host log; no panic, no Splash error"

get "gq?scale=0.5" >/dev/null
for _ in $(seq 1 30); do kill -0 "$PID" 2>/dev/null || break; sleep 0.5; done
kill -0 "$PID" 2>/dev/null && fail "the shell did not exit after /gq"
kill "$FAKE" 2>/dev/null || true
trap - EXIT
pass "shell exited after /gq; grabs in $WORK/grabs, export.png + export-pin.txt in $WORK"

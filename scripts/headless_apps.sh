#!/usr/bin/env bash
# Render every catalog app headlessly at a phone size, in the Android style,
# through Makepad's Studio protocol — "Studio headless mode".
#
# Each app is a binary built with the headless backend (`MAKEPAD=headless`,
# the flag makepad_test sets), driven over stdin with newline-JSON
# `StudioToApp` messages the way Studio drives it: a WindowGeomChange for
# the phone's geometry, ticks, then a Screenshot request; the app answers
# with `AppToStudio::Screenshot` (a PNG) on stdout, which lands in the
# output directory as <app>.png.
#
#   scripts/headless_apps.sh [out-dir] [app ...]
#
# Needs the sibling checkout built headless (the browser's CEF path is not
# headless-capable, so it is left out):
#   MAKEPAD=headless CARGO_TARGET_DIR=target-headless cargo build --release -p makepad-files ...
# and the Reference app the same way in this workspace:
#   MAKEPAD=headless CARGO_TARGET_DIR=target-headless cargo build --release -p makeos-reference --config target/ohos/patch.toml
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SIBLING="$(cd "${MAKEPAD_SIBLING:-$ROOT/../guofoo-makepad}" && pwd)"
BIN="$SIBLING/target-headless/release"
OUT="${1:-$ROOT/target/headless-apps}"; shift || true
STYLE="${MAKEOS_HEADLESS_STYLE:-android}"
# A phone in points: the WM's Android emulation window, minus its bar.
WIDTH="${MAKEOS_HEADLESS_WIDTH:-412}"
HEIGHT="${MAKEOS_HEADLESS_HEIGHT:-866}"
DPI="${MAKEOS_HEADLESS_DPI:-2}"
# Ticks before the screenshot: apps that load data need a few draw cycles.
WARMUP="${MAKEOS_HEADLESS_WARMUP:-30}"

# id:binary:args — the catalog's apps and how the catalog starts them.
ALL="reference:$ROOT/target-headless/release/makeos-reference: files:files:--demo terminal:terminal: mixer:makepad-mixer: task:task: sheets:sheets: photos:photos: clock:clock: weather:weather: fabric:makepad-fabric: score:makepad-app-score: video:video: route:makepad-app-route: vj:makepad-vj: fab:makepad-fab: image:image: pdf:pdf:"
WANT="$*"

mkdir -p "$OUT"
geom="{\"WindowGeomChange\":{\"dpi_factor\":$DPI.0,\"left\":0.0,\"top\":0.0,\"width\":$WIDTH.0,\"height\":$HEIGHT.0,\"window_id\":0}}"
tick='{"Tick":[]}'
shot='{"Screenshot":[{"request_id":1,"kind_id":0}]}'

for spec in $ALL; do
    IFS=: read -r id bin args <<<"$spec"
    if [ -n "$WANT" ] && ! [[ " $WANT " == *" $id "* ]]; then continue; fi
    exe="$bin"; [[ "$bin" == /* ]] || exe="$BIN/$bin"
    if [ ! -x "$exe" ]; then echo "$id: no headless binary ($exe)"; continue; fi
    dir="$OUT/$id"; rm -rf "$dir"; mkdir -p "$dir"
    {
        echo "$geom"
        for _ in $(seq 1 "$WARMUP"); do echo "$tick"; sleep 0.05; done
        echo "$shot"
        for _ in $(seq 1 12); do echo "$tick"; sleep 0.05; done
    } | (cd "$SIBLING" && MAKEPAD_STDIN_LOOP=1 MAKEPAD_WIDGET_STYLE="$STYLE" MAKEPAD_HEADLESS_OUT_DIR="$dir" MAKEPAD_HEADLESS_DPI="$DPI" MAKEPAD_HEADLESS_FRAMES=off perl -e 'alarm 90; exec @ARGV' -- "$exe" $args >"$dir/stdout.jsonl" 2>"$dir/stderr.log") || true
    python3 - "$dir/stdout.jsonl" "$OUT/$id.png" <<'PY' || echo "$id: NO SCREENSHOT ($(grep -m1 -iE 'panick|Cant parse|error' "$dir/stderr.log" || echo 'see stderr.log'))"
import json, sys
src, dst = sys.argv[1:3]
for line in open(src, encoding="utf-8", errors="replace"):
    line = line.strip()
    if not line.startswith('{"Screenshot"'):
        continue
    shot = json.loads(line)["Screenshot"]
    if isinstance(shot, list):
        shot = shot[0]
    open(dst, "wb").write(bytes(shot["png"]))
    print(f"{dst.split('/')[-1][:-4]}: {shot['width']}x{shot['height']} -> {dst}")
    break
else:
    sys.exit(1)
PY
done

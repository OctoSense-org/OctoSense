#!/bin/bash
# After a flash: wait for boot, run the checklist and the agent harness, take the
# three screenshots, and keep everything next to the build's manifest.
#   verify-phone.sh <build-tag> [serial]
set -uo pipefail
TAG=${1:?build tag}; D=${2:-cfb7c9e3}
HERE=$(cd "$(dirname "$0")/.." && pwd)
# OCTOSENSE_ROM_BUILDS overrides the local builds folder (see release.sh).
DIR=${OCTOSENSE_ROM_BUILDS:-$HOME/home/octosense-org/rom-builds}/$TAG; OUT=$DIR/verify; mkdir -p "$OUT"
export ADB=${ADB:-$HOME/.local/share/octosense/android-tools/sdk/platform-tools/adb}
echo "== waiting for boot"
for i in $(seq 1 200); do [ "$("$ADB" -s "$D" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && break; sleep 3; done
[ "$("$ADB" -s "$D" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] || { echo "phone not booted; nothing verified" >&2; exit 1; }
VERSION=$("$ADB" -s "$D" shell getprop ro.lineage.version | tr -d '\r')
echo "running $VERSION on slot $("$ADB" -s "$D" shell getprop ro.boot.slot_suffix | tr -d '\r')"
# The manifest records the build's ro.build.version.incremental (unique per build);
# refuse to record results for another build. FORCE=1 skips this for older builds.
WANT=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1])).get('incremental',''))" "$DIR/manifest.json" 2>/dev/null)
GOT=$("$ADB" -s "$D" shell getprop ro.build.version.incremental | tr -d '\r')
if [ "${FORCE:-0}" != 1 ] && [ "$GOT" != "$WANT" ]; then echo "phone runs build '$GOT', manifest is '$WANT'; nothing verified" >&2; exit 1; fi
echo "== checklist"; bash "$HERE/scripts/checklist.sh" "$D" | tee "$OUT/checklist.txt"
echo "== agent"; OUT=$OUT bash "$HERE/scripts/agent-test.sh" "$D" 2>&1 | grep -v "^$\|^SERVICE\|Client:" | tee "$OUT/agent.txt" | tail -12
echo "== screenshots"
shot() { "$ADB" -s "$D" exec-out screencap -p > "$OUT/$1.png" 2>/dev/null; [ -s "$OUT/$1.png" ] && echo "$1.png $(wc -c < "$OUT/$1.png") bytes"; }
"$ADB" -s "$D" shell "input keyevent KEYCODE_WAKEUP; wm dismiss-keyguard; input keyevent KEYCODE_HOME" >/dev/null 2>&1; sleep 2; shot home
"$ADB" -s "$D" shell "cmd statusbar expand-settings" >/dev/null 2>&1; sleep 2; shot shade; "$ADB" -s "$D" shell "cmd statusbar collapse" >/dev/null 2>&1
"$ADB" -s "$D" shell "input keyevent KEYCODE_APP_SWITCH" >/dev/null 2>&1; sleep 2; shot recents; "$ADB" -s "$D" shell "input keyevent KEYCODE_HOME" >/dev/null 2>&1
echo "results in $OUT"

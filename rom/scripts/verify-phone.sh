#!/bin/bash
# After a flash: wait for boot, run the checklist and the agent harness, take the
# three screenshots, and keep everything next to the build's manifest.
#   verify-phone.sh <build-tag> [serial]
set -uo pipefail
TAG=${1:?build tag}; D=${2:-cfb7c9e3}
HERE=$(cd "$(dirname "$0")/.." && pwd)
DIR=$HOME/home/octosense-org/rom-builds/$TAG; OUT=$DIR/verify; mkdir -p "$OUT"
export ADB=${ADB:-$HOME/.local/share/octosense/android-tools/sdk/platform-tools/adb}
echo "== waiting for boot"
for i in $(seq 1 200); do [ "$("$ADB" -s "$D" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ] && break; sleep 3; done
"$ADB" -s "$D" shell "getprop ro.lineage.version; getprop ro.boot.slot_suffix" | tr '\r\n' '  '; echo
echo "== checklist"; bash "$HERE/scripts/checklist.sh" "$D" | tee "$OUT/checklist.txt"
echo "== agent"; OUT=$OUT bash "$HERE/scripts/agent-test.sh" "$D" 2>&1 | grep -v "^$\|^SERVICE\|Client:" | tee "$OUT/agent.txt" | tail -12
echo "== screenshots"
shot() { "$ADB" -s "$D" exec-out screencap -p > "$OUT/$1.png" 2>/dev/null; [ -s "$OUT/$1.png" ] && echo "$1.png $(wc -c < "$OUT/$1.png") bytes"; }
"$ADB" -s "$D" shell "input keyevent KEYCODE_WAKEUP; wm dismiss-keyguard; input keyevent KEYCODE_HOME" >/dev/null 2>&1; sleep 2; shot home
"$ADB" -s "$D" shell "cmd statusbar expand-settings" >/dev/null 2>&1; sleep 2; shot shade; "$ADB" -s "$D" shell "cmd statusbar collapse" >/dev/null 2>&1
"$ADB" -s "$D" shell "input keyevent KEYCODE_APP_SWITCH" >/dev/null 2>&1; sleep 2; shot recents; "$ADB" -s "$D" shell "input keyevent KEYCODE_HOME" >/dev/null 2>&1
echo "results in $OUT"

#!/usr/bin/env bash
# Provision the LLM for AppCard inside OctoSense on the connected Android phone.
#   provision-appcard-llm.sh <family> <model> <key>
#   family: zhipu (bigmodel.cn key)  |  zai (z.ai key)  |  deepseek | openai | anthropic | ...
# The key goes only to the phone (launch-intent extra); it is not logged here.
set -euo pipefail
[ $# -eq 3 ] || { echo "usage: $0 <family> <model> <key>" >&2; exit 2; }
family=$1 model=$2 key=$3
ADB=${ADB:-$(command -v adb || true)}
[ -n "$ADB" ] || { echo "adb not found; set ADB=/path/to/adb (cargo-makepad installs one under tools/cargo_makepad/android_*/platform-tools)" >&2; exit 1; }
[ -x "$ADB" ] || { echo "adb not found; set ADB=/path/to/adb" >&2; exit 1; }
"$ADB" get-state >/dev/null 2>&1 || { echo "no authorized device" >&2; exit 1; }
# Build the JSON with python so quoting cannot break it; ship it through both shells as a single-quoted literal.
json=$(python3 -c 'import json,sys; print(json.dumps({"llm_family":sys.argv[1],"llm_model":sys.argv[2],"llm_key":sys.argv[3]}))' "$family" "$model" "$key")
esc=${json//\"/\\\"}
"$ADB" logcat -c >/dev/null 2>&1 || true
"$ADB" shell "am start -S -n dev.makepad.octosense/.MakepadApp --es makepad.PROVISION_CONFIG '$esc'" >/dev/null
echo "launched OctoSense with provisioning extra; open the AppCard tile now (waiting up to 40 s for the app to record it)…"
for i in $(seq 1 40); do
  line=$("$ADB" logcat -d -s Makepad 2>/dev/null | grep -E 'provisioned LLM|LLM provisioning failed' | tail -1 || true)
  [ -n "$line" ] && break; sleep 1
done
if [ -z "${line:-}" ]; then echo "no provisioning line yet — tap the AppCard tile, then run:  $ADB logcat -d -s Makepad | grep -E 'provisioned LLM|provisioning failed'"; exit 3; fi
echo "$line" | sed -E 's#.*Makepad : ##'
case "$line" in *"provisioned LLM"*) echo "OK: family=$family model=$model recorded. Now send a request (e.g. 'weather tokyo') from the composer.";; *) echo "FAILED — see the line above"; exit 4;; esac

#!/usr/bin/env bash
# Provision the LLM for AppCard inside a development test build of OctoSense
# Home on the connected Android phone.
#   PACKAGE=dev.makepad.octosense.<suffix> provision-appcard-llm.sh <family> <model> <key>
#   family: zhipu (bigmodel.cn key)  |  zai (z.ai key)  |  deepseek | openai | anthropic | ...
# The key goes only to the phone (launch-intent extra); it is not logged here.
# Any app can start Home with extras, so a release Home ignores launch-time
# provisioning: PACKAGE must be a development build that links AppCard
# (`--features app-appcard,dev-mode`), installed as its own test package. A
# release build takes providers from AI providers (QR code and PIN) instead.
set -euo pipefail
[ $# -eq 3 ] || { echo "usage: PACKAGE=dev.makepad.octosense.<suffix> $0 <family> <model> <key>" >&2; exit 2; }
family=$1 model=$2 key=$3
package=${PACKAGE:-}
case "$package" in
  *[!a-z0-9_.]*|dev.makepad.octosense.bridge|dev.makepad.octosense.quickstep|dev.makepad.octosense.agent) package= ;;
  dev.makepad.octosense.?*) ;;
  *) package= ;;
esac
[ -n "$package" ] || { echo "set PACKAGE to the development test package (dev.makepad.octosense.<suffix>), never the installed Home" >&2; exit 2; }
ADB=${ADB:-$(command -v adb || true)}
[ -n "$ADB" ] || { echo "adb not found; set ADB=/path/to/adb (cargo-makepad installs one under tools/cargo_makepad/android_*/platform-tools)" >&2; exit 1; }
[ -x "$ADB" ] || { echo "adb not found; set ADB=/path/to/adb" >&2; exit 1; }
"$ADB" get-state >/dev/null 2>&1 || { echo "no authorized device" >&2; exit 1; }
# Build the JSON with python so quoting cannot break it; ship it through both shells as a single-quoted literal.
json=$(python3 -c 'import json,sys; print(json.dumps({"llm_family":sys.argv[1],"llm_model":sys.argv[2],"llm_key":sys.argv[3]}))' "$family" "$model" "$key")
esc=${json//\"/\\\"}
"$ADB" logcat -c >/dev/null 2>&1 || true
"$ADB" shell "am start -S -n $package/.MakepadApp --es makepad.PROVISION_CONFIG '$esc'" >/dev/null
echo "launched $package with provisioning extra; open the AppCard tile now (waiting up to 40 s for the app to record it)…"
for i in $(seq 1 40); do
  line=$("$ADB" logcat -d -s Makepad 2>/dev/null | grep -E 'provisioned LLM|LLM provisioning failed' | tail -1 || true)
  [ -n "$line" ] && break; sleep 1
done
if [ -z "${line:-}" ]; then echo "no provisioning line yet — tap the AppCard tile, then run:  $ADB logcat -d -s Makepad | grep -E 'provisioned LLM|provisioning failed'"; exit 3; fi
echo "$line" | sed -E 's#.*Makepad : ##'
case "$line" in *"provisioned LLM"*) echo "OK: family=$family model=$model recorded. Now send a request (e.g. 'weather tokyo') from the composer.";; *) echo "FAILED — see the line above"; exit 4;; esac

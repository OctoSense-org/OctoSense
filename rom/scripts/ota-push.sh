#!/bin/bash
# Option 2: update a running OctoSense phone from this Mac, over the phone's own
# network. update_engine on the phone streams the ROM from the GitHub release
# into the inactive slot; nothing big crosses the USB cable.
#   ota-push.sh [tag|latest] [serial]
# Needs "Rooted debugging" on in Developer options (adb root), and Wi-Fi.
set -euo pipefail
TAG=${1:-latest}; D=${2:-cfb7c9e3}
REPO=OctoSense-org/octosense-rom
ADB=${ADB:-$HOME/.local/share/octosense/android-tools/sdk/platform-tools/adb}
if [ "$TAG" = latest ]; then JSON_URL="https://github.com/$REPO/releases/latest/download/update.json"
else JSON_URL="https://github.com/$REPO/releases/download/$TAG/update.json"; fi
JSON=$(curl -fsSL "$JSON_URL")
read -r URL OFFSET SIZE INC < <(python3 -c 'import json,sys; r=json.loads(sys.argv[1])["rom"]; print(r["url"], r["payload_offset"], r["payload_size"], r["incremental"])' "$JSON")
HEADERS=$(python3 -c 'import json,sys; print("\n".join(json.loads(sys.argv[1])["rom"]["payload_properties"]))' "$JSON")
# update_engine wants the storage URL that serves byte ranges, not the release link's redirect.
DIRECT=$(curl -fsSI -o /dev/null -w '%{redirect_url}' "$URL"); DIRECT=${DIRECT:-$URL}
echo "phone runs $("$ADB" -s "$D" shell getprop ro.build.version.incremental | tr -d '\r'), offering $INC"
"$ADB" -s "$D" root >/dev/null; sleep 2
"$ADB" -s "$D" wait-for-device
[ "$("$ADB" -s "$D" shell id -u | tr -d '\r')" = 0 ] || { echo "adb root refused: enable Developer options > Rooted debugging" >&2; exit 1; }
"$ADB" -s "$D" shell "update_engine_client --payload='$DIRECT' --update --offset=$OFFSET --size=$SIZE --headers='$HEADERS'"
echo "streaming; progress:"
"$ADB" -s "$D" shell "update_engine_client --follow" 2>&1 | tr -d '\r' | awk '/UPDATE_STATUS|progress/ { print; fflush() }' | tail -n +1
echo "when it reports UPDATED_NEED_REBOOT: adb -s $D reboot"

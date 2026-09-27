#!/bin/bash
# Flashes an OctoSense ROM build onto the OnePlus 6 from this Mac. See docs/flashing.md.
#   flash.sh <dir with lineage-*.zip boot.img vbmeta.img> [serial]
# The first flash over LineageOS needs a factory reset afterwards (different keys).
set -euo pipefail
DIR=${1:?build products directory}; D=${2:-cfb7c9e3}
TOOLS=$HOME/.local/share/octosense/android-tools/sdk/platform-tools
ADB="$TOOLS/adb"; FASTBOOT="$TOOLS/fastboot"
ZIP=$(ls "$DIR"/lineage-*.zip | head -1)
for f in "$ZIP" "$DIR/boot.img" "$DIR/vbmeta.img"; do [ -f "$f" ] || { echo "missing $f" >&2; exit 1; }; done
echo "zip: $ZIP"
if "$ADB" devices | grep -q "^$D.*device$"; then
    echo "rebooting to bootloader"; "$ADB" -s "$D" reboot bootloader
fi
until "$FASTBOOT" devices | grep -q "^$D"; do sleep 2; done
"$FASTBOOT" -s "$D" flash boot "$DIR/boot.img"
"$FASTBOOT" -s "$D" flash vbmeta "$DIR/vbmeta.img"
"$FASTBOOT" -s "$D" reboot recovery
echo "in recovery choose Apply update -> Apply from ADB, then press Enter"; read -r _
until "$ADB" devices | grep -q "^$D.*sideload$"; do sleep 2; done
"$ADB" -s "$D" sideload "$ZIP"
echo "sideload done; choose Factory reset -> Format data when coming from LineageOS, then reboot."

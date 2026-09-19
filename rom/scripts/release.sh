#!/bin/bash
# Package a finished host build for the web installer and open it in Chrome.
#   release.sh <build-tag> [serial]      e.g. release.sh 20260918-f
# Downloads the images and zip from the host, writes the manifest, points the
# served directory at the build, starts the local server if needed, puts the
# phone into bootloader mode when adb answers, and opens the installer page.
set -euo pipefail
TAG=${1:?build tag}; D=${2:-cfb7c9e3}
# The build host lives in a local, uncommitted file: ~/.config/octosense/build.env
# with OCTOSENSE_BUILD_HOST=user@host and OCTOSENSE_BUILD_KEY=<ssh key path>.
[ -f "$HOME/.config/octosense/build.env" ] && . "$HOME/.config/octosense/build.env"
HOST=${OCTOSENSE_BUILD_HOST:?set OCTOSENSE_BUILD_HOST in ~/.config/octosense/build.env}
KEY=${OCTOSENSE_BUILD_KEY:?set OCTOSENSE_BUILD_KEY in ~/.config/octosense/build.env}
HERE=$(cd "$(dirname "$0")/.." && pwd)
BUILDS=$HOME/home/octosense-org/rom-builds; DIR=$BUILDS/$TAG; SERVE=$BUILDS/serve
ADB=${ADB:-$HOME/.local/share/octosense/android-tools/sdk/platform-tools/adb}
OUT='~/octosense-adr0001/build/out/octosense-rom/target/product/enchilada'
mkdir -p "$DIR" "$SERVE"
echo "== downloading build $TAG"
# One remote argument, paths separated by spaces: rsync fetches them all in one session.
rsync -a --partial -e "ssh -i $KEY -o BatchMode=yes" \
  "$HOST:$OUT/boot.img $OUT/dtbo.img $OUT/vbmeta.img $OUT/vendor.img $OUT/system.img ~/octosense-adr0001/exports/rom-build/zip.sha256 ~/octosense-adr0001/exports/rom-build/lineage-*.zip" "$DIR/"
echo "== manifest"
python3 "$HERE/scripts/make-manifest.py" "$DIR" "OctoSense $TAG" sdm845
INC=$(ssh -i "$KEY" -o BatchMode=yes "$HOST" "grep -m1 '^ro.build.version.incremental=' $OUT/system/build.prop | cut -d= -f2")
python3 - "$DIR/manifest.json" "$INC" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); m["incremental"] = sys.argv[2]
open(sys.argv[1], "w").write(json.dumps(m, indent=2) + "\n")
PY
echo "incremental $INC"
echo "== serve"
ln -sf "$HERE/web-installer/index.html" "$HERE/web-installer/fastboot.mjs" "$SERVE/"
for f in boot dtbo vbmeta vendor system; do ln -sf "$DIR/$f.img" "$SERVE/$f.img"; done
ln -sf "$DIR/manifest.json" "$SERVE/manifest.json"
if ! curl -sf http://localhost:8321/manifest.json >/dev/null; then
  (nohup python3 -m http.server 8321 --directory "$SERVE" >/dev/null 2>&1 &); sleep 1
fi
curl -s http://localhost:8321/manifest.json | head -3
echo "== phone"
pkill -f "fastboot" 2>/dev/null || true
if "$ADB" devices | grep -q "^$D.*device$"; then "$ADB" -s "$D" reboot bootloader; echo "sent to bootloader"; else echo "adb not answering: hold VolUp+VolDown+Power for the bootloader"; fi
open -a "Google Chrome" http://localhost:8321 2>/dev/null || echo "open http://localhost:8321 in Chrome"
echo "in Chrome: Connect, then Install (leave Erase unchecked for an update)"

#!/bin/bash
# Package a finished host build, with an optional local installer preview.
#   release.sh <build-tag> [--serve]      e.g. release.sh 20260918-f --serve
# This command does not reboot phones, terminate USB tools or publish a release.
set -euo pipefail
TAG=${1:?build tag}; shift
[[ "$TAG" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || { echo "invalid build tag" >&2; exit 2; }
SERVE_PREVIEW=false
if [ "${1:-}" = --serve ] && [ "$#" -eq 1 ]; then
  SERVE_PREVIEW=true
elif [ "$#" -ne 0 ]; then
  echo "usage: release.sh <build-tag> [--serve]" >&2; exit 2
fi
# The build host lives in a local, uncommitted file: ~/.config/octosense/build.env
# with OCTOSENSE_BUILD_HOST=user@host and OCTOSENSE_BUILD_KEY=<ssh key path>.
[ -f "$HOME/.config/octosense/build.env" ] && . "$HOME/.config/octosense/build.env"
HOST=${OCTOSENSE_BUILD_HOST:?set OCTOSENSE_BUILD_HOST in ~/.config/octosense/build.env}
KEY=${OCTOSENSE_BUILD_KEY:?set OCTOSENSE_BUILD_KEY in ~/.config/octosense/build.env}
HERE=$(cd "$(dirname "$0")/.." && pwd)
BUILDS=$HOME/home/octosense-org/rom-builds; DIR=$BUILDS/$TAG; SERVE=$BUILDS/serve
OUT='~/octosense-adr0001/build/out/octosense-rom/target/product/enchilada'
mkdir -p "$DIR" "$SERVE"
echo "== downloading build $TAG"
# One remote argument, paths separated by spaces: rsync fetches them all in one session.
rsync -a --partial -e "ssh -i $KEY -o BatchMode=yes" \
  "$HOST:$OUT/boot.img $OUT/dtbo.img $OUT/vbmeta.img $OUT/vendor.img $OUT/system.img ~/octosense-adr0001/exports/rom-build/zip.sha256 ~/octosense-adr0001/exports/rom-build/lineage-*.zip" "$DIR/"
echo "== manifest"
python3 "$HERE/scripts/make-manifest.py" "$DIR" "OctoSense $TAG" enchilada
INC=$(ssh -i "$KEY" -o BatchMode=yes "$HOST" "grep -m1 '^ro.build.version.incremental=' $OUT/system/build.prop | cut -d= -f2")
python3 - "$DIR/manifest.json" "$INC" <<'PY'
import json, sys
m = json.load(open(sys.argv[1])); m["incremental"] = sys.argv[2]
open(sys.argv[1], "w").write(json.dumps(m, indent=2) + "\n")
PY
echo "incremental $INC"
[ "$SERVE_PREVIEW" = true ] || { echo "packaged: $DIR"; exit 0; }
echo "== serve"
ln -sf "$HERE/web-installer/index.html" "$HERE/web-installer/fastboot.mjs" "$SERVE/"
ln -sfn "$HERE/web-installer/src" "$SERVE/src"
ln -sfn "$HERE/web-installer/vendor" "$SERVE/vendor"
for f in boot dtbo vbmeta vendor system; do ln -sf "$DIR/$f.img" "$SERVE/$f.img"; done
ln -sf "$DIR/manifest.json" "$SERVE/manifest.json"
if ! curl -sf http://localhost:8321/manifest.json >/dev/null; then
  (nohup python3 -m http.server 8321 --bind 127.0.0.1 --directory "$SERVE" >/dev/null 2>&1 &); sleep 1
fi
curl -s http://localhost:8321/manifest.json | head -3
echo "open http://localhost:8321 in Chrome or Edge"
echo "in Chrome: Connect and review compatibility; this developer preview supports verified fresh installs only"

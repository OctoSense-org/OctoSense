#!/bin/bash
# Publish a build as a GitHub release of this repository: the ROM's OTA zip, the
# platform-signed Home APK, and update.json, which the phone's agent reads from
# https://github.com/OctoSense-org/octosense-rom/releases/latest/download/update.json
#   publish-release.sh <build-tag> [home.apk]     e.g. publish-release.sh 20260919-g
# The ROM zip must already be in rom-builds/<tag> (scripts/release.sh downloads it).
set -euo pipefail
TAG=${1:?build tag}
HERE=$(cd "$(dirname "$0")/.." && pwd)
REPO=OctoSense-org/octosense-rom
BUILDS=$HOME/home/octosense-org/rom-builds; DIR=$BUILDS/$TAG
HOME_APK=${2:-$HERE/out/home/rom/OctoSenseHome.apk}
if [ -f "$HOME_APK" ]; then
    [ "$(basename "$HOME_APK")" = OctoSenseHome.apk ] || { echo "use the ROM build's OctoSenseHome.apk and adjacent build.json" >&2; exit 2; }
    python3 "$HERE/scripts/stage-home.py" --build "$(dirname "$HOME_APK")" --verify-only
fi
ZIP=$(ls "$DIR"/lineage-*.zip | head -1)
ASSET_ZIP="octosense-$TAG-enchilada.zip"
[ -f "$HOME_APK" ] && HOME_APK=$(cd "$(dirname "$HOME_APK")" && pwd)/$(readlink "$HOME_APK" 2>/dev/null || basename "$HOME_APK")
python3 - "$ZIP" "$TAG" "$REPO" "$ASSET_ZIP" "$HOME_APK" "$DIR/update.json" <<'PY'
import hashlib, json, os, struct, subprocess, sys, zipfile
zpath, tag, repo, asset_zip, apk, out = sys.argv[1:7]
base = f"https://github.com/{repo}/releases/download/{tag}/"
def sha(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for c in iter(lambda: f.read(1 << 20), b""): h.update(c)
    return h.hexdigest()
z = zipfile.ZipFile(zpath)
meta = dict(l.split("=", 1) for l in z.read("META-INF/com/android/metadata").decode().splitlines() if "=" in l)
info = z.getinfo("payload.bin")
with open(zpath, "rb") as f:
    f.seek(info.header_offset); h = f.read(30)
n, e = struct.unpack("<HH", h[26:30])
props = [l for l in z.read("payload_properties.txt").decode().splitlines() if l]
update = {"schema": 1, "name": f"OctoSense {tag}", "tag": tag,
  "rom": {"device": meta.get("pre-device"), "incremental": meta.get("post-build-incremental"),
          "timestamp": int(meta.get("post-timestamp", "0")), "security_patch": meta.get("post-security-patch-level"),
          "url": base + asset_zip, "size": os.path.getsize(zpath), "sha256": sha(zpath),
          "payload_offset": info.header_offset + 30 + n + e, "payload_size": info.file_size,
          "payload_properties": props}}
if os.path.isfile(apk):
    tools = os.path.expanduser("~/.local/share/octosense/android-tools/sdk/build-tools/35.0.0/aapt2")
    badging = subprocess.run([tools, "dump", "badging", apk], capture_output=True, text=True).stdout
    vc = int(badging.split("versionCode='")[1].split("'")[0])
    update["home"] = {"package": "dev.makepad.octosense", "version_code": vc,
                      "url": base + os.path.basename(apk), "size": os.path.getsize(apk), "sha256": sha(apk)}
open(out, "w").write(json.dumps(update, indent=2) + "\n")
print(json.dumps({"rom": update["rom"]["incremental"], "home": update.get("home", {}).get("version_code")}))
PY
# Refuse to publish anything that carries private key material: the ROM zip's
# own entries, update.json and the APK are scanned (payload.bin is filesystem
# images built from public certificates only, never the private keys).
python3 - "$ZIP" "$DIR/update.json" "$HOME_APK" <<'PY'
import re, sys, zipfile
# An armored private key: the BEGIN line followed by a base64 body. Bare labels
# such as "RSA PRIVATE KEY" appear in every PEM parser (rustls) and are not keys.
ARMOR = re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----\s*[A-Za-z0-9+/=\s]{100,}")
class _M:
    def __contains__(self, data): return ARMOR.search(data) is not None
markers = (_M(),)
bad = []
for path in sys.argv[1:]:
    try:
        if zipfile.is_zipfile(path):
            z = zipfile.ZipFile(path)
            for n in z.namelist():
                if n.endswith((".pk8", ".p12", ".jks", ".keystore")): bad.append(f"{path}:{n}")
                elif n != "payload.bin" and any(z.read(n) in m for m in markers): bad.append(f"{path}:{n}")
        else:
            if any(open(path, "rb").read() in m for m in markers): bad.append(path)
    except FileNotFoundError:
        pass
if bad:
    sys.exit("refusing to publish, private key material in: " + ", ".join(bad))
print("key check: clean")
PY
ln -sf "$ZIP" "$DIR/$ASSET_ZIP"
ASSETS=("$DIR/$ASSET_ZIP" "$DIR/update.json")
[ -f "$HOME_APK" ] && ASSETS+=("$HOME_APK")
gh release create "$TAG" -R "$REPO" --title "OctoSense $TAG" --latest \
  --notes "OctoSense ROM for the OnePlus 6 (enchilada), LineageOS 22.2 base. Installs over the air from a running OctoSense phone; first install with the web installer." \
  "${ASSETS[@]}"
echo "published: https://github.com/$REPO/releases/tag/$TAG"

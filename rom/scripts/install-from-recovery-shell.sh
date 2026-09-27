#!/bin/bash
# Installs an A/B OTA zip from the recovery's root shell instead of adb sideload,
# for links that drop mid-stream: the zip is pushed to the recovery's RAM disk in
# chunks (each retried until its size matches), then update_engine_sideload is run
# the way "Apply from ADB" runs it. Needs the OctoSense/Lineage recovery at its
# main menu with Advanced -> Enable ADB on.
#   install-from-recovery-shell.sh <zip> [serial]
set -euo pipefail
ZIP=${1:?zip}; D=${2:-cfb7c9e3}
ADB=${ADB:-$HOME/.local/share/octosense/android-tools/sdk/platform-tools/adb}
sh() { "$ADB" -s "$D" shell "$@"; }
state=$("$ADB" devices | awk -v d="$D" '$1==d{print $2}')
[ "$state" = recovery ] || { echo "phone is '$state', need 'recovery' with ADB enabled" >&2; exit 1; }
sh 'id | grep -q uid=0 || { echo "recovery shell is not root"; exit 1; }'
read -r offset props < <(python3 - "$ZIP" <<'PY'
import struct, sys, zipfile
z = zipfile.ZipFile(sys.argv[1]); i = z.getinfo("payload.bin")
f = open(sys.argv[1], "rb"); f.seek(i.header_offset); h = f.read(30)
n, e = struct.unpack("<HH", h[26:30])
props = z.read("payload_properties.txt").decode().strip().replace("\n", "\\n")
print(i.header_offset + 30 + n + e, props)
PY
)
echo "payload offset $offset"
size=$(stat -f %z "$ZIP" 2>/dev/null || stat -c %s "$ZIP")
sh 'rm -rf /tmp/ota && mkdir -p /tmp/ota'
CHUNK=$((64*1024*1024)); n=$(( (size + CHUNK - 1) / CHUNK ))
tmp=$(mktemp -d)
for ((c=0; c<n; c++)); do
    dd if="$ZIP" of="$tmp/part" bs=$CHUNK skip=$c count=1 status=none
    want=$(stat -f %z "$tmp/part" 2>/dev/null || stat -c %s "$tmp/part")
    for attempt in 1 2 3 4 5 6; do
        "$ADB" -s "$D" push "$tmp/part" "/tmp/ota/part$c" >/dev/null 2>&1 || true
        got=$(sh "stat -c %s /tmp/ota/part$c 2>/dev/null || echo 0" | tr -d '\r')
        [ "$got" = "$want" ] && break
        sleep 2; "$ADB" kill-server >/dev/null 2>&1 || true; sleep 1
    done
    [ "$got" = "$want" ] || { echo "chunk $c never arrived" >&2; exit 1; }
    printf 'chunk %d/%d ok\r' "$((c+1))" "$n"
done
echo
sh "cd /tmp/ota && cat \$(ls part* | sort -t t -k2 -n) > /tmp/update.zip && rm -f part* && stat -c %s /tmp/update.zip"
sh "sha256sum /tmp/update.zip | cut -c1-64"; shasum -a 256 "$ZIP" | cut -c1-64
echo "installing..."
sh "update_engine_sideload --payload=file:///tmp/update.zip --offset=$offset --headers=\"\$(printf '$props')\" 2>&1 | tail -5"
echo "done; now Factory reset -> Format data on the phone, then Reboot system now"

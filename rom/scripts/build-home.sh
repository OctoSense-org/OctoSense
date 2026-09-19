#!/bin/bash
# Build the OctoSense Home app (OctoSense-mobile) and sign it with the ROM's
# platform key, so it installs over the ROM's copy as an updated system app.
#   build-home.sh [OctoSense-mobile checkout]   -> rom-builds/home/OctoSenseHome-<versionCode>.apk
# Version code is YYYYMMDDHH (cargo-makepad --version-code=auto): newer builds
# always win, and a later ROM carrying a newer copy wins back.
set -euo pipefail
MOBILE=${1:-$HOME/home/octosense-org/OctoSense-mobile-ux}
KEYS=$HOME/home/ssh-key/octosense-rom-keys
TOOLS=$HOME/.local/share/octosense/android-tools
OUT=$HOME/home/octosense-org/rom-builds/home; mkdir -p "$OUT"
export JAVA_HOME=$TOOLS/makepad-android/openjdk; export PATH=$JAVA_HOME/bin:$PATH
# Keep the builder's home directory (and so their name) out of the binaries:
# panic messages and debug paths otherwise embed /Users/<name>/... verbatim.
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME/.cargo=/cargo --remap-path-prefix=$HOME=/build"
# Build from a directory outside the builder's home: Makepad compiles each
# crate's directory into its resource paths (CARGO_MANIFEST_DIR), which no
# compiler flag remaps. Sources and the Cargo cache are mirrored there
# (APFS clones the first time, rsync after) so the APK carries no user name.
ROOT=${OCTOSENSE_BUILD_ROOT:-/Users/Shared/octosense-build}
SRC=$(cd "$MOBILE/.." && pwd)
mkdir -p "$ROOT"
if [ ! -d "$ROOT/cargo" ]; then mkdir -p "$ROOT/cargo" && cp -c -R "$HOME/.cargo/registry" "$HOME/.cargo/git" "$ROOT/cargo/"; fi
for repo in "$(basename "$MOBILE")" makepad-pinned octoscript-makepad-repin octoscript-pinned Octosense-Service-AppCards; do
  if [ ! -d "$ROOT/$repo" ]; then cp -c -R "$SRC/$repo" "$ROOT/$repo" && rm -rf "$ROOT/$repo/target" "$ROOT/$repo/.git"
  else rsync -a --delete --exclude /target --exclude .git "$SRC/$repo/" "$ROOT/$repo/"; fi
done
# Octoscript-AppCard's cards include_str! files from a sibling "octoscript-makepad".
ln -sfn octoscript-makepad-repin "$ROOT/octoscript-makepad"
# ...and splash.md from a sibling "makepad".
ln -sfn makepad-pinned "$ROOT/makepad"
# Octoscript-AppCard's design kits carry a designer's local font paths
# ("file:/Users/<name>/Library/Fonts/..."); strip the directory in the mirror's copy.
find "$ROOT/cargo/git/checkouts" -path '*octoscript-appcard*' -path '*/lab/core/kits/*' -name '*.json' \
  -exec grep -l 'file:/Users/' {} + 2>/dev/null | while read -r f; do sed -i '' -E 's#file:/Users/[^/"]+/Library/Fonts/#file:#g' "$f"; done
export CARGO_HOME=$ROOT/cargo
cd "$ROOT/$(basename "$MOBILE")"
sed -i '' -e 's|path = "\.\./makepad/|path = "../makepad-pinned/|g' -e 's|path = "\.\./octoscript-makepad/|path = "../octoscript-makepad-repin/|g' -e 's|path = "\.\./octoscript/|path = "../octoscript-pinned/|g' Cargo.toml
"$SRC/makepad/target/release/cargo-makepad" makepad android --sdk-path="$TOOLS/makepad-android" --version-code=auto build -p octosense --release 2>&1 | tail -3
APK=target/android/makepad-android-apk/octosense/apk/octo_sense.apk
VC=$("$TOOLS/sdk/build-tools/35.0.0/aapt2" dump badging "$APK" 2>/dev/null | sed -n "s/.*versionCode='\([0-9]*\)'.*/\1/p")
"$TOOLS/sdk/build-tools/35.0.0/apksigner" sign --key "$KEYS/platform.pk8" --cert "$KEYS/platform.x509.pem" --out "$OUT/OctoSenseHome-$VC.apk" "$APK"
ln -sf "OctoSenseHome-$VC.apk" "$OUT/latest.apk"
echo "$OUT/OctoSenseHome-$VC.apk (versionCode $VC, platform-signed)"

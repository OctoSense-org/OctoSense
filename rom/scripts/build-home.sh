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
cd "$MOBILE"
# Build against the pinned sibling worktrees (see the UX baseline notes); restore Cargo.toml after.
cp Cargo.toml /tmp/Cargo.toml.octosense-home
trap 'cp /tmp/Cargo.toml.octosense-home "$MOBILE/Cargo.toml"' EXIT
sed -i '' -e 's|path = "\.\./makepad/|path = "../makepad-pinned/|g' -e 's|path = "\.\./octoscript-makepad/|path = "../octoscript-makepad-repin/|g' -e 's|path = "\.\./octoscript/|path = "../octoscript-pinned/|g' Cargo.toml
../makepad/target/release/cargo-makepad makepad android --sdk-path="$TOOLS/makepad-android" --version-code=auto build -p octosense --release 2>&1 | tail -3
APK=target/android/makepad-android-apk/octosense/apk/octo_sense.apk
VC=$("$TOOLS/sdk/build-tools/35.0.0/aapt2" dump badging "$APK" 2>/dev/null | sed -n "s/.*versionCode='\([0-9]*\)'.*/\1/p")
"$TOOLS/sdk/build-tools/35.0.0/apksigner" sign --key "$KEYS/platform.pk8" --cert "$KEYS/platform.x509.pem" --out "$OUT/OctoSenseHome-$VC.apk" "$APK"
ln -sf "OctoSenseHome-$VC.apk" "$OUT/latest.apk"
echo "$OUT/OctoSenseHome-$VC.apk (versionCode $VC, platform-signed)"

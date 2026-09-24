#!/bin/bash
# OctoSense ROM build for the OnePlus 6 (enchilada), run inside the build chroot.
#   preflight : lunch and dump the product configuration (fast)
#   bacon     : full signed build; the flashable zip lands in /exports/rom-build
set -eo pipefail
cd /build
export TARGET_RELEASE=bp1a
export OUT_DIR=out/octosense-rom
export GOMAXPROCS=16
export GOGC=100
export GOMEMLIMIT=48GiB
# Unique per build: incremental builds otherwise keep the first build's date and
# version string, and two builds become indistinguishable on the phone.
export BUILD_NUMBER=octosense-$(date -u +%Y%m%d%H%M)
export BUILD_DATETIME=$(date +%s)
export LINEAGE_BUILDTYPE=UNOFFICIAL
source build/envsetup.sh
lunch lineage_enchilada-bp1a-userdebug
build/soong/soong_ui.bash --dumpvars-mode \
    --vars="PLATFORM_VERSION PLATFORM_SDK_VERSION BUILD_ID TARGET_PRODUCT TARGET_DEVICE LINEAGE_BUILD LINEAGE_VERSION PRODUCT_DEFAULT_DEV_CERTIFICATE TARGET_KERNEL_SOURCE" \
    > /exports/rom-build/product-configuration.txt
cat /exports/rom-build/product-configuration.txt
case "$1" in
    preflight) exit 0 ;;
    bacon) ;;
    module) shift; m -j64 "$@" 2>&1 | tail -c 400000 > /exports/rom-build/module.log; echo "module build exit ${PIPESTATUS[0]}" >> /exports/rom-build/module.log; exit 0 ;;
    *) echo 'Use preflight or bacon' >&2; exit 2 ;;
esac
date -u +%FT%TZ > /exports/rom-build/started.txt
rm -f /exports/rom-build/finished.txt
rom_version=$(get_build_var LINEAGE_VERSION)
[[ "$rom_version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || { echo 'Invalid ROM version for export' >&2; exit 1; }
rom_zip="$OUT_DIR/target/product/enchilada/lineage-$rom_version.zip"
m -j64 bacon 2>&1 | tail -c 4000000 > /exports/rom-build/bacon.log
[ -f "$rom_zip" ] && [ "$rom_zip" -nt /exports/rom-build/started.txt ] || { echo 'No current ROM zip was produced by this build; refusing stale artifacts' >&2; exit 1; }
ls -la "$OUT_DIR"/target/product/enchilada/*.zip "$OUT_DIR"/target/product/enchilada/*.img 2>/dev/null | tee /exports/rom-build/artifacts.txt
# Only the zip this build produced: the product dir keeps older zips under other date names.
rm -f /exports/rom-build/lineage-*.zip
cp "$rom_zip" /exports/rom-build/
sha256sum /exports/rom-build/*.zip > /exports/rom-build/zip.sha256
date -u +%FT%TZ > /exports/rom-build/finished.txt

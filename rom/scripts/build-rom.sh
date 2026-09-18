#!/bin/bash
# OctoSense ROM build for the OnePlus 6 (enchilada), run inside the build chroot.
#   preflight : lunch and dump the product configuration (fast)
#   bacon     : full signed build; the flashable zip lands in /exports/rom-build
set -e
cd /build
export TARGET_RELEASE=bp1a
export OUT_DIR=out/octosense-rom
export GOMAXPROCS=16
export GOGC=100
export GOMEMLIMIT=48GiB
export BUILD_NUMBER=octosense-rom
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
m -j64 bacon 2>&1 | tail -c 4000000 > /exports/rom-build/bacon.log
ls -la "$OUT_DIR"/target/product/enchilada/*.zip "$OUT_DIR"/target/product/enchilada/*.img 2>/dev/null | tee /exports/rom-build/artifacts.txt
cp "$OUT_DIR"/target/product/enchilada/lineage-*.zip /exports/rom-build/ 2>/dev/null || true
sha256sum /exports/rom-build/*.zip > /exports/rom-build/zip.sha256 2>/dev/null || true
date -u +%FT%TZ > /exports/rom-build/finished.txt

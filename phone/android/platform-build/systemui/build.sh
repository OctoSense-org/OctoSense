#!/bin/bash
set -euo pipefail
cd /build
python3 /exports/apply-rom-go-memory-controls.py
python3 /exports/systemui-source/android/platform-build/systemui/verify-source.py
export TARGET_PRODUCT=lineage_gsi_arm64 TARGET_RELEASE=bp1a TARGET_BUILD_VARIANT=userdebug
export OUT_DIR=out/octosense-platform GOMAXPROCS=16 GOGC=100 GOMEMLIMIT=48GiB
export BUILD_NUMBER=octosense-systemui WITH_DEXPREOPT=false
# Android's envsetup probes unset optional shell variables.
set +u
source build/envsetup.sh
lunch lineage_gsi_arm64-bp1a-userdebug
build/soong/soong_ui.bash --dumpvars-mode \
  --vars="PLATFORM_VERSION PLATFORM_SDK_VERSION PLATFORM_VERSION_CODENAME RELEASE_ACONFIG_VALUE_SETS BUILD_ID TARGET_PRODUCT LINEAGE_BUILD" \
  > /exports/systemui-build/product-configuration.txt
python3 /exports/verify-product.py /exports/systemui-build/product-configuration.txt
build/soong/soong_ui.bash --make-mode --soong-only -j32 OctoSenseSystemUI
python3 - <<'PY'
import hashlib,json,shutil
from pathlib import Path
root=Path('/exports/systemui-build')
config=json.loads(Path('out/octosense-platform/soong/dexpreopt.config').read_text())
assert config['DisablePreopt'] and config['DisablePreoptBootImages']
(root/'dexpreopt-configuration.json').write_text(json.dumps(config,indent=2)+'\n')
source=Path('out/octosense-platform/soong/target/product/generic_arm64/system/system_ext/priv-app/OctoSenseSystemUI/OctoSenseSystemUI.apk')
assert source.is_file()
shutil.copy2(source,root/'OctoSense-SystemUI.apk')
(root/'apk-sha256.txt').write_text(hashlib.sha256(source.read_bytes()).hexdigest()+'\n')
PY

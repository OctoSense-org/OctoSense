#!/bin/bash
# Installs this repository's vendor/octosense into a LineageOS tree and makes the
# OnePlus 6 product inherit it. Usage: apply-to-tree.sh <tree>
set -euo pipefail
TREE=${1:?tree}
HERE=$(cd "$(dirname "$0")/.." && pwd)
rsync -a --delete "$HERE/vendor/octosense/" "$TREE/vendor/octosense/"
PRODUCT="$TREE/device/oneplus/enchilada/lineage_enchilada.mk"
grep -q 'vendor/octosense/octosense.mk' "$PRODUCT" || printf '\n# OctoSense ROM layer\n$(call inherit-product, vendor/octosense/octosense.mk)\n' >> "$PRODUCT"
echo "applied to $TREE"

python3 "$HERE/patches/lineage-kernel-out-prefix.py" "$TREE"
python3 "$HERE/patches/lineage-no-trebuchet.py" "$TREE"
python3 "$HERE/patches/aosp-no-launcher3-phony.py" "$TREE"
BOARD="$TREE/device/oneplus/enchilada/BoardConfig.mk"
grep -q 'vendor/octosense/BoardConfigOctoSense.mk' "$BOARD" || printf '\n# OctoSense ROM board additions\n-include vendor/octosense/BoardConfigOctoSense.mk\n' >> "$BOARD"
echo "board include: ok"

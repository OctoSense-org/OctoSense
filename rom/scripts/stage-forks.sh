#!/bin/bash
# On the host: put the OctoSense layer and the Quickstep and SystemUI forks into the
# tree for a ROM build. Run only while no build is active.
#   stage-forks.sh <tree> <octosense-rom checkout> <OctoSense-mobile checkout>
set -euo pipefail
TREE=${1:?tree}; ROM=${2:?octosense-rom}; MOBILE=${3:?OctoSense-mobile}
systemctl is-active --quiet octosense-rom-bacon && { echo "a ROM build is running" >&2; exit 1; }
bash "$ROM/scripts/apply-to-tree.sh" "$TREE"
python3 "$MOBILE/android/platform-build/stage-quickstep.py" --tree "$TREE" "${STAGE_QUICKSTEP_ARGS[@]:-}"
python3 "$MOBILE/android/platform-build/stage-systemui.py" --tree "$TREE"
echo "staged"

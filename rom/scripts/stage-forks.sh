#!/bin/bash
# On the host: put the OctoSense layer and the Quickstep and SystemUI forks into the
# tree for a ROM build. Run only while no build is active.
#   stage-forks.sh <tree> <octosense-rom checkout> <OctoSense-mobile checkout>
# The stagers refuse a tree whose fork files are neither pristine nor the expected
# bytes, so an earlier staging (another palette, say) is reset to HEAD first.
set -euo pipefail
TREE=${1:?tree}; ROM=${2:?octosense-rom}; MOBILE=${3:?OctoSense-mobile}
# The Quickstep stager insists on the record of the upstream Quickstep build it
# was reviewed against (quickstep-result.json beside upstream-TrebuchetQuickStep.apk).
BASELINE=${QUICKSTEP_BASELINE:-$HOME/octosense-adr0001/exports/upstream-build/quickstep-result.json}
systemctl is-active --quiet octosense-rom-bacon && { echo "a ROM build is running" >&2; exit 1; }
reset_fork() { # <repo> <paths...>: drop earlier staged edits under the given paths only
    local repo=$1 path; shift
    for path in "$@"; do
        # One path at a time: git checkout refuses the whole list when one path is untracked.
        git -C "$repo" checkout -q -- "$path" 2>/dev/null || true
        git -C "$repo" clean -qfd -- "$path"
    done
}
reset_fork "$TREE/frameworks/base" packages/SystemUI
reset_fork "$TREE/packages/apps/Trebuchet" Android.bp octosense
bash "$ROM/scripts/apply-to-tree.sh" "$TREE"
python3 "$MOBILE/android/platform-build/stage-quickstep.py" --tree "$TREE" --baseline-result "$BASELINE"
python3 "$MOBILE/android/platform-build/stage-systemui.py" --tree "$TREE" --report "$TREE/out/octosense-rom/systemui-stage.json"
python3 "$MOBILE/android/platform-build/stage-quickstep.py" --tree "$TREE" --baseline-result "$BASELINE" --verify
python3 "$MOBILE/android/platform-build/stage-systemui.py" --tree "$TREE" --report "$TREE/out/octosense-rom/systemui-stage.json" --verify
echo "staged"

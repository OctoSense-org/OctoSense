#!/usr/bin/env bash
# Merge a pull request on a local CI pass (tools/ci_local_merge.py; docs/local-ci.md):
#
#   tools/ci-local.sh --only all            # on the PR's head, rebased on main
#   tools/ci-local-merge.sh <PR number>     # [--dry-run] [--fixes-main]
#
# Refuses a stale, failed or unexpectedly skipped run; then comments the
# summary on the PR and merges with --admin and a `[skip ci]` subject.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 "$here/ci_local_merge.py" "$@"

#!/usr/bin/env bash
# The dependency-graph guards every OctoSense shell build must pass (CI runs
# this for desktop/ and phone/; run it locally the same way):
#
#   tools/check-shell-graph.sh [-p <shell package>] [--manifest-path <Cargo.toml>]
#                              [--features-set "<f>"]... [--target <triple>]...
#
# e.g. `tools/check-shell-graph.sh -p octosense` (desktop/) or
# `-p octosense-home` (phone/) in the one workspace. Defaults: the current
# directory's package; feature sets "" (default)
# and "mobile-apps"; targets host and aarch64-linux-android. For every
# feature set and target, with --locked:
#
# - the shell's AI services are linked: octosense-ai-host, the octos kernel
#   (octosense-kernel, formerly octosense-octos-core) and octosense-app-peers;
# - with App Hub linked, so are the host services its Card runner offers the
#   system apps (octosense-mail-service, octosense-news-service);
# - AppCard's UI (octosense-appcard) is NOT linked without `app-appcard`;
# - hosted Rinx is the library module only (feature "octosense-module"),
#   never its standalone entry or a kernel of its own (Rinx ADR 0007);
# - one makepad, one App Hub, one octos: a single makepad-widgets /
#   makepad-platform, a single octosense-appstore and octosense-app-hub-app,
#   and every octos-* crate from one octos-org/octos revision.
#
# Exits non-zero with a GitHub `::error::` line on the first violation.
set -euo pipefail

manifest=()
package=()
feature_sets=()
targets=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest-path) manifest=(--manifest-path "$2"); shift 2 ;;
    -p|--package) package=(-p "$2"); shift 2 ;;
    --features-set) feature_sets+=("$2"); shift 2 ;;
    --target) targets+=("$2"); shift 2 ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[[ ${#feature_sets[@]} -gt 0 ]] || feature_sets=("" "mobile-apps")
[[ ${#targets[@]} -gt 0 ]] || targets=("" "aarch64-linux-android")

fail() { echo "::error::$*"; exit 1; }

# Whether package $1 is in the graph (0), absent (1).
linked() {
  local pkg=$1; shift
  local out
  if out=$(cargo tree "$@" -i "$pkg" --depth 0 2>&1); then
    return 0
  fi
  grep -q "did not match any packages" <<<"$out" && return 1
  # Several versions of one package: in the graph, and ambiguous.
  grep -q "There are multiple" <<<"$out" && return 0
  echo "$out" >&2
  fail "cargo tree failed for $pkg ($*)"
}

# Package $1 must resolve to exactly one package (one source, one version).
single() {
  local pkg=$1; shift
  local out
  out=$(cargo tree "$@" -i "$pkg" --depth 0 2>&1 || true)
  if grep -q "There are multiple" <<<"$out"; then
    echo "$out" >&2
    fail "more than one $pkg in the graph ($*)"
  fi
}

kernel_pkg=""
for features in "${feature_sets[@]}"; do
  for target in "${targets[@]}"; do
    # `cargo tree` flags for this feature set and target ("" = default / host).
    args=(--locked ${manifest[@]+"${manifest[@]}"} ${package[@]+"${package[@]}"})
    [[ -n $features ]] && args+=(--features "$features")
    [[ -n $target ]] && args+=(--target "$target")
    where="features='${features}' target='${target:-host}'"
    if [[ -z $kernel_pkg ]]; then
      if linked octosense-kernel "${args[@]}"; then kernel_pkg=octosense-kernel
      elif linked octosense-octos-core "${args[@]}"; then kernel_pkg=octosense-octos-core
      else fail "the octos kernel (octosense-kernel) is missing ($where)"; fi
    fi
    for pkg in octosense-ai-host "$kernel_pkg" octosense-app-peers; do
      linked "$pkg" "${args[@]}" || fail "$pkg is missing ($where)"
    done
    if linked octosense-app-hub-app "${args[@]}"; then
      for pkg in octosense-mail-service octosense-news-service; do
        linked "$pkg" "${args[@]}" || fail "$pkg is missing with App Hub ($where)"
      done
    fi
    if [[ ",$features," != *",app-appcard,"* ]] && linked octosense-appcard "${args[@]}"; then
      fail "octosense-appcard is linked without app-appcard ($where)"
    fi
    if linked rinx "${args[@]}"; then
      tree=$(cargo tree "${args[@]}" -e features -i rinx)
      grep -q 'rinx feature "octosense-module"' <<<"$tree" || fail "rinx is not linked as a module ($where)"
      if grep -qE 'rinx feature "(standalone|octos-local|octos-remote)"' <<<"$tree"; then
        fail "hosted rinx pulls a standalone runtime ($where)"
      fi
    fi
    for pkg in makepad-widgets makepad-platform octosense-appstore octosense-app-hub-app; do
      single "$pkg" "${args[@]}"
    done
    echo "ok: $where (kernel: $kernel_pkg)"
  done
done

# One octos: every octos-* crate in the lock from one octos-org/octos rev
# (the `nix` patch taken from the octos repo is not an octos crate).
lock_dir=$(dirname "$(cargo locate-project ${manifest[@]+"${manifest[@]}"} --workspace --message-format plain)")
revs=$(awk '/^name = "octos-/{octos=1; next} /^name = /{octos=0} octos && /^source = "git\+https:\/\/github.com\/octos-org\/octos/{print}' "$lock_dir/Cargo.lock" \
  | sed -E 's/.*[?&]rev=([0-9a-f]+).*/\1/' | sort -u)
if [[ $(grep -c . <<<"$revs") -gt 1 ]]; then
  fail "more than one octos revision in Cargo.lock: $(tr '\n' ' ' <<<"$revs")"
fi
echo "ok: one octos (${revs:-none linked})"

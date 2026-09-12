#!/usr/bin/env bash
# Build MakeOS for an OpenHarmony / HarmonyOS phone and run it over hdc.
#
#   scripts/ohos.sh deveco   cross-compile and generate the DevEco project
#                            (target/makepad-open-harmony/makeos)
#   scripts/ohos.sh build    cross-compile and package the HAP
#   scripts/ohos.sh run      build, then install and launch on the device
#   scripts/ohos.sh hilog    follow the app's log on the device
#
# What it needs:
#   - DevEco Studio: its SDK, node, hvigor and hdc.  DEVECO_HOME, default
#     /Applications/DevEco-Studio.app/Contents
#   - the sibling ../guofoo-makepad checkout on its `makeos-ohos` branch: the
#     pinned revision plus the OpenHarmony cfg gates makepad-platform lacks.
#     MAKEPAD_SIBLING overrides the path.  The build patches every makepad
#     crate to that checkout (target/ohos/patch.toml); desktop builds are
#     untouched, and Cargo.lock is restored afterwards.
#   - cargo-makepad built from that checkout, so its DevEco template matches
#     the platform code:
#       cargo install --path ../guofoo-makepad/tools/cargo_makepad --root ~/ohos-sdk/cargo-makepad-guofoo
#     CARGO_MAKEPAD names the binary (default: that install).
#   - nightly Rust with the aarch64-unknown-linux-ohos target
#     (`cargo makepad ohos install-toolchain`).
#   - signing material for a commercial HarmonyOS phone: DevEco's
#     "Automatically generate signature" (needs a Huawei ID) writes a
#     signingConfigs block bound to one bundle name.  Point
#     MAKEOS_OHOS_SIGNING at that build-profile.json5 (or a saved copy of the
#     block) and MAKEOS_OHOS_BUNDLE at the bundle it is bound to.  Without it
#     the HAP is left unsigned.
#
#   MAKEOS_OHOS_FEATURES  cargo features; default `all-apps` links every
#                         catalog app in-process (app-<id> features), since a
#                         phone cannot host child processes.  app-aichat also
#                         builds, but the assistant pane it seats is a desktop
#                         surface.
#   HDC_TARGET            device serial when several are connected.
#   MAKEOS_OHOS_GPU       `vulkan` (the default) renders through makepad's
#                         Vulkan backend on the phone's VK_OHOS_surface (cfg
#                         use_vulkan, via the MAKEPAD env the platform's
#                         build.rs reads); `gl` keeps OpenGL ES. The first run
#                         of a Vulkan build compiles every pipeline it meets
#                         (seconds per app on this driver); they persist in the
#                         app's data dir and later runs build none.
#   MAKEOS_OHOS_REMOTE    bind makepad's `--remote` control surface at launch,
#                         e.g. 0.0.0.0:8399, then drive the app over the
#                         phone's IP: curl http://<phone>:8399/  (the routes)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SIBLING="$(cd "${MAKEPAD_SIBLING:-$ROOT/../guofoo-makepad}" 2>/dev/null && pwd || true)"
DEVECO_HOME="${DEVECO_HOME:-/Applications/DevEco-Studio.app/Contents}"
CARGO_MAKEPAD="${CARGO_MAKEPAD:-$HOME/ohos-sdk/cargo-makepad-guofoo/bin/cargo-makepad}"
FEATURES="${MAKEOS_OHOS_FEATURES:-all-apps}"
# The platform crate's build.rs turns MAKEPAD=vulkan into cfg(use_vulkan);
# cargo-makepad forwards MAKEPAD into the cross build.
if [ "${MAKEOS_OHOS_GPU:-vulkan}" = "vulkan" ]; then
    export MAKEPAD="${MAKEPAD:+$MAKEPAD+}vulkan"
fi
HDC="${HDC:-$DEVECO_HOME/sdk/default/openharmony/toolchains/hdc}"
PRJ="$ROOT/target/makepad-open-harmony/makeos"
PATCH="$ROOT/target/ohos/patch.toml"
HAP="$PRJ/entry/build/default/outputs/default/makepad-default-signed.hap"
UNSIGNED="$PRJ/entry/build/default/outputs/default/makepad-default-unsigned.hap"

cmd="${1:-build}"

die() { echo "ohos.sh: $*" >&2; exit 1; }

[ -d "$SIBLING/platform" ] || die "sibling makepad checkout not found (MAKEPAD_SIBLING or ../guofoo-makepad)"
[ -x "$CARGO_MAKEPAD" ] || die "cargo-makepad not found at $CARGO_MAKEPAD (CARGO_MAKEPAD)"
[ -d "$DEVECO_HOME/sdk/default/openharmony" ] || die "no OpenHarmony SDK under DEVECO_HOME=$DEVECO_HOME"

# hvigor is Java and DevEco bundles a runtime; hvigor also wants the SDK laid
# out as <base>/<api>/<component>, which OHOS_BASE_SDK_HOME supplies when set.
export JAVA_HOME="${JAVA_HOME:-$DEVECO_HOME/jbr/Contents/Home}"
export PATH="$JAVA_HOME/bin:$PATH"

hdc() {
    if [ -n "${HDC_TARGET:-}" ]; then "$HDC" -t "$HDC_TARGET" "$@"; else "$HDC" "$@"; fi
}

# The [patch] that swaps every makepad crate in Cargo.lock for the sibling's
# copy.  Only workspace members are listed: the crates makepad vendors are
# reached through their path dependencies and follow along.
gen_patch() {
    mkdir -p "$(dirname "$PATCH")"
    python3 - "$ROOT/Cargo.lock" "$SIBLING" "$PATCH" <<'PY'
import json, os, re, subprocess, sys
lock, sibling, out = sys.argv[1:4]
names = set()
for block in open(lock).read().split("[[package]]"):
    n = re.search(r'^name = "([^"]+)"', block, re.M)
    if n and re.search(r'^source = "git\+https://github.com/guofoo/makepad\.git', block, re.M):
        names.add(n.group(1))
meta = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=sibling))
lines = ["# Generated by scripts/ohos.sh: build against the sibling makepad checkout.",
         '[patch."https://github.com/guofoo/makepad.git"]']
count = 0
for p in meta["packages"]:
    if p["name"] in names:
        lines.append(f'{p["name"]} = {{ path = "{os.path.dirname(p["manifest_path"])}" }}')
        count += 1
open(out, "w").write("\n".join(lines) + "\n")
print(f"  {count} makepad crates patched to {sibling}")
PY
}

# Cargo rewrites the lockfile for the patched sources; keep the desktop one.
LOCK_BACKUP="$ROOT/target/ohos/Cargo.lock.desktop"
save_lock() { mkdir -p "$ROOT/target/ohos"; cp "$ROOT/Cargo.lock" "$LOCK_BACKUP"; }
restore_lock() { [ -f "$LOCK_BACKUP" ] && cp "$LOCK_BACKUP" "$ROOT/Cargo.lock"; }

cargo_args=(-p makeos --release --features "$FEATURES" --config "$PATCH")

adapt_project() {
    DEVECO_HOME="$DEVECO_HOME" python3 "$ROOT/scripts/ohos_project.py" "$PRJ"
}

do_deveco() {
    echo "==> generating the DevEco project"
    gen_patch
    save_lock; trap restore_lock EXIT
    (cd "$ROOT" && "$CARGO_MAKEPAD" ohos --deveco-home="$DEVECO_HOME" deveco "${cargo_args[@]}")
    adapt_project
}

do_build() {
    [ -f "$PRJ/build-profile.json5" ] || do_deveco
    echo "==> building the HAP"
    gen_patch
    save_lock; trap restore_lock EXIT
    adapt_project
    (cd "$ROOT" && "$CARGO_MAKEPAD" ohos --deveco-home="$DEVECO_HOME" build "${cargo_args[@]}")
    if [ -f "$HAP" ]; then
        ls -lh "$HAP"
    elif [ -f "$UNSIGNED" ]; then
        ls -lh "$UNSIGNED"
        echo "    unsigned only: set MAKEOS_OHOS_SIGNING (see the header) to install on a device"
    else
        die "hvigor produced no HAP under $PRJ/entry/build"
    fi
}

bundle_name() {
    python3 - "$PRJ/AppScope/app.json5" <<'PY'
import json, re, sys
raw = open(sys.argv[1]).read()
print(json.loads(re.sub(r",(\s*[}\]])", r"\1", re.sub(r"//[^\n]*", "", raw)))["app"]["bundleName"])
PY
}

do_run() {
    do_build
    [ -f "$HAP" ] || die "no signed HAP to install"
    local bundle; bundle="$(bundle_name)"
    echo "==> installing $bundle"
    hdc shell "aa force-stop $bundle" >/dev/null 2>&1 || true
    hdc shell "rm -rf data/local/tmp/makeos; mkdir -p data/local/tmp/makeos" >/dev/null
    hdc file send "$HAP" data/local/tmp/makeos >/dev/null
    local out; out="$(hdc shell "bm install -p data/local/tmp/makeos" | tr -d '\r')"
    echo "    $out"
    case "$out" in
        *"install entry already exist"*|*9568267*|*"version downgrade"*)
            # Another module name (a DevEco sample uses "entry"; makepad's
            # template is "makepad") or a higher versionCode is installed
            # under this bundle: replace it.
            echo "    replacing the app already installed under $bundle"
            hdc shell "bm uninstall -n $bundle" | tr -d '\r'
            hdc shell "bm install -p data/local/tmp/makeos" | tr -d '\r'
            ;;
    esac
    hdc shell "rm -rf data/local/tmp/makeos" >/dev/null
    echo "==> launching"
    local params=""
    [ -n "${MAKEOS_OHOS_REMOTE:-}" ] && params="--ps makepad.MAKEPAD_REMOTE $MAKEOS_OHOS_REMOTE"
    hdc shell "aa start -a EntryAbility -b $bundle $params" | tr -d '\r'
    echo "log: scripts/ohos.sh hilog"
}

do_hilog() {
    hdc shell "hilog -z 400 | grep -iE 'makepad|makeos'" | tr -d '\r'
}

case "$cmd" in
    deveco) do_deveco ;;
    build) do_build ;;
    run) do_run ;;
    hilog) do_hilog ;;
    *) die "unknown command '$cmd' (deveco | build | run | hilog)" ;;
esac

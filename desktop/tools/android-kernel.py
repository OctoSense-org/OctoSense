#!/usr/bin/env python3
"""Bundle OctoSense's octos kernel into an Android APK (Python 3.9+).

The octos kernel is a shell service (feature `octos-core`, on in every
Android build): at run time it execs `liboctos.so serve --stdio` from the
APK's native lib dir, the only place an Android app may exec a binary from.
This tool cross-builds that binary from the one octos revision `Cargo.lock`
pins and runs the packager with it bundled:

  python3 tools/android-kernel.py --sdk <cargo-makepad Android SDK dir> \\
      -- cargo makepad android run -p octosense --release

It checks octos out into `target/octos-kernel/src` (never a sibling you may
be working in), builds `octos` for aarch64-linux-android with the SDK's NDK
clang (API 33, `--no-default-features --features api,git,ast`) and runs the
command after `--` with `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>`.
`--kernel <path>` bundles a prebuilt aarch64-linux-android `octos` instead;
with no command it only builds and prints the kernel's path.
"""
import argparse
import glob
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OCTOS_URL = "https://github.com/octos-org/octos.git"
TARGET = "aarch64-linux-android"
API = "33"
KERNEL_BUILD = ["-p", "octos-cli", "--bin", "octos", "--no-default-features", "--features", "api,git,ast"]


def octos_revision(lock=None):
    """The one octos revision OctoSense links (the workspace Cargo.lock)."""
    text = (lock or ROOT.parent / "Cargo.lock").read_text()
    match = re.search(r'name = "octos-cli"\nversion = "[^"]+"\nsource = "git\+https://github\.com/octos-org/octos\.git\?rev=([0-9a-f]{40})#', text)
    if not match:
        raise RuntimeError("Cargo.lock names no octos-cli from octos-org/octos: cannot tell which kernel to build")
    return match[1]


def ndk_bin(sdk):
    """The newest NDK's LLVM bin dir inside a cargo-makepad Android SDK dir."""
    found = sorted(glob.glob(str(sdk / "ndk/*/toolchains/llvm/prebuilt/*/bin")),
                   key=lambda p: [int(x) if x.isdigit() else x for x in re.split(r"[./]", p)])
    if not found:
        raise RuntimeError(f"No NDK under {sdk}/ndk (cargo makepad android install-toolchain puts one there)")
    return Path(found[-1])


def build_env(bin_dir):
    clang = bin_dir / f"{TARGET}{API}-clang"
    return {"CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER": str(clang),
            "CARGO_TARGET_AARCH64_LINUX_ANDROID_AR": str(bin_dir / "llvm-ar"),
            "CC_aarch64_linux_android": str(clang),
            "CXX_aarch64_linux_android": str(bin_dir / f"{TARGET}{API}-clang++"),
            "AR_aarch64_linux_android": str(bin_dir / "llvm-ar"),
            "RANLIB_aarch64_linux_android": str(bin_dir / "llvm-ranlib")}


def plan(revision, work, offline=False):
    """(cwd, argv) steps that check out and build the kernel."""
    src = work / "src"
    steps = [(ROOT, ["git", "init", "--quiet", str(src)])]
    if not offline:
        steps.append((src, ["git", "fetch", "--quiet", "--no-tags", "--depth=1", OCTOS_URL, revision]))
    steps.append((src, ["git", "checkout", "--quiet", "--detach", revision]))
    build = ["cargo", "build", "--locked", "--release", "--target", TARGET, *KERNEL_BUILD]
    if offline:
        build.append("--offline")
    steps.append((src, build))
    return steps, work / "target" / TARGET / "release/octos"


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    command = []
    if "--" in argv:
        i = argv.index("--")
        argv, command = argv[:i], argv[i + 1:]
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--sdk", type=Path, help="cargo-makepad's Android SDK dir (holds ndk/)")
    p.add_argument("--kernel", type=Path, help="A prebuilt aarch64-linux-android octos to bundle")
    p.add_argument("--offline", action="store_true")
    args = p.parse_args(argv)
    if args.kernel:
        kernel = args.kernel.resolve()
        if not kernel.is_file():
            p.error(f"no such kernel: {kernel}")
    else:
        if not args.sdk:
            p.error("--sdk is required to build the kernel (or pass --kernel)")
        work = ROOT / "target/octos-kernel"
        steps, kernel = plan(octos_revision(), work, args.offline)
        env = dict(os.environ, **build_env(ndk_bin(args.sdk.resolve())))
        env["CARGO_TARGET_DIR"] = str(work / "target")
        for cwd, step in steps:
            subprocess.run(step, cwd=cwd, env=env, check=True)
    print(f"octos kernel: {kernel}", flush=True)
    if command:
        env = dict(os.environ, MAKEPAD_ANDROID_EXTRA_LIBS=f"liboctos.so={kernel}")
        sys.exit(subprocess.run(command, cwd=ROOT, env=env).returncode)


if __name__ == "__main__":
    main()

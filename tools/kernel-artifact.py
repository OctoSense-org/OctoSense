#!/usr/bin/env python3
"""The octos kernel an OctoSense Android APK bundles (Python 3.9+).

The octos kernel is a shell service (octosense-ai-host, feature
`octos-core`; always on Android): at run time the shell execs
`liboctos.so serve --stdio` from the APK's native lib dir, the only place an
Android app may exec a binary from. This tool produces that artifact for
every shell build (desktop/ and phone/ alike):

- the octos revision is the ONE the workspace's Cargo.lock pins (octos-cli
  from octos-org/octos);
- by default it checks that revision out into a private work dir (never a
  sibling checkout you may be working in) and cross-builds `octos` for
  aarch64-linux-android with the cargo-makepad SDK's NDK clang (API 33,
  `--no-default-features --features api,git,ast`: the stdio server without
  the llama.cpp embedder);
- `--kernel <path>` takes a prebuilt aarch64-linux-android `octos` instead;
- it records what it produced (source, revision, sha256) with `--receipt`;
- with a command after `--` it runs that command (the packager) with
  `MAKEPAD_ANDROID_EXTRA_LIBS=liboctos.so=<octos>`.

  python3 tools/kernel-artifact.py --sdk <cargo-makepad Android SDK dir> \\
      -- cargo makepad android run -p octosense --release

`--plan` prints the steps as JSON without running anything. Build scripts
import this file (`kernel_plan`, `extra_libs`, `receipt`) rather than copying
it.
"""
import argparse
import glob
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OCTOS_URL = "https://github.com/octos-org/octos.git"
TARGET = "aarch64-linux-android"
API = "33"
# The kernel as the shells' phones run it: the stdio server, no llama.cpp
# embedder (needs cmake and is not used on a phone).
KERNEL_BUILD = ["-p", "octos-cli", "--bin", "octos", "--no-default-features", "--features", "api,git,ast"]
LIB_NAME = "liboctos.so"


def octos_revision(lock=None):
    """The one octos revision the workspace links (its Cargo.lock)."""
    lock = Path(lock or ROOT / "Cargo.lock")
    text = lock.read_text()
    revisions = set(re.findall(
        r'name = "octos-cli"\nversion = "[^"]+"\nsource = "git\+https://github\.com/octos-org/octos\.git\?rev=([0-9a-f]{40})#', text))
    if not revisions:
        raise RuntimeError(f"{lock} names no octos-cli from octos-org/octos: cannot tell which kernel to build")
    if len(revisions) > 1:
        raise RuntimeError(f"{lock} links more than one octos-cli ({', '.join(sorted(revisions))}): one octos per graph")
    return revisions.pop()


def ndk_bin(sdk, required=True):
    """The newest NDK's LLVM bin dir inside a cargo-makepad Android SDK dir.

    `required=False` returns a placeholder path instead of failing, for plans
    printed on a machine without the SDK.
    """
    sdk = Path(sdk)
    found = sorted(glob.glob(str(sdk / "ndk/*/toolchains/llvm/prebuilt/*/bin")),
                   key=lambda p: [int(x) if x.isdigit() else x for x in re.split(r"[./]", p)])
    if found:
        return Path(found[-1])
    if required:
        raise RuntimeError(f"No NDK under {sdk}/ndk (cargo makepad android install-toolchain puts one there)")
    return sdk / "ndk/<version>/toolchains/llvm/prebuilt/<host>/bin"


def build_env(bin_dir):
    """The NDK toolchain for the kernel's cross build."""
    clang = bin_dir / f"{TARGET}{API}-clang"
    return {"CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER": str(clang),
            "CARGO_TARGET_AARCH64_LINUX_ANDROID_AR": str(bin_dir / "llvm-ar"),
            "CC_aarch64_linux_android": str(clang),
            "CXX_aarch64_linux_android": str(bin_dir / f"{TARGET}{API}-clang++"),
            "AR_aarch64_linux_android": str(bin_dir / "llvm-ar"),
            "RANLIB_aarch64_linux_android": str(bin_dir / "llvm-ranlib")}


def build_command(sdk, target_dir, offline=False, required=True):
    """`env <toolchain> CARGO_TARGET_DIR=<dir> cargo build …`: a step that
    carries its own environment, so a printed plan is the whole story."""
    env = build_env(ndk_bin(sdk, required))
    env["CARGO_TARGET_DIR"] = str(target_dir)
    command = ["env", *(f"{k}={v}" for k, v in env.items()),
               "cargo", "build", "--locked", "--release", "--target", TARGET, *KERNEL_BUILD]
    if offline:
        command.append("--offline")
    return command


def plan(revision, work, sdk, offline=False, required=True):
    """(steps, kernel): the (cwd, argv) steps that check the revision out into
    `work/src` and cross-build it into `work/target`, and the binary's path."""
    work = Path(work)
    src = work / "src"
    steps = [(work, ["git", "init", "--quiet", str(src)])]
    if not offline:
        steps.append((src, ["git", "fetch", "--quiet", "--no-tags", "--depth=1", OCTOS_URL, revision]))
    steps.append((src, ["git", "checkout", "--quiet", "--detach", revision]))
    steps.append((src, build_command(sdk, work / "target", offline, required)))
    return steps, work / "target" / TARGET / "release/octos"


def kernel_plan(*, lock=None, work=None, sdk=None, kernel=None, no_kernel=False, offline=False, required=True):
    """(steps, kernel path or None, source) for the kernel an APK bundles:
    none, a prebuilt one, or one built from the locked revision."""
    if no_kernel:
        return [], None, None
    if kernel:
        return [], Path(kernel), "prebuilt"
    if not sdk:
        raise RuntimeError("An Android SDK dir (--sdk) is required to build the kernel (or pass --kernel / --no-kernel)")
    revision = octos_revision(lock)
    steps, binary = plan(revision, work or ROOT / "target/octos-kernel", sdk, offline, required)
    return steps, binary, f"{OCTOS_URL}@{revision}"


def extra_libs(kernel):
    """cargo-makepad's MAKEPAD_ANDROID_EXTRA_LIBS for the kernel."""
    return f"{LIB_NAME}={kernel}" if kernel else None


def receipt(kernel, source):
    """What a build records about the bundled kernel."""
    if not kernel:
        return None
    return {"source": source, "sha256": hashlib.sha256(Path(kernel).read_bytes()).hexdigest()}


def run_steps(steps):
    for cwd, command in steps:
        Path(cwd).mkdir(parents=True, exist_ok=True)
        subprocess.run(command, cwd=cwd, check=True)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    command = []
    if "--" in argv:
        i = argv.index("--")
        argv, command = argv[:i], argv[i + 1:]
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--lock", type=Path, help="The Cargo.lock naming the octos revision (default: the workspace's)")
    p.add_argument("--sdk", type=Path, help="cargo-makepad's Android SDK dir (holds ndk/)")
    source = p.add_mutually_exclusive_group()
    source.add_argument("--kernel", type=Path, help="A prebuilt aarch64-linux-android octos to bundle")
    source.add_argument("--no-kernel", action="store_true", help="Bundle no kernel (the shell then runs none)")
    p.add_argument("--work", type=Path, help="Where octos is checked out and built (default: target/octos-kernel)")
    p.add_argument("--offline", action="store_true")
    p.add_argument("--receipt", type=Path, help="Write {source, sha256} of the kernel here (JSON)")
    p.add_argument("--plan", action="store_true", help="Print the plan as JSON; run nothing")
    args = p.parse_args(argv)
    if args.kernel:
        args.kernel = args.kernel.resolve()
    try:
        steps, kernel, origin = kernel_plan(lock=args.lock, work=args.work and args.work.resolve(), sdk=args.sdk and args.sdk.resolve(),
                                            kernel=args.kernel, no_kernel=args.no_kernel, offline=args.offline,
                                            required=not args.plan)
    except RuntimeError as e:
        p.error(str(e))
    if args.plan:
        print(json.dumps({"kernel": str(kernel) if kernel else None, "source": origin,
                          "android_env": {"MAKEPAD_ANDROID_EXTRA_LIBS": extra_libs(kernel)},
                          "steps": [{"cwd": str(cwd), "argv": c} for cwd, c in steps],
                          "then": command or None}, indent=2))
        return
    if args.kernel and not args.kernel.is_file():
        p.error(f"no such kernel: {args.kernel}")
    run_steps(steps)
    if kernel and not kernel.is_file():
        raise RuntimeError(f"The octos kernel was not built: {kernel}")
    print(f"octos kernel: {kernel}", flush=True)
    if args.receipt:
        args.receipt.write_text(json.dumps(receipt(kernel, origin), indent=2) + "\n")
    if command:
        env = dict(os.environ)
        env.pop("MAKEPAD_ANDROID_EXTRA_LIBS", None)
        if kernel:
            env["MAKEPAD_ANDROID_EXTRA_LIBS"] = extra_libs(kernel)
        sys.exit(subprocess.run(command, env=env).returncode)


if __name__ == "__main__":
    main()

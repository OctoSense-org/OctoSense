#!/usr/bin/env python3
"""Build the desktop shell with its pinned Octos runtime beside it.

No environment override is needed when starting the resulting shell. This
produces a source-build layout, not a relocatable installer or signed .app.
Use --kernel to reuse a native executable whose --version matches Cargo.lock.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("kernel_artifact", ROOT / "tools/kernel-artifact.py")
kernel_tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kernel_tool)
SUFFIX = ".exe" if os.name == "nt" else ""


def version_matches(version, revision):
    match = re.fullmatch(r"octos \S+ \(([0-9a-f]{7,40}) \d{4}-\d{2}-\d{2}\)", version.strip())
    return bool(match and revision.startswith(match[1]))


def stage(kernel, directory, revision):
    """Check the actual staged bytes before replacing an earlier runtime."""
    kernel, directory = Path(kernel), Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    destination = directory / ("octos-kernel" + SUFFIX)
    fd, temp = tempfile.mkstemp(prefix=".octos-stage-", suffix=SUFFIX, dir=directory)
    os.close(fd)
    temp = Path(temp)
    try:
        shutil.copy2(kernel, temp)
        temp.chmod(0o755)
        version = subprocess.check_output([str(temp), "--version"], text=True, timeout=30).strip()
        if not version_matches(version, revision):
            raise RuntimeError(f"Kernel version {version!r} differs from the locked revision {revision}")
        receipt = {"source": kernel_tool.OCTOS_URL, "revision": revision,
                   "version": version, "sha256": hashlib.sha256(temp.read_bytes()).hexdigest()}
        temp.replace(destination)
        (directory / "octos-kernel.json").write_text(json.dumps(receipt, indent=2) + "\n")
        return destination
    finally:
        temp.unlink(missing_ok=True)


def source_at(revision, work, offline):
    source = work / "src"
    source.mkdir(parents=True, exist_ok=True)
    def git(*args, check=True):
        return subprocess.run(["git", "-C", str(source), *args], capture_output=True, text=True, check=check)
    if not (source / ".git").exists():
        if any(source.iterdir()):
            raise RuntimeError(f"Preserving nonempty kernel source directory: {source}")
        git("init", "--quiet")
    if git("status", "--porcelain").stdout.strip():
        raise RuntimeError(f"Preserving modified kernel source directory: {source}")
    if git("cat-file", "-e", revision + "^{commit}", check=False).returncode:
        if offline:
            raise RuntimeError("Pinned kernel source is not cached; run once without --offline")
        git("fetch", "--quiet", "--no-tags", "--depth=1", kernel_tool.OCTOS_URL, revision)
    git("checkout", "--quiet", "--detach", revision)
    return source


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("dev", "release"), default="release")
    parser.add_argument("--features", help="Additional shell features, e.g. mobile-apps")
    parser.add_argument("--target-dir", type=Path, default=Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")))
    parser.add_argument("--work", type=Path, default=ROOT / "target/desktop-kernel")
    parser.add_argument("--kernel", type=Path, help="Reuse a native executable from the locked Octos revision")
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--plan", action="store_true", help="Print the build plan; create nothing")
    args = parser.parse_args(argv)
    revision = kernel_tool.octos_revision()
    target, work = args.target_dir.resolve(), args.work.resolve()
    output = target / ("debug" if args.profile == "dev" else "release")
    kernel = args.kernel.resolve() if args.kernel else work / "target/release" / ("octos" + SUFFIX)
    build_kernel = ["cargo", "build", "--locked", "--release", "--target-dir", str(work / "target"), *kernel_tool.KERNEL_BUILD]
    build_shell = ["cargo", "build", "--locked", "-p", "octosense", "--profile", args.profile, "--target-dir", str(target)]
    if args.features:
        build_shell += ["--features", args.features]
    if args.offline:
        build_kernel.append("--offline")
        build_shell.append("--offline")
    if args.plan:
        print(json.dumps({"revision": revision, "kernel_source": "prebuilt" if args.kernel else kernel_tool.OCTOS_URL,
                          "kernel_build": None if args.kernel else build_kernel,
                          "shell_build": build_shell, "kernel": str(kernel),
                          "shell": str(output / ("octosense" + SUFFIX)),
                          "packaged_kernel": str(output / ("octos-kernel" + SUFFIX))}, indent=2))
        return
    subprocess.run([sys.executable, str(ROOT / "tools/setup.py"), "--check", "--cargo"], cwd=ROOT, check=True)
    if not args.kernel:
        source = source_at(revision, work, args.offline)
        env = dict(os.environ)
        for name in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "MAKEPAD", "MAKEPAD_PACKAGE_DIR", "CARGO_BUILD_TARGET"):
            env.pop(name, None)
        subprocess.run(build_kernel, cwd=source, env=env, check=True)
    # Validate/stage first: a wrong prebuilt must not leave a newly built
    # shell next to the wrong runtime. The shell remains a normal Cargo build.
    runtime = stage(kernel, output, revision)
    env = dict(os.environ)
    env.pop("CARGO_BUILD_TARGET", None)
    subprocess.run(build_shell, cwd=ROOT, env=env, check=True)
    print(json.dumps({"shell": str(output / ("octosense" + SUFFIX)), "kernel": str(runtime), "revision": revision}))


if __name__ == "__main__":
    main()

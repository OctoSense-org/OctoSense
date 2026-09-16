#!/usr/bin/env python3
"""Bootstrap an application's pinned Octoscript-Makepad runtime (Python 3.9+)."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

CONSUMER = Path(__file__).resolve().parents[1]
URL = "https://github.com/OctoSense-org/Octoscript-Makepad.git"


def git(path, *args, check=True):
    return subprocess.run(["git", "-C", str(path), *args], check=check,
                          capture_output=True, text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=CONSUMER.parent)
    parser.add_argument("--update", action="store_true", help="Update clean checkouts to the release")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--cargo-manifest", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    if root == CONSUMER or root.is_relative_to(CONSUMER):
        raise RuntimeError("Keep native framework repositories outside the application")
    lock = json.loads((CONSUMER / "native-runtime.lock.json").read_text())
    if lock.get("schema_version") != 1 or lock.get("url") != URL or not re.fullmatch(r"[0-9a-f]{40}", lock.get("revision", "")):
        raise RuntimeError("Expected a pinned OctoSense-org/Octoscript-Makepad release")
    runtime = root / "octoscript-makepad"
    if not (runtime / ".git").exists():
        if args.check or runtime.exists() and any(runtime.iterdir()):
            raise RuntimeError(f"Prepare the runtime in an empty workspace: {runtime}")
        runtime.mkdir(parents=True, exist_ok=True)
        git(runtime, "init", "--quiet")
        git(runtime, "remote", "add", "origin", URL)
    current = git(runtime, "rev-parse", "--verify", "HEAD", check=False).stdout.strip()
    for name in ("octoscript-makepad", "makepad", "octoscript"):
        path = root / name
        if (path / ".git").exists() and git(path, "status", "--porcelain", "--untracked-files=normal").stdout:
            raise RuntimeError(f"Preserving local changes: {path}")
    if current != lock["revision"]:
        if args.check or current and not args.update:
            raise RuntimeError("Another runtime is checked out; use --update with clean sources")
        if git(runtime, "cat-file", "-e", lock["revision"] + "^{commit}", check=False).returncode:
            git(runtime, "fetch", "--quiet", "--no-tags", "--depth=1", "origin", lock["revision"])
        git(runtime, "checkout", "--quiet", "--detach", lock["revision"])
    command = [sys.executable, str(runtime / "tools/runtime.py"),
               "verify" if args.check else "prepare", "--root", str(root), "--consumer", str(CONSUMER)]
    if args.update:
        command.append("--update")
    if args.cargo_manifest:
        command += ["--cargo-manifest", str(args.cargo_manifest.resolve())]
    subprocess.run(command, check=True)


if __name__ == "__main__":
    main()

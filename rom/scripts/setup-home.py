#!/usr/bin/env python3
"""Prepare Home's pinned dependencies without a separate launcher checkout."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / ".sources"


def git(path, *args, check=True):
    return subprocess.run(["git", "-C", str(path), *args], check=check,
                          capture_output=True, text=True)


def prepare_app(name, spec, *, check=False, update=False, cache=None):
    if name not in {"appcards", "camera"} or spec.get("url") != "https://github.com/OctoSense-org/Octoscript-AppCard.git" or not re.fullmatch(r"[0-9a-f]{40}", spec.get("revision", "")):
        raise RuntimeError(f"Invalid pinned application source: {name}")
    path = SOURCES / name
    if not (path / ".git").exists():
        if check or path.exists() and any(path.iterdir()):
            raise RuntimeError(f"Expected an empty dependency directory: {path}")
        path.mkdir(parents=True, exist_ok=True)
        git(path, "init", "--quiet")
        git(path, "remote", "add", "origin", spec["url"])
    if git(path, "status", "--porcelain", "--untracked-files=normal").stdout:
        raise RuntimeError(f"Preserving local changes: {path}")
    current = git(path, "rev-parse", "--verify", "HEAD", check=False).stdout.strip()
    if current != spec["revision"]:
        if check or current and not update:
            raise RuntimeError(f"{path} differs from the lock; use --update for a clean checkout")
        if git(path, "cat-file", "-e", spec["revision"] + "^{commit}", check=False).returncode:
            cached = cache / name if cache else None
            source = str(cached) if cached and (cached / ".git").exists() and not git(cached, "cat-file", "-e", spec["revision"] + "^{commit}", check=False).returncode else "origin"
            git(path, "fetch", "--quiet", "--no-tags", "--depth=1", source, spec["revision"])
        git(path, "checkout", "--quiet", "--detach", spec["revision"])
    print(f"{name}: {spec['revision']}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--update", action="store_true")
    parser.add_argument("--cache", type=Path, help="Optional directory of Git object caches")
    parser.add_argument("--cargo", action="store_true", help="Also verify the locked Cargo dependency graph")
    args = parser.parse_args()
    lock = json.loads((ROOT / "home/native-apps.lock.json").read_text())
    if lock.get("schema_version") != 1 or set(lock.get("repositories", {})) != {"appcards", "camera"}:
        parser.error("Unsupported application source lock")
    for name, spec in lock["repositories"].items():
        prepare_app(name, spec, check=args.check, update=args.update,
                    cache=args.cache.resolve() if args.cache else None)
    command = [sys.executable, str(ROOT / "home/tools/setup-native.py")]
    for flag in ("check", "update"):
        if getattr(args, flag):
            command.append("--" + flag)
    if args.cache:
        command += ["--cache", str(args.cache.resolve())]
    if args.cargo:
        command += ["--cargo-manifest", str(ROOT / "home/Cargo.toml")]
    subprocess.run(command, check=True)


if __name__ == "__main__":
    main()

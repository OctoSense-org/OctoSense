#!/usr/bin/env python3
"""Build the isolated API lab with Home's exact production Java calendar adapter.

Uses already installed tools. Stages one ignored source file for cargo-makepad
and removes only the file it created. No device, account or network login use.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packager", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--package", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"dev\.makepad\.octosense\.hostapilab\.[a-z][a-z0-9_]*", args.package):
        parser.error("Use a fresh isolated hostapilab suffix, never Home")
    if not args.packager.is_file() or not args.sdk.is_dir():
        parser.error("Provide existing cargo-makepad and Android SDK paths")
    source = ROOT / "phone/resources/android/java/dev/makepad/octosense/DeviceCalendarClient.java"
    target = ROOT / "crates/host-api-smoke/resources/android/java/dev/makepad/octosense/DeviceCalendarClient.java"
    data = source.read_bytes()
    # Exclusive creation refuses another build's staging file and never replaces
    # a developer's changes. Staging also works on Windows without symlinks.
    with target.open("xb") as stream:
        stream.write(data)
    try:
        env = os.environ.copy()
        env["MAKEPAD_FORCE_DEBUGGABLE"] = "1"
        subprocess.run([str(args.packager.resolve()), "makepad", "android",
            "--sdk-path=" + str(args.sdk.resolve()), "--abi=aarch64",
            "--package-name=" + args.package, "--app-label=Host API Lab",
            "build", "-p", "octosense-host-api-smoke", "--release", "--locked", "--offline"],
            cwd=ROOT, env=env, check=True)
        print(json.dumps({"package": args.package,
            "calendar_adapter_sha256": hashlib.sha256(data).hexdigest(),
            "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()}))
    finally:
        if not target.exists():
            raise RuntimeError("Staged adapter disappeared during the build")
        if target.read_bytes() == data:
            target.unlink()
        else:
            raise RuntimeError("Staged adapter changed during the build; preserved for review")


if __name__ == "__main__":
    main()

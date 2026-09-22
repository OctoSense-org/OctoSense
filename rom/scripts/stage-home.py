#!/usr/bin/env python3
"""Verify a ROM Home build receipt, then optionally stage its APK pair."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[1]


def verify(build):
    receipt = json.loads((build / "build.json").read_text())
    if receipt.get("schema_version") != 1 or receipt.get("variant") != "rom" or receipt.get("development") is not False:
        raise ValueError("ROM staging requires a ROM build signed with the existing platform certificate")
    certificates = set()
    for name in ("OctoSenseHome.apk", "OctoSenseBridge.apk"):
        item = receipt["artifacts"][name]
        if hashlib.sha256((build / name).read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError(f"Artifact changed after signing: {name}")
        certificates.add(item["certificate_sha256"])
    if len(certificates) != 1 or not next(iter(certificates)):
        raise ValueError("Home and Bridge must have matching signing certificates")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--build", type=Path, default=ROOT / "out/home/rom")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    verify(args.build)
    if not args.verify_only:
        destination = ROOT / "vendor/octosense/prebuilt"
        destination.mkdir(parents=True, exist_ok=True)
        for name in ("OctoSenseHome.apk", "OctoSenseBridge.apk"):
            shutil.copy2(args.build / name, destination / name)
    print("ROM Home/Bridge receipt verified" + ("" if args.verify_only else "; APKs staged"))


if __name__ == "__main__":
    main()

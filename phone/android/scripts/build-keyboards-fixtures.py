#!/usr/bin/env python3
"""Build independently signed synthetic keyboard providers and an editor, without ADB."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.output.exists():
        raise SystemExit("Use a new output directory; preserving existing fixture identities")
    args.output.mkdir(parents=True)
    env = dict(os.environ, JAVA_HOME=str(args.java_home))

    def run(*command):
        return subprocess.run([str(part) for part in command], env=env, check=True,
                              capture_output=True, text=True).stdout

    spec = importlib.util.spec_from_file_location("fixture_builder", ROOT / "home/android/scripts/run-settings-accessibility-probe.py")
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    sources = ROOT / "home/android/validation-fixtures"
    provider = tuple(sorted((sources / "keyboards-provider").glob("*.java")))
    report = {"device_accessed": False, "fixtures": {}}
    certificates = set()
    for name in ("keyboards-aware", "keyboards-unaware", "keyboards"):
        apk = builder.build_fixture(sources / name, args.output / name, run,
                                    args.java_home, args.sdk, provider if name != "keyboards" else ())
        certs = run(args.sdk / "build-tools/35.0.0/apksigner", "verify", "--print-certs", apk)
        cert = re.search(r"Signer #1 certificate SHA-256 digest: ([0-9a-f]+)", certs).group(1)
        assert cert not in certificates
        certificates.add(cert)
        report["fixtures"][name] = {"path": str(apk), "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(), "certificate_sha256": cert}
    (args.output / "build.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Three independent public-SDK keyboard fixture APKs compiled; no device accessed")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Verify a ROM Home build receipt, then optionally stage its APK pair.

Home's native libraries are staged beside the APK, not inside it. A system app
that has not been updated never gets its libraries extracted by the package
manager: it loads them straight from the APK, so there is no file to exec.
Home's octos kernel (`liboctos.so`) has to be one. The staged Home APK therefore
carries no `lib/` entries, and `vendor/octosense/Android.mk` installs each
library into the app's own `lib/arm64` directory, which the package manager
then uses as Home's native library directory (the AOSP layout for system apps
with native code). `config.fs` makes the kernel executable there. The build
re-signs the APK with the platform certificate, so removing entries here does
not change what the phone verifies.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import zipfile

ROOT = Path(__file__).resolve().parents[1]
HOME_APK = "OctoSenseHome.apk"
BRIDGE_APK = "OctoSenseBridge.apk"
# The APK's ABI directory and the installed ISA directory (what the package
# manager calls the arm64-v8a native library directory of a system app).
APK_ABI_DIR = "lib/arm64-v8a/"
STAGED_LIB_DIR = Path("lib/arm64")
# Exactly the libraries Android.mk installs: Home itself and its octos kernel.
HOME_LIBRARIES = ("libmakepad.so", "liboctos.so")


def verify(build):
    receipt = json.loads((build / "build.json").read_text())
    if receipt.get("schema_version") != 1 or receipt.get("variant") != "rom" or receipt.get("development") is not False:
        raise ValueError("ROM staging requires a ROM build signed with the existing platform certificate")
    if receipt.get("dev_mode", False) is not False:
        raise ValueError("ROM staging refuses Home builds with developer mode compiled in")
    if receipt.get("home_package", "dev.makepad.octosense") != "dev.makepad.octosense":
        raise ValueError("ROM staging refuses a Home test package")
    certificates = set()
    for name in (HOME_APK, BRIDGE_APK):
        item = receipt["artifacts"][name]
        if hashlib.sha256((build / name).read_bytes()).hexdigest() != item["sha256"]:
            raise ValueError(f"Artifact changed after signing: {name}")
        certificates.add(item["certificate_sha256"])
    if len(certificates) != 1 or not next(iter(certificates)):
        raise ValueError("Home and Bridge must have matching signing certificates")
    return receipt


def split_native_libraries(apk, destination):
    """Write `apk` to `destination/OctoSenseHome.apk` without its `lib/`
    entries, and its arm64 libraries to `destination/lib/arm64/`. Returns the
    staged library names."""
    with zipfile.ZipFile(apk) as source:
        entries = source.infolist()
        native = [e for e in entries if e.filename.startswith("lib/") and not e.is_dir()]
        found = sorted(e.filename[len(APK_ABI_DIR):] for e in native if e.filename.startswith(APK_ABI_DIR))
        other_abis = sorted(e.filename for e in native if not e.filename.startswith(APK_ABI_DIR))
        if other_abis:
            raise ValueError(f"Home carries libraries for other ABIs, which the ROM does not install: {other_abis}")
        if found != sorted(HOME_LIBRARIES):
            raise ValueError(f"Home must carry exactly {list(HOME_LIBRARIES)} (the kernel included; "
                             f"build without --no-octos-kernel), found {found}")
        libraries = destination / STAGED_LIB_DIR
        shutil.rmtree(destination / "lib", ignore_errors=True)
        libraries.mkdir(parents=True)
        for entry in native:
            with source.open(entry) as reader, open(libraries / Path(entry.filename).name, "wb") as writer:
                shutil.copyfileobj(reader, writer, 1 << 20)
        staged = destination / HOME_APK
        temporary = staged.with_suffix(".staging.apk")
        with zipfile.ZipFile(temporary, "w") as target:
            for entry in entries:
                if entry.filename.startswith("lib/"):
                    continue
                with source.open(entry) as reader, target.open(entry, "w") as writer:
                    shutil.copyfileobj(reader, writer, 1 << 20)
        temporary.replace(staged)
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--build", type=Path, default=ROOT / "out/home/rom")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    verify(args.build)
    if not args.verify_only:
        destination = ROOT / "vendor/octosense/prebuilt"
        destination.mkdir(parents=True, exist_ok=True)
        shutil.copy2(args.build / BRIDGE_APK, destination / BRIDGE_APK)
        libraries = split_native_libraries(args.build / HOME_APK, destination)
        print(f"Home's native libraries staged under {destination / STAGED_LIB_DIR}: {', '.join(libraries)}")
    print("ROM Home/Bridge receipt verified" + ("" if args.verify_only else "; APKs staged"))


if __name__ == "__main__":
    main()

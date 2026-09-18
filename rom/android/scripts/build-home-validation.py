#!/usr/bin/env python3
"""Build an opt-in Home validation APK using the existing native packager."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--packager", required=True, type=Path)
    parser.add_argument("--sdk", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    manifest = (root / "resources/android/AndroidManifest.xml.template").read_text()
    if manifest.count("<application") != 1 or "<instrumentation" in manifest:
        parser.error("The normal Home manifest must contain one application and no instrumentation.")
    registration = (
        '<instrumentation android:name="dev.makepad.octosense.validation.BridgeInstrumentation"\n'
        '        android:targetPackage="{package_id}" android:functionalTest="true" />\n    '
    )
    validation_dir = root / "target/android/home-validation"
    validation_dir.mkdir(parents=True, exist_ok=True)
    template = validation_dir / "AndroidManifest.xml.template"
    manifest=manifest.replace("<application", registration + "<application", 1)
    manifest=manifest.replace("</application>",
        '<meta-data android:name="octosense.validation" android:value="true" />\n'
        '        <activity-alias android:name="dev.makepad.octosense.validation.PackageChangeAlias"\n'
        '            android:targetActivity=".{class_name}" android:enabled="false" android:exported="false" />\n'
        '    </application>',1)
    template.write_text(manifest)
    environment = dict(os.environ, MAKEPAD_ANDROID_MANIFEST_TEMPLATE=str(template))
    subprocess.run(
        [str(args.packager.resolve()), "android", "--sdk-path=" + str(args.sdk.resolve()),
         "--abi=aarch64", "build", "--release", "--locked", "--offline",
         "--no-default-features", "-p", "octosense"],
        cwd=root, env=environment, check=True,
    )
    shutil.copy2(root / "target/android/makepad-android-apk/octosense/apk/octo_sense.apk",
                 validation_dir / "home-validation.apk")
    print("Validation APK:", validation_dir / "home-validation.apk")


if __name__ == "__main__":
    main()

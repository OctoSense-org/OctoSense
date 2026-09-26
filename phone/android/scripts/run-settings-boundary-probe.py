#!/usr/bin/env python3
"""Check the Settings Binder caller gates on a dedicated AOSP test emulator."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
import time
import zipfile

PACKAGE = "dev.makepad.octosense.settingsboundary"
AOSP_TEST_CERT = "c8a2e9bccf597c2fb6dc66bee293fc13f2fc47ec77bc6b2b0d52c11f51192ab8"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--platform-test-key", type=Path, required=True)
    parser.add_argument("--platform-test-cert", type=Path, required=True)
    parser.add_argument("--roles", choices=("controller", "all"), help="Also test the native role adapter; all includes Agent60/61")
    parser.add_argument("--permissions", choices=("controller", "all"), help="Also test the native runtime-permission adapter; all includes Agent62/63")
    parser.add_argument("--dnd", choices=("broker", "all"), help="Also test DND Broker2–6; all includes Agent64–68")
    parser.add_argument("--app-policy",choices=("network","battery","storage","language","all"),help="Test observed per-app policy Agent69–76 and the finite Broker services")
    parser.add_argument("--captions",choices=("custom","language","all"),help="Test caption editor lifecycle/actions Agent77–79 and native language Agent80–81")
    parser.add_argument("--system-languages",action="store_true",help="Test Agent82/83 and the native ordered system-language broker")
    args = parser.parse_args()
    env = dict(os.environ, JAVA_HOME=str(args.java_home))

    def run(*command, check=True):
        return subprocess.run([str(part) for part in command], env=env, check=check,
                              text=True, capture_output=True)

    def adb(*command, check=True):
        return run(args.adb, "-s", args.serial, *command, check=check)

    if not args.serial.startswith("emulator-") or adb("shell", "getprop", "ro.kernel.qemu").stdout.strip() != "1":
        parser.error("Use a dedicated emulator; physical devices are excluded")
    if adb("shell", "pm", "path", PACKAGE, check=False).stdout.strip():
        parser.error("The disposable package already exists; refusing to replace it")
    for package in ("dev.makepad.octosense.agent", "dev.makepad.octosense.settingsbroker", "com.android.systemui"):
        if not adb("shell", "pm", "path", package, check=False).stdout.startswith("package:"):
            parser.error(f"Stage the platform services first: missing {package}")

    bt = args.sdk / "build-tools/35.0.0"
    android = args.sdk / "platforms/android-35/android.jar"
    source = Path(__file__).resolve().parents[1] / "validation-fixtures/settings-boundary/BoundaryActivity.java"
    with tempfile.TemporaryDirectory(prefix="octosense-settings-boundary-") as temporary:
        work = Path(temporary)
        (work / "classes").mkdir()
        (work / "dex").mkdir()
        run(args.java_home / "bin/javac", "-cp", android, "-d", work / "classes", source)
        run(args.java_home / "bin/jar", "cf", work / "classes.jar", "-C", work / "classes", ".")
        run(bt / "d8", "--lib", android, "--min-api", "26", "--output", work / "dex", work / "classes.jar")
        manifest = work / "AndroidManifest.xml"
        manifest.write_text('''<manifest xmlns:android="http://schemas.android.com/apk/res/android"
            package="dev.makepad.octosense.settingsboundary" android:versionCode="1" android:versionName="1">
          <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="35"/>
          <uses-permission android:name="dev.makepad.octosense.permission.BIND_AGENT_PLATFORM"/>
          <queries><package android:name="dev.makepad.octosense.agent"/>
            <package android:name="dev.makepad.octosense.settingsbroker"/>
            <package android:name="com.android.systemui"/>
            <package android:name="com.android.permissioncontroller"/></queries>
          <application android:label="Disposable Settings boundary test"
              android:theme="@android:style/Theme.Material.Light.NoActionBar">
            <activity android:name=".BoundaryActivity" android:exported="true"/>
          </application></manifest>''')
        run(bt / "aapt2", "link", "--manifest", manifest, "-I", android, "-o", work / "unsigned.apk")
        with zipfile.ZipFile(work / "unsigned.apk", "a") as archive:
            archive.write(work / "dex/classes.dex", "classes.dex")
        run(bt / "zipalign", "-f", "4", work / "unsigned.apk", work / "aligned.apk")
        # The ordinary caller has a freshly generated disposable identity.
        run(args.java_home / "bin/keytool", "-genkeypair", "-keystore", work / "ordinary.p12",
            "-storepass", "fixture-only", "-alias", "fixture", "-keyalg", "RSA", "-validity", "1",
            "-dname", "CN=Disposable Settings Boundary Fixture", "-noprompt")
        for variant in ("ordinary", "platform"):
            apk = work / f"{variant}.apk"
            signing = ["--ks", work / "ordinary.p12", "--ks-pass", "pass:fixture-only"] if variant == "ordinary" else [
                "--key", args.platform_test_key, "--cert", args.platform_test_cert]
            run(bt / "apksigner", "sign", *signing, "--out", apk, work / "aligned.apk")
            if variant == "platform" and AOSP_TEST_CERT not in run(bt / "apksigner", "verify", "--print-certs", apk).stdout:
                raise RuntimeError("Only the public AOSP platform test certificate is accepted")
            installed = False
            try:
                adb("install", "--no-incremental", apk)
                installed = True
                # A platform run must have the signature permission, otherwise
                # denied binding would hide a missing in-service package check.
                permission = adb("shell", "dumpsys", "package", PACKAGE).stdout
                granted = "dev.makepad.octosense.permission.BIND_AGENT_PLATFORM: granted=true" in permission
                if granted != (variant == "platform"):
                    raise RuntimeError(f"{variant}: signature permission does not match test identity")
                extras = ("--es", "roles", args.roles) if args.roles else ()
                if args.permissions:
                    extras += ("--es", "permissions", args.permissions)
                if args.dnd:
                    extras += ("--es", "dnd", args.dnd)
                if args.app_policy:
                    extras += ("--es","app_policy",args.app_policy)
                if args.captions:
                    extras += ("--es","captions",args.captions)
                if args.system_languages:
                    extras += ("--ez","system_languages","true")
                adb("shell", "am", "start", "-W", "-n", PACKAGE + "/.BoundaryActivity", *extras)
                pid = adb("shell", "pidof", PACKAGE).stdout.strip()
                if not pid.isdigit():
                    raise RuntimeError(f"{variant}: fixture process missing")
                deadline = time.monotonic() + 12
                while True:
                    log = adb("logcat", "-d", "--pid=" + pid, "-s", "OctoSenseBoundary:I", "*:S").stdout
                    if "BOUNDARY_FAIL" in log:
                        raise RuntimeError(f"{variant}: {log}")
                    if "BOUNDARY_PASS" in log:
                        labels = ["unexported_confirmation", "helper", "accounts", "sensors", "dnd", "time", "history", "sounds", "sound_stop", "display", "display_write", "controls", "controls_write", "text_interaction", "text_interaction_write", "app_notifications", "app_notifications_write", "broker_notifications", "broker_notifications_write"]
                        if args.roles:
                            labels += ["unexported_role_confirmation", "controller_roles", "controller_roles_confirm"]
                        if args.roles == "all":
                            labels += ["roles", "roles_confirm"]
                        if args.permissions:
                            labels += ["unexported_permission_operation", "controller_permissions", "controller_permission_choice"]
                        if args.permissions == "all":
                            labels += ["permissions", "permission_choice"]
                        if args.dnd:
                            labels += ["broker_dnd_" + method for method in ("snapshot", "policy", "schedule", "enabled", "delete")]
                        if args.dnd == "all":
                            labels += ["agent_dnd_" + method for method in ("snapshot", "policy", "schedule", "enabled", "delete")]
                        for area in ("network","battery","storage","language"):
                            if args.app_policy in (area,"all"):
                                labels += [side+"_app_"+area+"_"+method for side in ("agent","broker") for method in ("snapshot","set")]
                        if args.captions in ("custom","all"):
                            labels += ["caption_custom_"+method for method in ("snapshot","set","close")]
                        if args.captions in ("language","all"):
                            labels += ["caption_language_"+method for method in ("snapshot","set")]
                        if args.system_languages:
                            labels += [side+"_system_languages_"+method for side in ("agent","broker") for method in ("snapshot","apply")]
                        for label in labels:
                            if f"{label} denied=true" not in log:
                                raise RuntimeError(f"{variant}: incomplete denial results")
                        print(f"{variant}: all {len(labels)} caller checks passed")
                        break
                    if time.monotonic() >= deadline:
                        raise RuntimeError(f"{variant}: timed out without a complete result")
                    time.sleep(0.25)
            finally:
                if installed:
                    adb("uninstall", PACKAGE)


if __name__ == "__main__":
    main()

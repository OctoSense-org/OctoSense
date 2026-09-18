#!/usr/bin/env python3
"""Exercise the isolated bridge queue, restoring the exact installed bridge APK."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--validation-apk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--original-apk", type=Path, help="Retained APK; must match the installed checksum before use")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    adb = [args.adb, "-s", args.serial]
    package = "dev.makepad.octosense.bridge"
    backup = args.output / "original-bridge.apk"
    report = {"result": "fail", "serial": args.serial, "restored": False}
    attempted_install = False

    def command(*parts, timeout=60):
        return subprocess.check_output(adb + list(parts), text=True,
                                       stderr=subprocess.STDOUT, timeout=timeout).strip()

    def digest(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def installed(name):
        paths = command("shell", "pm", "path", name).splitlines()
        if len(paths) != 1 or not paths[0].startswith("package:"):
            raise AssertionError("Expected one retained base APK: " + name)
        path = paths[0].removeprefix("package:")
        return path, command("shell", "sha256sum", path).split()[0]

    def device_state():
        return {
            "apks": {name: installed(name)[1] for name in [
                "dev.makepad.octosense", package, "dev.makepad.octosense.quickstep"]},
            "home_role": command("shell", "cmd", "role", "get-role-holders", "--user", "0", "android.app.role.HOME"),
            "recents": command("shell", "cmd", "overlay", "lookup", "android", "android:string/config_recentsComponentName"),
            "settings": {namespace: {key: command("shell", "settings", "get", namespace, key) for key in keys}
                         for namespace, keys in {
                             "system": ["screen_brightness", "screen_brightness_mode", "accelerometer_rotation", "volume_music"],
                             "secure": ["navigation_mode", "enabled_notification_listeners", "enabled_notification_policy_access_packages"],
                             "global": ["wifi_on", "bluetooth_on", "low_power", "zen_mode"],
                         }.items()},
            "instrumentation": command("shell", "pm", "list", "instrumentation"),
        }

    try:
        report["before"] = device_state()
        if package in report["before"]["instrumentation"]:
            raise AssertionError("Bridge validation is already installed; preserve the existing test session")
        path, expected = installed(package)
        if args.original_apk:
            shutil.copy2(args.original_apk, backup)
        else:
            command("pull", path, str(backup))
        if digest(backup) != expected:
            raise AssertionError("Rollback APK checksum mismatch")
        report["validation_sha256"] = digest(args.validation_apk)
        attempted_install = True
        command("install", "-r", str(args.validation_apk))
        if installed(package)[1] != report["validation_sha256"]:
            raise AssertionError("Installed validation APK mismatch")
        output = command("shell", "am", "instrument", "-w",
                         package + "/" + package + ".BridgeQueueInstrumentation")
        (args.output / "instrumentation.log").write_text(output + "\n")
        results = dict(re.findall(r"^INSTRUMENTATION_RESULT: ([^=]+)=(.*)$", output, re.MULTILINE))
        report["instrumentation"] = results
        if results.get("result") != "pass" or "INSTRUMENTATION_CODE: -1" not in output:
            raise AssertionError("Bridge queue instrumentation failed: " + results.get("failure", output))
        report["result"] = "pass"
    except Exception as error:
        report["failure"] = str(error)
    finally:
        if attempted_install:
            try:
                command("install", "-r", str(backup))
                report["after"] = device_state()
                report["restored"] = report["after"] == report["before"]
                if not report["restored"]:
                    raise AssertionError("Final APKs, registrations or settings differ from the retained baseline")
                pid = report.get("instrumentation", {}).get("pid")
                if pid:
                    callback_pid = report["instrumentation"].get("callback_pid")
                    owned = {pid} | ({callback_pid} if callback_pid else set())
                    deadline = time.monotonic() + 10
                    while time.monotonic() < deadline:
                        probe = subprocess.run(adb + ["shell", "pidof", package, package + ":queue_callback"], text=True,
                                               capture_output=True, timeout=10)
                        if probe.returncode not in (0, 1):
                            raise RuntimeError("Cannot verify validation process cleanup: " + probe.stderr)
                        pids = probe.stdout.split()
                        if not owned.intersection(pids):
                            break
                        time.sleep(0.1)
                    report["owned_pid_exited"] = not owned.intersection(pids)
                    if callback_pid:
                        report["callback_pid_exited"] = callback_pid not in pids
                    if not report["owned_pid_exited"]:
                        raise AssertionError("Validation process survived restoration")
            except Exception as error:
                report["result"] = "fail"
                report["restore_failure"] = str(error)
        (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({key: value for key, value in report.items() if key not in ["before", "after"]}, indent=2))
    return 0 if report["result"] == "pass" and report["restored"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

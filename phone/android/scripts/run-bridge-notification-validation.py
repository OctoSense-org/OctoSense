#!/usr/bin/env python3
"""Run an explicitly authorized fixture-only listener test and restore access/APKs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--allow-temporary-notification-access", action="store_true", required=True)
    parser.add_argument("--ui-flow", action="store_true", help="Exercise the visible shade/editor/Rust/JNI flow")
    parser.add_argument("--visible-seconds", type=int, default=12)
    parser.add_argument("--skip-captures", action="store_true", help="Diagnose UI behavior without PixelCopy; does not prove visual acceptance")
    for name in ["home-normal", "home-validation", "bridge-normal", "bridge-test"]:
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    if not 0 <= args.visible_seconds <= 30:
        parser.error("--visible-seconds must be between 0 and 30")
    args.output.mkdir(parents=True, exist_ok=False)
    adb = [args.adb, "-s", args.serial]
    package = "dev.makepad.octosense"
    component = package + ".bridge/.NotificationAccessService"
    permission = "android.permission.POST_NOTIFICATIONS"
    report = {"result": "fail", "explicit_temporary_access_approved": True,
              "retained_notification_content": "fixture_only", "phases": [], "restored": {}}
    originals = {package: args.home_normal, package + ".bridge": args.bridge_normal}
    process = None
    changed = False
    baseline = None
    ui = None

    def command(*parts):
        # All commands here are reads or idempotent install/permission/forward
        # operations. A transient USB loss must not skip restoration. Interactive
        # input and instrumentation launches are deliberately not retried here.
        deadline = time.monotonic() + 30
        while True:
            try:
                return subprocess.check_output(adb + list(parts), text=True, stderr=subprocess.STDOUT, timeout=90).strip()
            except subprocess.CalledProcessError as error:
                message = error.output.lower()
                disconnected = "no devices/emulators" in message or ("device" in message and any(
                    marker in message for marker in ["not found", "offline", "disconnected"]))
                if not disconnected or time.monotonic() >= deadline:
                    raise
                report["adb_reconnect_retries"] = report.get("adb_reconnect_retries", 0) + 1
                time.sleep(1)

    def installed(pkg):
        path = command("shell", "pm", "path", pkg).splitlines()[0].removeprefix("package:")
        return command("shell", "sha256sum", path).split()[0]

    def state():
        dump = command("shell", "dumpsys", "package", package)
        line = next(line.strip() for line in dump.splitlines() if permission + ": granted=" in line)
        op = command("shell", "cmd", "appops", "get", package, "POST_NOTIFICATION")
        match = re.search(r"POST_NOTIFICATION: (\w+)", op)
        return {
            "listeners": command("shell", "settings", "get", "secure", "enabled_notification_listeners"),
            "post_permission": line,
            "post_appop": match.group(1) if match else "default",
            "home_role": command("shell", "cmd", "role", "get-role-holders", "--user", "0", "android.app.role.HOME"),
            "recents": command("shell", "cmd", "overlay", "lookup", "android", "android:string/config_recentsComponentName"),
            "navigation_mode": command("shell", "settings", "get", "secure", "navigation_mode"),
            "instrumentation": command("shell", "pm", "list", "instrumentation"),
            "forwards": command("forward", "--list"),
        }

    def listener(enabled):
        command("shell", "cmd", "notification", "allow_listener" if enabled else "disallow_listener", component, "0")
        preserve_listener_entries(enabled)

    def preserve_listener_entries(enabled):
        # Android rewrites this setting using resolved ComponentNames and can
        # drop a preexisting package-only entry. Preserve all original entries
        # and concurrent additions, without retaining our temporary component.
        actual = command("shell", "settings", "get", "secure", "enabled_notification_listeners")
        originals = [] if baseline["listeners"] in ("", "null") else baseline["listeners"].split(":")
        current = [] if actual in ("", "null") else actual.split(":")
        preserved = list(dict.fromkeys(originals + current))
        if not enabled:
            preserved = [item for item in preserved if not item.startswith(package + ".bridge/")]
        value = ":".join(preserved)
        if value != actual and not (actual == "null" and not value):
            command("shell", "settings", "put", "secure", "enabled_notification_listeners", value)
            report.setdefault("listener_setting_normalization_preserved", []).append(enabled)
        if not preserved and baseline["listeners"] == "null":
            command("shell", "settings", "delete", "secure", "enabled_notification_listeners")

    try:
        for pkg, apk in originals.items():
            expected = hashlib.sha256(apk.read_bytes()).hexdigest()
            assert installed(pkg) == expected, "Unexpected original APK: " + pkg
        baseline = state()
        report["before"] = baseline
        assert package + ".bridge" not in baseline["listeners"], "Test requires OctoSense access initially revoked"
        changed = True
        command("install", "-r", str(args.bridge_test.resolve()))
        command("install", "-r", str(args.home_validation.resolve()))
        if "granted=false" in baseline["post_permission"]:
            command("shell", "pm", "grant", "--user", "0", package, permission)
        report["test_apks"] = {package: installed(package), package + ".bridge": installed(package + ".bridge")}
        if args.ui_flow:
            from notification_flow_driver import NotificationFlowDriver
            ui = NotificationFlowDriver(command, args.output, report, args.visible_seconds, args.skip_captures)
        log_path = args.output / "instrumentation.log"
        with log_path.open("w") as log:
            process = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "notification_flow" if ui else "notification_roundtrip",
                package + "/dev.makepad.octosense.validation.BridgeInstrumentation"], stdout=log, stderr=subprocess.STDOUT)
            if ui:
                ui.run(log_path, process, listener)
            deadline = time.monotonic() + 160
            while process.poll() is None and time.monotonic() < deadline:
                output = log_path.read_text()
                for phase, enabled in [("ready_for_grant", True), ("ready_for_revoke", False)]:
                    if "fixture_phase=" + phase in output and phase not in report["phases"]:
                        listener(enabled)
                        report["phases"].append(phase)
                        print("Fixture phase: " + phase, flush=True)
                time.sleep(0.2)
            if process.poll() is None:
                raise AssertionError("Fixture runner timeout")
        output = log_path.read_text()
        assert "result=pass" in output and "INSTRUMENTATION_CODE: -1" in output, "Fixture instrumentation failed; see fixture-only log"
        assert report["phases"] == ["ready_for_grant", "ready_for_revoke"]
        report["result"] = "pass"
    except Exception as error:
        report["error"] = str(error)
    finally:
        errors = []
        if changed:
            try:
                listener(False)
            except Exception as error:
                errors.append("listener revocation: " + str(error))
            if ui:
                try:
                    ui.close()
                except Exception as error:
                    errors.append("UI fixture cleanup: " + str(error))
            if process is not None and process.poll() is None:
                try:
                    process.wait(timeout=35)
                except subprocess.TimeoutExpired:
                    errors.append("fixture did not exit after revocation")
            try:
                if "granted=false" in baseline["post_permission"]:
                    command("shell", "pm", "revoke", "--user", "0", package, permission)
            except Exception as error:
                errors.append("post permission: " + str(error))
            for pkg, apk in originals.items():
                try:
                    command("install", "-r", str(apk.resolve()))
                    report["restored"][pkg] = installed(pkg)
                    assert report["restored"][pkg] == hashlib.sha256(apk.read_bytes()).hexdigest()
                except Exception as error:
                    errors.append(pkg + ": " + str(error))
            try:
                # Package replacement can trigger another normalization pass.
                preserve_listener_entries(False)
                after = state()
                if after["post_appop"] != baseline["post_appop"]:
                    command("shell", "cmd", "appops", "set", package, "POST_NOTIFICATION", baseline["post_appop"])
                    after = state()
                report["after"] = after
                if "owned_home_pid" in report:
                    pid = str(report["owned_home_pid"])
                    found = subprocess.run(adb + ["shell", "ps", "-p", pid, "-o", "PID="], capture_output=True, text=True, timeout=10)
                    report["owned_home_pid_exited"] = pid not in found.stdout.split()
                    assert report["owned_home_pid_exited"], "Owned Home test process still alive"
                report["state_restored"] = after == baseline
                assert report["state_restored"], "Permission, role or settings mismatch after restoration"
            except Exception as error:
                errors.append("restoration verification: " + str(error))
        if errors:
            report["cleanup_errors"] = errors
            report["result"] = "fail"
        (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))
    if report["result"] != "pass":
        raise SystemExit(1)


if __name__ == "__main__":
    main()

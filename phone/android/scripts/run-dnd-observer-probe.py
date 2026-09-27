#!/usr/bin/env python3
"""Validate native DND observation and synthetic ranking on the disposable emulator."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import time

PACKAGE = "dev.makepad.octosense.dndfixture"
LISTENER = PACKAGE + "/.DndRankingListener"
ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", type=Path, required=True)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--java-home", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--ui-observer", action="store_true", help="Also check nested ranking while Settings owns UiAutomation")
    parser.add_argument("--ui-scenario", choices=("dnd_observer", "dnd_policy", "dnd_schedules"))
    args = parser.parse_args()
    env = dict(os.environ, JAVA_HOME=str(args.java_home))

    def run(*command):
        return subprocess.run([str(part) for part in command], check=True,
                              capture_output=True, text=True, env=env).stdout

    def adb(*command):
        return run(args.adb, "-s", args.serial, *command)

    def shell(*command):
        return adb("shell", shlex.join(str(part) for part in command))

    if args.serial != "emulator-5560" or shell("getprop", "ro.kernel.qemu").strip() != "1" \
            or "OctoSense_ROM_Roles_API35" not in adb("emu", "avd", "name"):
        raise SystemExit("DND observations require the disposable emulator-5560 clone.")
    if shell("pm", "list", "packages", PACKAGE).strip():
        raise SystemExit("Synthetic DND fixture already exists; refusing to replace it.")
    ui_package = "dev.makepad.octosense.settingsa11yfixture"
    ui_scenario = args.ui_scenario or ("dnd_observer" if args.ui_observer else None)
    if ui_scenario and shell("pm", "list", "packages", ui_package).strip():
        raise SystemExit("Settings UI fixture already exists; refusing to replace it.")
    listeners = shell("settings", "get", "secure", "enabled_notification_listeners").strip()
    if PACKAGE in listeners:
        raise SystemExit("Synthetic listener already granted; refusing to alter that baseline.")

    spec = importlib.util.spec_from_file_location("settings_fixture_build", Path(__file__).with_name("run-settings-accessibility-probe.py"))
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    installed = False
    ui_installed = False
    before = None
    report = {"emulator": args.serial, "scenario": ui_scenario or "observer",
              "listeners_before": listeners}

    def observe(operation="state"):
        output = shell("am", "instrument", "-w", "-e", "operation", operation,
                       PACKAGE + "/.DndFixture")
        values = {}
        for line in output.splitlines():
            if line.startswith("INSTRUMENTATION_RESULT: "):
                key, _, value = line.removeprefix("INSTRUMENTATION_RESULT: ").partition("=")
                values[key] = value
        if values.get("passed") != "true":
            raise RuntimeError("DND observer failed: " + values.get("failure", output[-2000:]))
        return {key: json.loads(values[key]) for key in ("state", "ranking") if key in values}

    try:
        with tempfile.TemporaryDirectory(prefix="octosense-dnd-observer-") as tmp:
            apk = builder.build_fixture(ROOT / "home/android/validation-fixtures/dnd-observer",
                                        Path(tmp), run, args.java_home, args.sdk)
            adb("install", str(apk)); installed = True
            before = observe()["state"]
            report["before"] = before
            shell("pm", "grant", PACKAGE, "android.permission.POST_NOTIFICATIONS")
            shell("cmd", "notification", "allow_listener", LISTENER, "0")
            after = observe("post")
            report["ranking"] = after["ranking"]
            if {row["category"] for row in after["ranking"]} != {"alarm", "event", "reminder", "msg", "call", "status"} \
                    or any(row["bypass_dnd"] or row["suspended"] or row["importance"] != 3 for row in after["ranking"]):
                raise RuntimeError("Synthetic ranking has missing categories or unsuitable channel state")
            if before["filter"] == 1 and not all(row["matches_filter"] for row in after["ranking"]):
                raise RuntimeError("DND Off unexpectedly intercepted a synthetic notification")
            if before != after["state"]:
                raise RuntimeError("DND policy/rules changed during read-only synthetic ranking probe")
            if ui_scenario:
                fixture = ROOT / "home/android/validation-fixtures/settings-accessibility"
                contract = ROOT / "home/android/contracts/src/main/java/dev/makepad/octosense/contracts"
                ui_apk = builder.build_fixture(fixture, Path(tmp) / "ui", run, args.java_home, args.sdk,
                                              (contract / "SystemSettings.java", contract / "Protocol.java",
                                               fixture.parent / "dnd-observer/DndNativeSnapshot.java"))
                adb("install", str(ui_apk)); ui_installed = True
                ui_result = shell("am", "instrument", "-w", "-e", "scenario", ui_scenario,
                                  ui_package + "/.SettingsAccessibilityProbe")
                report["ui_result"] = ui_result
                if "INSTRUMENTATION_RESULT: passed=true" not in ui_result:
                    raise RuntimeError("Nested Settings DND observer failed: " + ui_result)
            observe("cancel")
            shell("cmd", "notification", "disallow_listener", LISTENER, "0")
            final = observe()["state"]
            if final != before:
                raise RuntimeError("DND policy/rules changed after listener cleanup")
            report["native_state_preserved"] = True
    except BaseException as failure:
        report["failure"] = str(failure)
        raise
    finally:
        failures = []
        if installed and before is not None:
            try:
                report["after"] = observe()["state"]
                report["native_state_preserved"] = report["after"] == before
                if not report["native_state_preserved"]:
                    failures.append("Native policy/rules differ from the captured baseline")
            except Exception as failure:
                failures.append("Native restoration observation: " + str(failure))
        for command, needed in ((lambda: adb("uninstall", ui_package), ui_installed),
                                (lambda: shell("cmd", "notification", "disallow_listener", LISTENER, "0"), installed),
                                (lambda: adb("uninstall", PACKAGE), installed)):
            if needed:
                try: command()
                except Exception as failure: failures.append(str(failure))
        deadline = time.monotonic() + 5
        while True:
            restored = shell("settings", "get", "secure", "enabled_notification_listeners").strip()
            if restored == listeners or time.monotonic() >= deadline:
                break
            time.sleep(.1)
        def components(value):
            return set() if value in ("", "null") else set(value.split(":"))
        if restored != listeners and components(restored) == components(listeners):
            # Native grant removal can normalize absent/empty or order. Restore
            # raw representation only after the original grants are identical.
            if listeners == "null": shell("settings", "delete", "secure", "enabled_notification_listeners")
            else: shell("settings", "put", "secure", "enabled_notification_listeners", listeners)
            report["listener_representation_restored"] = True
            restored = shell("settings", "get", "secure", "enabled_notification_listeners").strip()
        report["listeners_after"] = restored
        report["listeners_preserved"] = restored == listeners
        report["fixture_removed"] = not shell("pm", "list", "packages", PACKAGE).strip() \
            and (not ui_installed or not shell("pm", "list", "packages", ui_package).strip())
        args.report.parent.mkdir(parents=True, exist_ok=True)
        report["cleanup_failures"] = failures
        args.report.write_text(json.dumps(report, indent=2) + "\n")
        if failures or restored != listeners or not report["fixture_removed"]:
            raise RuntimeError("DND fixture/listener cleanup did not preserve baseline: " + "; ".join(failures))
    if ui_scenario in ("dnd_policy", "dnd_schedules"):
        print(f"{ui_scenario} acceptance passed; native policy/rules restored and fixtures removed.")
    else:
        print("DND read-only policy/rule and six-category ranking preflight passed; fixtures removed.")


if __name__ == "__main__":
    main()

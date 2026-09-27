#!/usr/bin/env python3
"""Verify public Home using a disposable activity in the bridge validation APK.

Requires approved Home/bridge installations. Temporarily disables only the
bridge service, restores its verified default state, and replaces the bridge
with the supplied normal/validation builds to exercise package callbacks.
An isolated Home cache journal protects the user's placement data.
"""
import argparse
import http.client
import json
from pathlib import Path
import re
import shlex
import subprocess
import time
import urllib.parse
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--bridge-normal", required=True, type=Path)
    parser.add_argument("--bridge-validation", required=True, type=Path)
    parser.add_argument("--process-death", action="store_true",
                        help="Kill only the owned Home PID after placement, then recover its isolated journal in a fresh process")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]
    component = "dev.makepad.octosense.bridge/.SystemBridgeService"
    fixture = "dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.validation.LauncherFixtureActivity"
    identity = "android:0:" + fixture
    report = {"serial": args.serial, "result": "fail", "states": {}, "native_launches": [],
              "captures": [], "production_placements_modified": False,
              "process_recreation_tested": False, "reboot_tested": False}
    forwards = []
    home = None
    native = None
    closed = False
    component_changed = False

    def command(*parts):
        return subprocess.check_output(adb + list(parts), text=True, stderr=subprocess.STDOUT, timeout=90).strip()

    def service_state(enabled):
        # Android 15 rejects shell-UID changes to individual components. Use
        # the bench's existing ADB root grant, never the bridge's Magisk grant.
        operation = "default-state" if enabled else "disable"
        result = command("shell", "su -c " + shlex.quote("pm " + operation + " --user 0 " + component))
        expected = "default" if enabled else "disabled"
        assert "new state: " + expected in result, result
        return result

    def logs(tag):
        return command("logcat", "-d", "-s", tag + ":I", "*:S")

    def remotes(tag):
        if tag == "OctoSenseLauncherFixture":
            return [(pid, "localabstract:" + address, token) for pid, address, token in
                    re.findall(r"--remote pid=(\d+) socket=([a-z0-9-]+) token=([a-f0-9-]+)", logs(tag))]
        return [(pid, "tcp:" + port, token) for pid, port, token in
                re.findall(r"--remote pid=(\d+) port=(\d+) token=([a-f0-9-]+)", logs(tag))]

    def attach(tag, previous):
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            found = [entry for entry in remotes(tag) if entry not in previous]
            if found:
                pid, endpoint, token = found[-1]
                forward = command("forward", "tcp:0", endpoint)
                forwards.append(forward)
                return {"pid": int(pid), "base": "http://127.0.0.1:" + forward + "/" + token}
            time.sleep(0.25)
        raise RuntimeError("Owned remote did not start: " + tag)

    def request(remote, route):
        with urllib.request.urlopen(remote["base"] + route, timeout=5) as response:
            return response.read()

    def state(name, predicate, timeout=15):
        deadline = time.monotonic() + timeout
        value = None
        while time.monotonic() < deadline:
            try:
                value = json.loads(request(home, "/status"))
                assert value["pid"] == home["pid"]
                if predicate(value):
                    report["states"][name] = value
                    print("Verified " + name, flush=True)
                    return value
            except (OSError, ValueError):
                pass
            time.sleep(0.2)
        report["states"][name + "-timeout"] = value
        raise AssertionError("State timeout: " + name)

    def launcher(value):
        return value["widgets"]["launcher"]

    def geometry(value):
        return value["widgets"]["home_integration"]

    def fixture_icon(value):
        return next((icon for icon in geometry(value).get("icons", []) if icon["component"] == fixture), None)

    def icon(name):
        for page in range(5):
            request(home, "/page?index=" + str(page))
            time.sleep(0.4)
            value = state(name + "-page-" + str(page), lambda v: geometry(v).get("local_ready", False))
            found = fixture_icon(value)
            if found:
                left, top, right, bottom = found["bounds"]
                return {"x": (left + right) / 2, "y": (top + bottom) / 2}
        raise AssertionError("Fixture icon was not actually drawn")

    def tap(point, hold=False):
        request(home, ("/hold?" if hold else "/tap?") + urllib.parse.urlencode(point))
        if hold:
            time.sleep(1.1)  # Wait for the original finger's release.

    def menu(point, label, name):
        tap(point, hold=True)
        value = state(name, lambda v: any(item["label"] == label for item in launcher(v)["menu"]))
        selected = next(item for item in launcher(value)["menu"] if item["label"] == label)
        return {"x": selected["x"], "y": selected["y"]}

    def capture(route, name):
        # ADB can truncate a large response while small status requests still
        # succeed. Retry only read-only captures, never input or lifecycle calls,
        # and retain every failed attempt in the evidence.
        assert route in ("/g", "/surface")
        for attempt in range(1, 4):
            try:
                data = request(home, route)
                break
            except http.client.IncompleteRead as exc:
                report.setdefault("capture_transfer_failures", []).append({
                    "name": name, "attempt": attempt, "pid": home["pid"],
                    "received_bytes": len(exc.partial), "missing_bytes": exc.expected})
                if attempt == 3:
                    raise
                time.sleep(0.2)
        assert data.startswith(b"\x89PNG\r\n\x1a\n")
        (args.output / name).write_bytes(data)
        report["captures"].append(name)

    def launch_and_return(name, point):
        nonlocal native
        previous = set(remotes("OctoSenseLauncherFixture"))
        tap(point)
        native = attach("OctoSenseLauncherFixture", previous)
        deadline = time.monotonic() + 10
        while True:
            value = json.loads(request(native, "/status"))
            if value["resumed"]:
                break
            assert time.monotonic() < deadline, "Native activity did not resume"
            time.sleep(0.1)
        assert value["pid"] == native["pid"] and value["task"] > 0
        report["native_launches"].append({"scenario": name, **value})
        request(native, "/close")
        deadline = time.monotonic() + 10
        while "closed task=" + str(value["task"]) not in logs("OctoSenseLauncherFixture"):
            assert time.monotonic() < deadline, "Owned native task did not close"
            time.sleep(0.1)
        native = None
        state(name + "-returned", lambda v: geometry(v).get("local_ready", False))

    # Exact restoration is possible only when this service starts with no
    # component override. Refuse to replace an existing customized state.
    original = command("shell", "dumpsys", "package", "dev.makepad.octosense.bridge")
    (args.output / "bridge-before.txt").write_text(original + "\n")
    assert "enabledComponents:" not in original and "disabledComponents:" not in original, "Bridge component overrides require a scoped restore plan"
    previous = set(remotes("OctoSenseValidation"))
    instrumentation_path = args.output / "instrumentation.log"
    with instrumentation_path.open("w") as log:
        runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "launcher_ui",
            "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"], stdout=log, stderr=subprocess.STDOUT)
        try:
            home = attach("OctoSenseValidation", previous)
            report["owned_pid"] = home["pid"]
            value = state("ready", lambda v: launcher(v)["fixture_available"] and identity in launcher(v)["placements"].get("favorites", []))
            point = icon("initial-icon")
            selected = menu(point, "Place in dock position 1", "dock-menu")
            capture("/g", "placement-menu.png")
            tap(selected)
            state("docked", lambda v: not launcher(v)["menu"] and launcher(v)["placements"]["dock"][0] == identity)
            point = icon("dock-icon")
            tap(menu(point, "Remove from Home", "remove-favorite-menu"))
            state("favorite-removed", lambda v: not launcher(v)["menu"] and identity not in launcher(v)["placements"]["favorites"])
            tap(menu(point, "Add to Home", "add-favorite-menu"))
            before = state("favorite-restored", lambda v: not launcher(v)["menu"] and identity in launcher(v)["placements"]["favorites"])

            menu(point, "Remove from dock", "menu-before-recreation")
            previous = set(remotes("OctoSenseValidation"))
            request(home, "/recreate")
            home = attach("OctoSenseValidation", previous)
            assert home["pid"] == report["owned_pid"], "Activity test unexpectedly replaced the process"
            after = state("recreated", lambda v: launcher(v)["epoch"] != launcher(before)["epoch"] and geometry(v).get("local_ready", False))
            assert not launcher(after)["menu"]
            for key in ["favorites", "dock"]:
                assert launcher(after)["placements"][key] == launcher(before)["placements"][key]
            report["activity_recreation_tested"] = True
            point = icon("recreated-icon")
            capture("/surface", "home-after-recreation.png")
            if args.process_death:
                saved = state("before-process-death", lambda v: launcher(v)["bridge_state"] == "connected"
                              and geometry(v).get("connection") == "connected" and geometry(v).get("home_ready", False))
                old_pid = home["pid"]
                assert command("shell", "ps", "-p", str(old_pid), "-o", "NAME=") == "dev.makepad.octosense"
                report["initial_owned_pid"] = old_pid
                report["intentional_process_death"] = {"pid": old_pid, "signal": "SIGKILL"}
                command("shell", "su -c " + shlex.quote("kill -9 " + str(old_pid)))
                home = None
                runner.wait(timeout=15)
                stopped = subprocess.run(adb + ["shell", "ps", "-p", str(old_pid), "-o", "PID="],
                                         text=True, capture_output=True, timeout=10)
                assert str(old_pid) not in stopped.stdout.split(), "Owned process survived SIGKILL"
                report["initial_owned_pid_exited"] = True
                # Instrumentation death is expected in the first phase. The
                # second phase must read that process's journal without seeding.
                report["initial_instrumentation"] = instrumentation_path.read_text()
                previous = set(remotes("OctoSenseValidation"))
                instrumentation_path = args.output / "recovery-instrumentation.log"
                with instrumentation_path.open("w") as recovery_log:
                    runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "launcher_resume",
                        "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"],
                        stdout=recovery_log, stderr=subprocess.STDOUT)
                home = attach("OctoSenseValidation", previous)
                assert home["pid"] != old_pid, "Recovery reused the killed PID"
                report["owned_pid"] = home["pid"]
                recovered = state("process-recovered", lambda v: launcher(v)["epoch"] != launcher(saved)["epoch"]
                                  and launcher(v)["bridge_state"] == "connected" and geometry(v).get("local_ready", False)
                                  and geometry(v).get("connection") == "connected" and geometry(v).get("home_ready", False))
                for key in ["favorites", "dock"]:
                    assert launcher(recovered)["placements"][key] == launcher(saved)["placements"][key]
                assert not launcher(recovered)["menu"]
                report["process_recreation_tested"] = True
                point = icon("process-recovered-icon")
                capture("/surface", "home-after-process-recovery.png")
            launch_and_return("native-launch", point)

            component_changed = True
            report["disable_bridge"] = service_state(False)
            state("bridge-unavailable", lambda v: launcher(v)["bridge_state"] != "connected" and geometry(v).get("local_ready", False))
            launch_and_return("without-system-bridge", point)
            report["restore_bridge"] = service_state(True)
            component_changed = False
            state("bridge-reconnected", lambda v: launcher(v)["bridge_state"] == "connected")

            report["install_normal_bridge"] = command("install", "-r", str(args.bridge_normal.resolve()))
            missing = state("package-component-removed", lambda v: not launcher(v)["fixture_available"] and geometry(v).get("local_ready", False))
            assert identity in launcher(missing)["placements"]["favorites"] and launcher(missing)["placements"]["dock"][0] == identity
            assert fixture_icon(missing) is None
            selected = menu(point, "Remove from dock", "unavailable-placement-menu")
            capture("/g", "unavailable-placement-menu.png")
            # Close by activity recreation, preserving the unavailable identity.
            previous = set(remotes("OctoSenseValidation"))
            request(home, "/recreate")
            home = attach("OctoSenseValidation", previous)
            state("unavailable-after-recreation", lambda v: not launcher(v)["fixture_available"] and launcher(v)["placements"].get("dock", [""])[0] == identity and geometry(v).get("local_ready", False))
            report["install_validation_bridge"] = command("install", "-r", str(args.bridge_validation.resolve()))
            state("package-component-restored", lambda v: launcher(v)["fixture_available"] and launcher(v)["bridge_state"] == "connected" and geometry(v).get("local_ready", False))
            point = icon("restored-icon")
            tap(menu(point, "Remove from dock", "undock-menu"))
            state("undocked", lambda v: not launcher(v)["menu"] and launcher(v)["placements"]["dock"][0] == "")
            point = icon("favorite-icon")
            tap(menu(point, "Remove from Home", "final-remove-menu"))
            state("removed", lambda v: not launcher(v)["menu"] and identity not in launcher(v)["placements"]["favorites"])
            request(home, "/gq")
            closed = True
            runner.wait(timeout=20)
            output = instrumentation_path.read_text()
            assert "result=pass" in output and "INSTRUMENTATION_CODE: -1" in output, output
            report["result"] = "pass"
        except Exception as exc:
            report["error"] = str(exc) + ("\n" + exc.output if isinstance(exc, subprocess.CalledProcessError) else "")
            raise
        finally:
            if native:
                try:
                    request(native, "/close")
                except OSError:
                    pass
            if component_changed:
                try:
                    report["restore_bridge_after_failure"] = service_state(True)
                except (OSError, subprocess.SubprocessError, AssertionError) as exc:
                    report["component_restore_error"] = str(exc)
            if home and not closed:
                try:
                    request(home, "/quit")
                    runner.wait(timeout=15)
                except (OSError, subprocess.TimeoutExpired):
                    pass
            for forward in forwards:
                command("forward", "--remove", "tcp:" + forward)
            report["restore_normal_bridge"] = command("install", "-r", str(args.bridge_normal.resolve()))
            if "owned_pid" in report:
                deadline = time.monotonic() + 10
                while True:
                    process = subprocess.run(adb + ["shell", "ps", "-p", str(report["owned_pid"]), "-o", "PID="], text=True, capture_output=True, timeout=10)
                    report["owned_pid_exited"] = str(report["owned_pid"]) not in process.stdout.split()
                    if report["owned_pid_exited"] or time.monotonic() >= deadline:
                        break
                    time.sleep(0.1)
            report["instrumentation_cleanup_pending"] = runner.poll() is None
            final = command("shell", "dumpsys", "package", "dev.makepad.octosense.bridge")
            report["component_overrides_restored"] = "enabledComponents:" not in final and "disabledComponents:" not in final
            (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"result": report["result"], "output": str(args.output)}, indent=2))


if __name__ == "__main__":
    main()

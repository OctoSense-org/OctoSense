#!/usr/bin/env python3
"""Exercise real pin requests through owned validation windows, then restore APKs."""
import argparse
import hashlib
import http.client
import json
from pathlib import Path
import re
import subprocess
import time
import urllib.parse
import urllib.request
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    for name in ["home-normal", "home-validation", "bridge-normal", "bridge-validation"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--tap-file", type=Path, help="Optional app-window shortcut coordinates, written after reviewing the actual capture")
    parser.add_argument("--lifecycle", action="store_true", help="Test publisher updates, disable/re-enable, live unpin and placement removal using renderer hit bounds")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]
    owner = str(uuid.uuid4())
    report = {"result": "fail", "owner": owner, "states": {}, "publisher": {}, "restored": {},
              "shortcut_launch_tested": False, "production_placements_modified": False}
    forwards, publishers = [], []
    home = None
    runner = None
    fixture = "dev.makepad.octosense.bridge/dev.makepad.octosense.bridge.validation.LauncherFixtureActivity"
    normal = {"dev.makepad.octosense": args.home_normal, "dev.makepad.octosense.bridge": args.bridge_normal}

    def command(*parts):
        return subprocess.check_output(adb + list(parts), text=True, stderr=subprocess.STDOUT, timeout=90).strip()

    def installed(package):
        path = command("shell", "pm", "path", package).splitlines()[0].removeprefix("package:")
        return command("shell", "sha256sum", path).split()[0]

    def remotes(tag):
        logs = command("logcat", "-d", "-s", tag + ":I", "*:S")
        if tag == "OctoSenseValidation":
            return [(pid, "tcp:" + port, token) for pid, port, token in re.findall(r"--remote pid=(\d+) port=(\d+) token=([a-f0-9-]+)", logs)]
        return [(pid, "localabstract:" + sock, token) for pid, sock, token in re.findall(r"--remote pid=(\d+) socket=([a-z0-9-]+) token=([a-f0-9-]+)", logs)]

    def attach(tag, previous):
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            found = [entry for entry in remotes(tag) if entry not in previous]
            if found:
                pid, endpoint, token = found[-1]
                forward = command("forward", "tcp:0", endpoint)
                forwards.append(forward)
                return {"pid": int(pid), "base": "http://127.0.0.1:" + forward + "/" + token}
            time.sleep(0.2)
        raise AssertionError("Owned remote missing: " + tag)

    def request(remote, route):
        with urllib.request.urlopen(remote["base"] + route, timeout=8) as response:
            return response.read()

    def launcher(value):
        return value["widgets"]["launcher"]

    def state(name, predicate):
        deadline = time.monotonic() + 20
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
        raise AssertionError("Home state timeout: " + name)

    def tap(point):
        request(home, "/tap?" + urllib.parse.urlencode({k: point[k] for k in ["x", "y"]}))

    def probe(name, identity, predicate):
        deadline = time.monotonic() + 20
        token = None
        value = None
        while time.monotonic() < deadline:
            if token is None:
                token = str(uuid.uuid4())
                assert json.loads(request(home, "/launcher/probe?" + urllib.parse.urlencode({"token": token, "identity": identity})))["queued"]
            value = json.loads(request(home, "/status"))
            assert value["pid"] == home["pid"]
            model = launcher(value).get("renderer")
            if model and model.get("token") == token:
                if predicate(model):
                    report["states"][name] = value
                    print("Verified " + name, flush=True)
                    return value
                token = None
            time.sleep(0.2)
        report["states"][name + "-timeout"] = value
        raise AssertionError("Renderer state timeout: " + name)

    def renderer_point(value):
        model = launcher(value)["renderer"]
        bounds = model["bounds"]
        assert bounds is not None
        scale = value["width"] / model["logical_width"]
        offset = value["surface_capture_offset"]
        assert 0.5 <= scale <= 6 and offset is not None
        return {"x": (bounds[0] + bounds[2] / 2) * scale + offset[0],
                "y": (bounds[1] + bounds[3] / 2) * scale + offset[1]}

    def shortcut_point(identity):
        for page in range(5):
            request(home, "/page?index=" + str(page))
            time.sleep(0.35)
            value = probe("shortcut-page-" + str(page), identity, lambda v: True)
            model = launcher(value)["renderer"]
            bounds = model["bounds"]
            if bounds is not None and bounds[0] >= 0 and bounds[0] + bounds[2] <= model["logical_width"]:
                return value
        raise AssertionError("Actual shortcut hit region missing")

    def lifecycle(publisher, identity, original):
        request(publisher, "/background")
        state("publisher-backgrounded", lambda v: v["widgets"]["home_integration"]["local_ready"])
        deadline = time.monotonic() + 10
        while json.loads(request(publisher, "/status"))["resumed"]:
            assert time.monotonic() < deadline
            time.sleep(0.1)
        before_icon = launcher(original)["renderer"]["app"]["icon"]
        def change(operation):
            value = json.loads(request(publisher, "/shortcut?op=" + operation))
            assert value["shortcut_operation"] == operation and not value["operation_error"], value
            report["publisher"][operation] = value
            return value
        changed = change("update")
        assert changed["shortcut_label"] == "Pin updated" and changed["shortcut_enabled"]
        updated = probe("publisher-update-rendered", identity, lambda v: v["app"] and v["app"]["label"] == "Pin updated"
                        and v["app"]["icon"] != before_icon and v["app"]["icon_loaded"])
        assert identity in launcher(updated)["placements"]["favorites"]
        capture("/surface", "shortcut-updated.png")
        disabled_message = "This test shortcut was retired by its app."
        assert not change("disable")["shortcut_enabled"]
        disabled = probe("publisher-disabled-rendered", identity, lambda v: v["app"] and v["app"]["suspended"]
                         and v["app"]["disabled_message"] == disabled_message)
        capture("/surface", "shortcut-disabled.png")
        before = set(remotes("OctoSenseLauncherFixture"))
        tap(renderer_point(disabled))
        probe("disabled-tap-explained", identity, lambda v: v["last_local_notice"]
              and v["last_local_notice"]["title"] == "Shortcut unavailable" and v["last_local_notice"]["body"] == disabled_message)
        time.sleep(0.35)
        capture("/surface", "shortcut-disabled-message.png")
        assert set(remotes("OctoSenseLauncherFixture")) == before, "Disabled shortcut launched a fixture activity"
        assert not json.loads(request(publisher, "/status"))["resumed"]
        assert change("enable")["shortcut_enabled"]
        enabled = probe("publisher-reenabled-rendered", identity, lambda v: v["app"] and not v["app"]["suspended"]
                        and not v["app"]["disabled_message"] and v["app"]["icon_loaded"])
        capture("/surface", "shortcut-reenabled.png")
        request(publisher, "/close")
        publishers.remove(publisher)
        previous = set(remotes("OctoSenseLauncherFixture"))
        tap(renderer_point(enabled))
        launched = attach("OctoSenseLauncherFixture", previous)
        publishers.append(launched)
        report.setdefault("owned_publisher_pids", []).append(launched["pid"])
        observed = json.loads(request(launched, "/status"))
        assert observed["launched_shortcut"] == changed["shortcut_id"] and observed["launched_revision"] == "updated", observed
        report["publisher"]["updated-intent-launch"] = observed
        request(launched, "/close")
        publishers.remove(launched)
        state("updated-shortcut-returned", lambda v: v["widgets"]["home_integration"]["local_ready"])
        assert not json.loads(request(home, "/shortcuts/cleanup?owner=" + str(uuid.uuid4())))["queued"], "Foreign cleanup owner accepted"
        assert json.loads(request(home, "/shortcuts/cleanup?owner=" + owner))["queued"]
        removed = probe("framework-unpin-observed", identity, lambda v: v["app"] is None)
        capture("/surface", "shortcut-unpinned.png")
        if identity in launcher(removed)["placements"]["favorites"]:
            request(home, "/hold?" + urllib.parse.urlencode(renderer_point(removed)))
            menu = state("unavailable-shortcut-menu", lambda v: bool(launcher(v)["menu"]))
            tap(next(item for item in launcher(menu)["menu"] if item["label"] == "Remove from Home"))
        state("shortcut-placement-removed", lambda v: identity not in launcher(v)["placements"]["favorites"])
        after = recreate("removed-shortcut-recreated")
        assert identity not in launcher(after)["placements"]["favorites"]
        fixture_point()
        probe("removed-shortcut-absent", identity, lambda v: v["app"] is None and v["bounds"] is None)
        capture("/surface", "shortcut-removed.png")
        report["shortcut_lifecycle_tested"] = True

    def fixture_point():
        for page in range(5):
            request(home, "/page?index=" + str(page))
            time.sleep(0.35)
            value = state("fixture-page-" + str(page), lambda v: v["widgets"]["home_integration"]["local_ready"])
            icons = value["widgets"]["home_integration"].get("icons", [])
            icon = next((i for i in icons if i["component"] == fixture), None)
            if icon:
                left, top, right, bottom = icon["bounds"]
                return {"x": (left + right) / 2, "y": (top + bottom) / 2}
        raise AssertionError("Actually drawn fixture icon missing")

    def capture(route, name):
        for attempt in range(1, 4):
            try:
                data = request(home, route)
                break
            except (http.client.IncompleteRead, http.client.RemoteDisconnected) as error:
                report.setdefault("capture_transfer_failures", []).append({"name": name, "attempt": attempt, "error": type(error).__name__})
                if attempt == 3:
                    raise
                time.sleep(0.2)
        assert data.startswith(b"\x89PNG\r\n\x1a\n")
        (args.output / name).write_bytes(data)

    def recreate(name):
        nonlocal home
        previous = set(remotes("OctoSenseValidation"))
        old_pid = home["pid"]
        request(home, "/recreate")
        home = attach("OctoSenseValidation", previous)
        assert home["pid"] == old_pid
        return state(name, lambda v: not launcher(v)["pin"]["showing"] and v["widgets"]["home_integration"]["local_ready"])

    # Exact originals are checked before any authorized installation.
    for package, apk in normal.items():
        assert installed(package) == hashlib.sha256(apk.read_bytes()).hexdigest(), "Unexpected installed APK: " + package
    try:
        command("install", "-r", str(args.bridge_validation.resolve()))
        command("install", "-r", str(args.home_validation.resolve()))
        previous = set(remotes("OctoSenseValidation"))
        with (args.output / "instrumentation.log").open("w") as log:
            runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "shortcut_ui",
                "-e", "shortcut_owner", owner, "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"],
                stdout=log, stderr=subprocess.STDOUT)
        home = attach("OctoSenseValidation", previous)
        report["owned_home_pid"] = home["pid"]
        state("ready", lambda v: launcher(v)["fixture_available"])
        for scenario in ["cancel", "recreate", "accept"]:
            previous = set(remotes("OctoSenseLauncherFixture"))
            point = fixture_point()
            before = json.loads(request(home, "/status"))
            tap(point)
            publisher = attach("OctoSenseLauncherFixture", previous)
            publishers.append(publisher)
            report.setdefault("owned_publisher_pids", []).append(publisher["pid"])
            deadline = time.monotonic() + 10
            while not json.loads(request(publisher, "/status"))["resumed"]:
                assert time.monotonic() < deadline
                time.sleep(0.1)
            value = json.loads(request(publisher, "/pin?" + urllib.parse.urlencode({"case": scenario, "owner": owner})))
            assert value["pin_requested"] and not value["pin_error"], value
            identity = "android-shortcut:0:dev.makepad.octosense.bridge:" + value["shortcut_id"]
            prompt = state(scenario + "-prompt", lambda v: launcher(v)["pin"]["showing"] and launcher(v)["pin"].get("identity") == identity)
            assert launcher(prompt)["epoch"] == launcher(before)["epoch"], "Pin confirmation replaced the Makepad renderer"
            if scenario == "recreate":
                old_instance = launcher(prompt)["pin"]["instance"]
                assert json.loads(request(home, "/pin/recreate"))["queued"]
                after = state("native-prompt-recreated", lambda v: launcher(v)["pin"]["showing"]
                              and launcher(v)["pin"].get("instance") != old_instance)
                assert launcher(after)["epoch"] == launcher(before)["epoch"]
                tap(next(b for b in launcher(after)["pin"]["buttons"] if b["label"].lower() == "cancel"))
            else:
                label = "Add" if scenario == "accept" else "Cancel"
                if scenario == "accept":
                    capture("/g", "pin-confirmation.png")
                selected = next(b for b in launcher(prompt)["pin"]["buttons"] if b["label"].lower() == label.lower())
                tap(selected)
            deadline = time.monotonic() + 10
            while True:
                observed = json.loads(request(publisher, "/status"))
                if scenario != "accept" or (observed["pin_accepted"] and observed["pin_persisted"]):
                    break
                assert time.monotonic() < deadline, observed
                time.sleep(0.1)
            assert observed["pin_accepted"] == (scenario == "accept")
            assert observed["pin_persisted"] == (scenario == "accept")
            report["publisher"][scenario] = observed
            request(publisher, "/close")
            publishers.remove(publisher)
            returned = state(scenario + "-closed", lambda v: not launcher(v)["pin"]["showing"]
                             and v["widgets"]["home_integration"]["local_ready"])
            if scenario != "accept":
                assert identity not in launcher(returned)["placements"]["favorites"]
            if scenario == "accept":
                state("pin-placed", lambda v: identity in launcher(v)["placements"]["favorites"])
                after = recreate("accepted-placement-recreated")
                assert identity in launcher(after)["placements"]["favorites"]
                if args.lifecycle:
                    original = shortcut_point(identity)
                    original = probe("initial-shortcut-rendered", identity, lambda v: v["app"] and v["app"]["icon_loaded"] and v["bounds"] is not None)
                else:
                    fixture_point()
                capture("/surface", "pinned-shortcut-surface.png")
                if args.tap_file or args.lifecycle:
                    if args.lifecycle:
                        point = renderer_point(original)
                    else:
                        print("Awaiting inspected shortcut coordinates in " + str(args.tap_file), flush=True)
                        deadline = time.monotonic() + 60
                        while not args.tap_file.exists():
                            assert time.monotonic() < deadline, "No inspected shortcut coordinate supplied"
                            time.sleep(0.25)
                        point = json.loads(args.tap_file.read_text())
                    previous = set(remotes("OctoSenseLauncherFixture"))
                    tap(point)
                    launched = attach("OctoSenseLauncherFixture", previous)
                    publishers.append(launched)
                    report.setdefault("owned_publisher_pids", []).append(launched["pid"])
                    observed = json.loads(request(launched, "/status"))
                    assert observed["launched_shortcut"] == value["shortcut_id"], observed
                    report["shortcut_launch_tested"] = True
                    report["shortcut_launch"] = observed
                    if args.lifecycle:
                        lifecycle(launched, identity, original)
                    else:
                        request(launched, "/close")
                        publishers.remove(launched)
                        state("shortcut-returned", lambda v: v["widgets"]["home_integration"]["local_ready"])
        request(home, "/close")
        runner.wait(timeout=20)
        log = (args.output / "instrumentation.log").read_text()
        assert "result=pass" in log and "owned_shortcuts_unpinned=true" in log, log
        report["result"] = "pass"
        report["visual_review_required"] = True
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        for publisher in publishers:
            try:
                request(publisher, "/close")
            except OSError:
                pass
        if home:
            try:
                request(home, "/close")
            except OSError:
                pass
        if runner is not None and runner.poll() is None:
            try:
                runner.wait(timeout=20)
            except subprocess.TimeoutExpired:
                report["instrumentation_cleanup"] = "owned runner did not exit before APK restoration"
        # The owner identifies three fixture shortcuts only, including recovery
        # when the first instrumentation process failed before its finally block.
        cleanup_errors = []
        try:
            report["cleanup"] = command("shell", "am", "instrument", "-w", "-e", "mode", "shortcut_cleanup",
                "-e", "shortcut_owner", owner, "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation")
            assert "owned_shortcuts_unpinned=true" in report["cleanup"] and "result=pass" in report["cleanup"]
        except Exception as error:
            cleanup_errors.append("shortcuts: " + str(error))
        for package, apk in normal.items():
            try:
                command("install", "-r", str(apk.resolve()))
                report["restored"][package] = installed(package)
                assert report["restored"][package] == hashlib.sha256(apk.read_bytes()).hexdigest()
            except Exception as error:
                cleanup_errors.append(package + ": " + str(error))
        for forward in forwards:
            try:
                command("forward", "--remove", "tcp:" + forward)
            except Exception as error:
                cleanup_errors.append("forward: " + str(error))
        report["owned_pids_exited"] = {}
        for pid in set(report.get("owned_publisher_pids", []) + [report.get("owned_home_pid", 0)]) - {0}:
            process = subprocess.run(adb + ["shell", "ps", "-p", str(pid), "-o", "PID="],
                                     capture_output=True, text=True, timeout=10)
            report["owned_pids_exited"][str(pid)] = str(pid) not in process.stdout.split()
            if not report["owned_pids_exited"][str(pid)]:
                cleanup_errors.append("Owned PID still present: " + str(pid))
        try:
            command("shell", "am", "start", "-a", "android.intent.action.MAIN", "-c", "android.intent.category.HOME",
                    "-n", "dev.makepad.octosense/.MakepadApp")
        except Exception as error:
            cleanup_errors.append("Home restore: " + str(error))
        if cleanup_errors:
            report["result"] = "fail"
            report["cleanup_errors"] = cleanup_errors
        (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"result": report["result"], "owner": owner, "restored": report["restored"]}), flush=True)
        if cleanup_errors:
            raise RuntimeError("Cleanup needs attention: " + str(cleanup_errors))


if __name__ == "__main__":
    main()

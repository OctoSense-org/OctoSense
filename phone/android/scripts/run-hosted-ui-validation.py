#!/usr/bin/env python3
"""Exercise hosted Home and dock placement through owned windows, then restore APKs."""
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
    parser.add_argument("--points", type=Path, required=True, help="JSON map of capture names to inspected app-window x/y coordinates")
    parser.add_argument("--app", default="appcard")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]
    owner = str(uuid.uuid4())
    report = {"result": "fail", "owner": owner, "states": {}, "restored": {},
              "hosted_app": args.app, "production_placements_modified": False}
    forwards = []
    home = None
    runner = None
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
        time.sleep(0.15)

    def menu(point, label, name):
        request(home, "/hold?" + urllib.parse.urlencode(point))
        time.sleep(1.1)
        value = state(name, lambda v: any(item["label"] == label for item in launcher(v)["menu"]))
        return next(item for item in launcher(value)["menu"] if item["label"] == label)

    def inspected(name):
        capture("/surface", name + ".png")
        print("Inspect " + str(args.output / (name + ".png")) + "; supply point key " + name, flush=True)
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if args.points.exists():
                points = json.loads(args.points.read_text())
                if name in points:
                    point = {k: points[name][k] for k in ["x", "y"]}
                    report.setdefault("inspected_points", {})[name] = point
                    return point
            time.sleep(0.25)
        raise AssertionError("No inspected coordinate for " + name)

    def capture(route, name):
        assert route in ("/g", "/surface")
        for attempt in range(1, 4):
            try:
                data = request(home, route)
                break
            except http.client.IncompleteRead as error:
                report.setdefault("capture_transfer_failures", []).append({
                    "name": name, "attempt": attempt, "received_bytes": len(error.partial),
                    "missing_bytes": error.expected, "pid": home["pid"]})
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
        report["placement_storage"] = command("shell", "am", "instrument", "-w", "-e", "mode", "placements",
            "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation")
        assert "result=pass" in report["placement_storage"], report["placement_storage"]
        previous = set(remotes("OctoSenseValidation"))
        with (args.output / "instrumentation.log").open("w") as log:
            runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "launcher_ui", "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"],
                stdout=log, stderr=subprocess.STDOUT)
        home = attach("OctoSenseValidation", previous)
        report["owned_home_pid"] = home["pid"]
        state("ready", lambda v: launcher(v)["fixture_available"])
        request(home, "/page?index=0")
        time.sleep(0.5)
        initial = state("initial-home", lambda v: v["widgets"]["home_integration"]["local_ready"])
        native_favorites = launcher(initial)["placements"]["favorites"]
        point = inspected("hosted-home")
        selected = menu(point, "Place in dock position 1", "hosted-menu")
        capture("/g", "hosted-placement-menu.png")
        tap(selected)
        state("docked-first", lambda v: not launcher(v)["menu"] and launcher(v)["placements"]["dock"][0] == args.app)
        point = inspected("hosted-dock-first")
        tap(menu(point, "Remove from Home", "hosted-hide-menu"))
        state("hidden-while-docked", lambda v: not launcher(v)["menu"] and args.app in launcher(v)["placements"]["hidden_hosted"])
        tap(menu(point, "Place in dock position 4", "hosted-move-menu"))
        saved = state("docked-fourth", lambda v: not launcher(v)["menu"] and launcher(v)["placements"]["dock"][3] == args.app
                      and launcher(v)["placements"]["dock"][0] == "")
        point = inspected("hosted-dock-fourth")
        after = recreate("hosted-recreated")
        for key in ["favorites", "dock", "hidden_hosted"]:
            assert launcher(after)["placements"][key] == launcher(saved)["placements"][key]
        assert launcher(after)["epoch"] != launcher(saved)["epoch"]
        capture("/surface", "hosted-after-recreation.png")
        tap(menu(point, "Remove from dock", "hosted-undock-menu"))
        state("hosted-removed", lambda v: not launcher(v)["menu"] and args.app not in launcher(v)["placements"]["dock"]
              and args.app in launcher(v)["placements"]["hidden_hosted"])
        capture("/surface", "hosted-hidden.png")
        request(home, "/page?index=32")
        time.sleep(0.7)
        point = inspected("hosted-drawer")
        tap(menu(point, "Add to Home", "hosted-add-menu"))
        state("hosted-added", lambda v: not launcher(v)["menu"] and args.app not in launcher(v)["placements"]["hidden_hosted"])
        request(home, "/page?index=0")
        time.sleep(0.5)
        restored = state("hosted-restored", lambda v: v["widgets"]["home_integration"]["local_ready"])
        assert launcher(restored)["placements"]["favorites"] == native_favorites
        capture("/surface", "hosted-restored.png")
        report["activity_recreation_tested"] = True
        request(home, "/close")
        runner.wait(timeout=20)
        log = (args.output / "instrumentation.log").read_text()
        assert "result=pass" in log and "INSTRUMENTATION_CODE: -1" in log, log
        report["result"] = "pass"
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
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
        cleanup_errors = []
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
        for pid in set([report.get("owned_home_pid", 0)]) - {0}:
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

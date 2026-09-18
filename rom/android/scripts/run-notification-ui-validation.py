#!/usr/bin/env python3
"""Inspect synthetic notification cards with real PackageManager identity, then restore APKs."""
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
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]
    owner = str(uuid.uuid4())
    report = {"result": "fail", "owner": owner, "states": {}, "restored": {},
              "notification_listener_access_granted": False, "synthetic_notification_content": True, "production_placements_modified": False}
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

    def renderer(name, predicate):
        # Read the actual Rust model after a uniquely identified event. The
        # Java snapshot alone does not prove delivery, decoding, or shade state.
        deadline = time.monotonic() + 20
        value = None
        token = None
        while time.monotonic() < deadline:
            if token is None:
                token = str(uuid.uuid4())
                assert json.loads(request(home, "/notification/probe?token=" + token))["queued"]
            value = json.loads(request(home, "/status"))
            assert value["pid"] == home["pid"]
            model = launcher(value).get("notification_renderer")
            if model and model.get("token") == token:
                if predicate(model):
                    report["states"][name] = value
                    print("Verified " + name, flush=True)
                    return value
                token = None
            time.sleep(0.2)
        report["states"][name + "-timeout"] = value
        raise AssertionError("Renderer state timeout: " + name)

    def capture(route, name):
        assert route in ("/g", "/surface")
        for attempt in range(1, 4):
            try:
                data = request(home, route)
                break
            except (http.client.IncompleteRead, http.client.RemoteDisconnected) as error:
                report.setdefault("capture_transfer_failures", []).append({
                    "name": name, "attempt": attempt, "error": type(error).__name__,
                    "received_bytes": len(getattr(error,"partial",b"")),
                    "missing_bytes": getattr(error,"expected",None), "pid": home["pid"]})
                if attempt == 3:
                    raise
                time.sleep(0.2)
        assert data.startswith(b"\x89PNG\r\n\x1a\n")
        (args.output / name).write_bytes(data)

    # Exact originals are checked before any authorized installation.
    for package, apk in normal.items():
        assert installed(package) == hashlib.sha256(apk.read_bytes()).hexdigest(), "Unexpected installed APK: " + package
    try:
        command("install", "-r", str(args.bridge_validation.resolve()))
        command("install", "-r", str(args.home_validation.resolve()))
        previous = set(remotes("OctoSenseValidation"))
        with (args.output / "instrumentation.log").open("w") as log:
            runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "notification_ui", "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"],
                stdout=log, stderr=subprocess.STDOUT)
        home = attach("OctoSenseValidation", previous)
        report["owned_home_pid"] = home["pid"]
        state("ready", lambda v: launcher(v)["fixture_available"] and launcher(v)["notification_fixture"]
              and v["widgets"]["home_integration"]["local_ready"])
        time.sleep(0.5)
        capture("/surface", "notification-home.png")
        initial = renderer("shade-closed", lambda v: v["shade_open"] == 0 and v["open_bounds"] is not None)
        probe = launcher(initial)["notification_renderer"]
        bounds = probe["open_bounds"]
        scale = initial["width"] / probe["logical_width"]
        offset = initial["surface_capture_offset"]
        assert offset is not None and 0.5 <= scale <= 6
        point = {"x": (bounds[0] + bounds[2] / 2) * scale + offset[0],
                 "y": (bounds[1] + bounds[3] / 2) * scale + offset[1]}
        report["notification_open_tap"] = {"logical_bounds": bounds, "scale": scale, "offset": offset, "window": point}
        request(home, "/notification/known")
        known = state("known-package", lambda v: len(launcher(v)["notification_presentation"]) == 1
                      and launcher(v)["notification_presentation"][0]["app_icon"].endswith(".png"))
        model = launcher(known)["notification_presentation"][0]
        assert model["app_label"] not in [model["package"], "Untrusted label must be replaced"]
        assert "/notification-validation-icons/" in model["app_icon"]
        report["resolved_app_label"] = model["app_label"]
        tap(point)
        rendered = renderer("known-rendered", lambda v: v["shade_open"] >= 0.99 and len(v["notes"]) == 1
                            and v["notes"][0]["icon_loaded"] and v["notes"][0]["app_label"] == model["app_label"])
        note_id = launcher(rendered)["notification_renderer"]["notes"][0]["id"]
        capture("/surface", "notification-known.png")
        request(home, "/notification/update")
        updated = state("content-updated", lambda v: launcher(v)["notification_presentation"][0]["title"] == "Updated notification")
        assert launcher(updated)["notification_presentation"][0]["handle"] == model["handle"]
        assert launcher(updated)["notification_presentation"][0]["app_icon"] == model["app_icon"]
        renderer("updated-rendered", lambda v: v["shade_open"] >= 0.99 and len(v["notes"]) == 1
                 and v["notes"][0]["id"] == note_id and v["notes"][0]["title"] == "Updated notification"
                 and v["notes"][0]["icon_loaded"])
        capture("/surface", "notification-updated.png")
        request(home, "/notification/missing")
        missing = state("package-missing", lambda v: launcher(v)["notification_presentation"][0]["package"] == "dev.makepad.octosense.fixture.missing")
        fallback = launcher(missing)["notification_presentation"][0]
        assert fallback["app_label"] == fallback["package"] and fallback["app_icon"] == ""
        renderer("missing-rendered", lambda v: v["shade_open"] >= 0.99 and len(v["notes"]) == 1
                 and v["notes"][0]["app_label"] == fallback["package"] and not v["notes"][0]["icon_loaded"])
        capture("/surface", "notification-missing.png")
        request(home, "/notification/clear")
        state("removed", lambda v: not launcher(v)["notification_presentation"])
        renderer("removed-rendered", lambda v: v["shade_open"] >= 0.99 and not v["notes"])
        capture("/surface", "notification-cleared.png")
        request(home, "/close")
        runner.wait(timeout=20)
        log = (args.output / "instrumentation.log").read_text()
        assert "result=pass" in log and "INSTRUMENTATION_CODE: -1" in log, log
        report["result"] = "pass"
        report["visual_review_required"] = True
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

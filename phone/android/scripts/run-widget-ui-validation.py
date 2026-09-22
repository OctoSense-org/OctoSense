#!/usr/bin/env python3
"""Inspect an installed validation Home through its app-owned loopback remote.

Requires an authorized device and previously authorized validation APK install.
Uses a disposable clock widget; restores its host/journal in instrumentation.
Captures the app's two drawing surfaces separately, never the system display.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time
import urllib.error
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--done-tap", type=float, nargs=2, metavar=("X", "Y"),
                        help="Done button center inspected in this app's workspace capture, in window pixels")
    parser.add_argument("--geometry", action="store_true", help="Also require actual native-app icon geometry from the isolated Home placement")
    parser.add_argument("--require-quickstep", action="store_true", help="Require the installed layout service to acknowledge Home geometry and readiness")
    parser.add_argument("--invalidate-layout", action="store_true", help="Use previously authorized ADB su to send a package-change broadcast only to the owned Quickstep service and verify Home republishes geometry")
    args = parser.parse_args()
    if args.require_quickstep and not args.geometry:
        parser.error("--require-quickstep requires --geometry")
    if args.invalidate_layout and not args.require_quickstep:
        parser.error("--invalidate-layout requires --require-quickstep")
    args.output.mkdir(parents=True, exist_ok=True)
    adb = [args.adb, "-s", args.serial]

    def command(*parts):
        return subprocess.check_output(adb + list(parts), text=True, timeout=15).strip()

    def remotes():
        log = command("logcat", "-d", "-s", "OctoSenseValidation:I", "*:S")
        return re.findall(r"--remote pid=(\d+) port=(\d+) token=([a-f0-9-]+)", log)

    previous = set(remotes())
    report = {"serial": args.serial, "statuses": {}, "captures": [], "result": "fail",
              "capture_scope": "separate app Window and Makepad SurfaceView; no display/compositor capture",
              "consent_ui_tested": False, "performance_parity_tested": False}
    forward = None
    base = None
    closed = False
    with (args.output / "instrumentation.log").open("w") as log:
        runner = subprocess.Popen(adb + ["shell", "am", "instrument", "-w", "-e", "mode", "widget_ui",
            "dev.makepad.octosense/dev.makepad.octosense.validation.BridgeInstrumentation"], stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 75
            while time.monotonic() < deadline:
                found = [item for item in remotes() if item not in previous]
                if found:
                    pid, port, token = found[-1]
                    report["owned_pid"] = int(pid)
                    forward = command("forward", "tcp:0", "tcp:" + port)
                    base = "http://127.0.0.1:" + forward + "/" + token
                    break
                if runner.poll() is not None:
                    raise RuntimeError("Instrumentation ended before remote startup; see instrumentation.log")
                time.sleep(0.5)
            if base is None:
                raise RuntimeError("Owned validation remote did not start")

            def request(route):
                with urllib.request.urlopen(base + route, timeout=10) as response:
                    return response.read()

            def status(name, predicate=lambda value: True):
                until = time.monotonic() + 15
                error = None
                last = None
                while time.monotonic() < until:
                    try:
                        value = json.loads(request("/status"))
                        last = value
                        assert value["pid"] == report["owned_pid"], "Owned PID changed"
                        if predicate(value):
                            report["statuses"][name] = value
                            return value
                    except (OSError, ValueError) as exc:
                        error = str(exc)
                    time.sleep(0.25)
                report["statuses"][name + "-timeout"] = last
                raise RuntimeError("State timeout: " + name + ": " + str(error))

            def capture(route, filename):
                # Native view readiness can precede Makepad's first GPU frame.
                # Retry only an unavailable app drawable, for a bounded period.
                until = time.monotonic() + 15
                while True:
                    try:
                        data = request(route)
                        break
                    except OSError:
                        if route == "/gq" or time.monotonic() >= until:
                            raise
                        time.sleep(0.25)
                assert data.startswith(b"\x89PNG\r\n\x1a\n"), "Capture is not a PNG"
                (args.output / filename).write_bytes(data)
                report["captures"].append(filename)

            status("ready", lambda v: v["widgets"]["native_views"] == 1)
            try:
                urllib.request.urlopen("http://127.0.0.1:" + forward + "/wrong-token/status", timeout=3)
                raise AssertionError("Remote accepted wrong token")
            except urllib.error.HTTPError as exc:
                assert exc.code == 403
                report["wrong_token_rejected"] = True
            request("/page?index=0")
            status("home", lambda v: not v["widgets"]["pages_visible"])
            if args.geometry:
                def geometry_ready(value):
                    geometry = value["widgets"]["home_integration"]
                    if not geometry.get("local_ready", False):
                        return False
                    return not args.require_quickstep or (
                        geometry.get("connection") == "connected"
                        and geometry.get("home_ready", False)
                        and geometry.get("layout_revision") == geometry.get("local_revision")
                        and geometry.get("icon_count") == len(geometry.get("icons", [])))

                found=False
                for index in range(5):
                    request("/page?index=" + str(index))
                    time.sleep(0.5)
                    value=status("geometry-page-"+str(index),geometry_ready)
                    if value["widgets"]["home_integration"].get("icons",[]):
                        report["native_icon_page"]=index;found=True;break
                assert found,"No visible native app icon was published on Home pages"
                geometry=value["widgets"]["home_integration"]
                assert not geometry["controllers_implemented"] and not geometry["transitions_validated"]
                clock=[icon for icon in geometry["icons"] if icon["component"].startswith("com.android.deskclock/")]
                assert len(clock)==1 and clock[0]["user"]>=0
                left,top,right,bottom=clock[0]["bounds"]
                assert 0<=left<right<=value["width"] and 0<=top<bottom<=value["height"]
                report["native_clock_icon_geometry_verified"]=True
                report["quickstep_acknowledged_geometry"] = args.require_quickstep
                if args.invalidate_layout:
                    # Target only our service: Home's independent catalog
                    # listener must not hide a missing invalidation callback.
                    before_generation=geometry["generation"]
                    before_revision=geometry["layout_revision"]
                    broadcast=command("shell","su","-c",
                        "am broadcast --user 0 -a android.intent.action.PACKAGE_CHANGED "
                        "-d package:dev.makepad.octosense -p dev.makepad.octosense.quickstep --receiver-include-background")
                    (args.output / "layout-invalidation-broadcast.txt").write_text(broadcast+"\n")
                    assert "Broadcast completed" in broadcast,broadcast
                    value=status("geometry-after-service-invalidation",lambda v: geometry_ready(v)
                        and v["widgets"]["home_integration"]["generation"]>before_generation
                        and v["widgets"]["home_integration"]["layout_revision"]>before_revision)
                    assert value["widgets"]["home_integration"]["icons"]==geometry["icons"],"Republication changed unchanged icon geometry"
                    report["service_invalidation_republished_geometry"]=True
                    report["invalidation_stimulus"]="ADB-root protected broadcast scoped to Quickstep; no package mutation"
                capture("/surface","native-icon-page-surface.png")
                request("/page?index=0")
                status("home-after-geometry",lambda v: not v["widgets"]["pages_visible"])
            capture("/surface", "home-surface.png")
            widget_page = None
            for index in range(1, 6):
                request("/page?index=" + str(index))
                time.sleep(0.6)
                value = status("page-" + str(index))
                if value["widgets"]["pages_visible"]:
                    widget_page = index
                    break
            assert widget_page is not None, "Bound widget never appears on a Home page"
            report["widget_page"] = widget_page
            visible = [p for p in value["widgets"]["pages"] if p["visible"]]
            assert len(visible) == 1
            placed = visible[0]
            assert placed["width"] > 0 and placed["height"] > 0
            assert 0 <= placed["x"] < value["width"] and 0 <= placed["y"] < value["height"]
            assert placed["x"] + placed["width"] <= value["width"] + 1
            assert placed["y"] + placed["height"] <= value["height"] + 1
            capture("/surface", "widget-page-surface.png")
            capture("/g", "widget-page-native.png")
            request("/widgets")
            status("workspace", lambda v: v["widgets"]["workspace"] and not v["widgets"]["pages_visible"])
            if args.geometry:
                status("covered-home-geometry", lambda v:
                       not v["widgets"]["home_integration"]["local_ready"]
                       and (not args.require_quickstep or (
                           v["widgets"]["home_integration"].get("connection") == "connected"
                           and v["widgets"]["home_integration"].get("home_ready") is False)))
                report["quickstep_acknowledged_covered_home"] = args.require_quickstep
            time.sleep(0.5)
            capture("/g", "workspace-native.png")
            if args.done_tap:
                request("/tap?x=" + str(args.done_tap[0]) + "&y=" + str(args.done_tap[1]))
            else:
                request("/home")
            status("returned-to-widget", lambda v: not v["widgets"]["workspace"] and v["widgets"]["pages_visible"])
            report["native_done_button_input_tested"] = args.done_tap is not None
            request("/page?index=0")
            status("left-widget", lambda v: not v["widgets"]["pages_visible"])
            capture("/gq", "closing-native.png")
            closed = True
            runner.wait(timeout=25)
            output = (args.output / "instrumentation.log").read_text()
            assert "result=pass" in output and "INSTRUMENTATION_CODE: -1" in output, output
            report["result"] = "pass"
        except Exception as exc:
            report["error"] = str(exc)
            raise
        finally:
            if base and not closed:
                try:
                    urllib.request.urlopen(base + "/quit", timeout=8).read()
                except OSError:
                    pass
                try:
                    runner.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    pass
            if forward:
                command("forward", "--remove", "tcp:" + forward)
            if runner.poll() is None:
                # The fixture itself has a 180-second cleanup deadline. Leave
                # it able to release its ID, even if remote startup failed.
                report["instrumentation_cleanup_pending"] = True
            if "owned_pid" in report:
                until=time.monotonic()+10
                while True:
                    process = subprocess.run(adb + ["shell", "ps", "-p", str(report["owned_pid"]), "-o", "PID="], capture_output=True, text=True, timeout=10)
                    report["owned_pid_exited"] = str(report["owned_pid"]) not in process.stdout.split()
                    if report["owned_pid_exited"] or time.monotonic()>=until:break
                    time.sleep(0.1)
            (args.output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"result": report["result"], "output": str(args.output)}, indent=2))


if __name__ == "__main__":
    main()

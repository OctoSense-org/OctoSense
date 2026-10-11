"""Capture linear drawer drags in an installed OctoSense TEST package.

Requires a compiled/dexed TouchLatency.jar on the selected device. Restarts
only the explicitly named package, opens All apps using page:999, records
phone.frames/android.present, and writes JSON + log for input_latency.py.
Coordinates default to the 1080x2280 OnePlus fixture. No screen capture or
system setting changes are used. Do not run against production Home.
"""
import argparse
import json
from pathlib import Path
import re
import shlex
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--serial", required=True)
    parser.add_argument("--test-package", required=True)
    parser.add_argument("--device-jar", required=True)
    parser.add_argument("--injector-cpu-mask",
                        help="Optional taskset hex mask for the injector only; recorded in results")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--count", type=int, default=8)
    parser.add_argument("--x", type=int, default=530)
    parser.add_argument("--start-y", type=int, default=1750)
    parser.add_argument("--end-y", type=int, default=550)
    args = parser.parse_args()
    if not 1 <= args.count <= 100 or args.start_y <= args.end_y:
        parser.error("Count must be 1..100; start-y must exceed end-y")
    if args.injector_cpu_mask and (not re.fullmatch(r"[0-9a-fA-F]+", args.injector_cpu_mask)
                                  or int(args.injector_cpu_mask, 16) == 0):
        parser.error("Injector CPU mask must be nonzero hexadecimal")
    package = args.test_package
    if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+", package):
        parser.error("Invalid Android package name")
    if package == "dev.makepad.octosense":
        parser.error("Use a separate test package, not production Home")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    if args.output.exists() or args.output.with_suffix(".log").exists():
        parser.error("Output already exists; preserve the previous measurement")

    def adb(*command):
        return subprocess.check_output(
            [args.adb, "-s", args.serial, *command], text=True, timeout=35
        )

    def shell(*command):
        return adb("shell", shlex.join([str(arg) for arg in command]))

    def foreground():
        activity = shell("dumpsys", "activity", "activities")
        if not any(package + "/" in line and
                   ("topResumedActivity=" in line or "mResumedActivity:" in line)
                   for line in activity.splitlines()):
            raise RuntimeError("Test package lost foreground; stop this measurement")

    def logs():
        pid = shell("pidof", package).strip()
        if not pid.isdecimal():
            raise RuntimeError("Expected exactly one test process")
        return adb("logcat", "-d", "--pid=" + pid, "-v", "brief")

    def frames(log):
        return [{key: value if key == "screen" else float(value)
                 for key, value in re.findall(r"(\w+)=([^ ]+)", line)}
                for line in log.splitlines() if "[phone.frames]" in line]

    shell("am", "force-stop", package)
    shell("am", "start", "-W", "-n", package + "/.MakepadApp",
          "--es", "makepad.TRACE", "phone.frames,android.present",
          "--es", "makepad.APP_CONFIG", json.dumps({"test_actions": ["page:999"]}))
    time.sleep(5)
    scenarios = []
    try:
        for i in range(args.count):
            foreground()
            if i:
                shell("input", "swipe", args.x, args.end_y, args.x, args.start_y, 160)
                time.sleep(1.3)
            before = frames(logs())[-1]
            if (before["screen"] != "Drawer" or before["search_scroll"] != 0
                    or before["search_stretch"] != 0):
                raise RuntimeError("Drawer must begin at its settled top")
            points = [f"{args.x},{args.start_y + (args.end_y - args.start_y) * t / 1200},{t}"
                      for t in range(0, 1201, 8)]
            points.append(f"{args.x},{args.end_y},1320")
            command = ["env", "CLASSPATH=" + args.device_jar,
                       "app_process", "/system/bin", "TouchLatency", *points]
            if args.injector_cpu_mask:
                command = ["taskset", args.injector_cpu_mask, *command]
            injection = shell(*command)
            time.sleep(.2)
            foreground()
            log = logs()
            scenarios.append({"name": f"linear-drag-{i}", "before": before,
                              "frames": [f for f in frames(log) if f["ns"] > before["ns"]],
                              "injector_cpu_mask": args.injector_cpu_mask,
                              "injection": injection})
            args.output.write_text(json.dumps(scenarios, indent=2))
            args.output.with_suffix(".log").write_text(log)
            print(f"Captured drag {i + 1}/{args.count}", flush=True)
    finally:
        # Finish with trace topics disabled in the same test package.
        shell("am", "force-stop", package)
        shell("am", "start", "-W", "-n", package + "/.MakepadApp",
              "--es", "makepad.TRACE", "off",
              "--es", "makepad.APP_CONFIG", json.dumps({"test_actions": ["page:999"]}))


if __name__ == "__main__":
    main()

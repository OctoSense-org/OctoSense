#!/usr/bin/env python3
"""Exercise the developer-only studio renderer in an already installed test APK.

Requires a --dev-mode build with MAKEPAD_FORCE_DEBUGGABLE=1. Never installs,
flashes, clears app data, changes the default Home, or accesses another package.
The supplied device must be unlocked and app startup permission prompts resolved.
Rendering needs no location grant; location can stay denied. This probes the real executor and GPU;
it does not test model calls, image delivery to a model, or ROM capture.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import struct
import subprocess
import sys
import threading
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PACKAGE = "dev.makepad.octosense.studio"
FIXTURE = ROOT / "apps/calendar/host-service/resources/agenda.card"
PNG_LIMIT = 5 * 1024 * 1024


class ProbeError(RuntimeError):
    pass


def test_package(value):
    # Keep this utility incapable of selecting Home, Bridge, or an unrelated app.
    if not re.fullmatch(r"dev\.makepad\.octosense\.studio(?:\.[A-Za-z][A-Za-z0-9_]*)*", value):
        raise argparse.ArgumentTypeError("use dev.makepad.octosense.studio or a child test package")
    return value


def png_metadata(data):
    if len(data) < 33 or len(data) > PNG_LIMIT or data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ProbeError("renderer output is not a bounded PNG")
    if data[12:16] != b"IHDR":
        raise ProbeError("PNG has no leading IHDR")
    width, height = struct.unpack(">II", data[16:24])
    if not width or not height:
        raise ProbeError("PNG has empty dimensions")
    return {"width": width, "height": height, "bytes": len(data),
            "sha256": hashlib.sha256(data).hexdigest()}


def output_name(value):
    if not isinstance(value, str) or not re.fullmatch(r"\.studio-[0-9a-f-]+\.png", value):
        raise ProbeError("renderer returned an unexpected output path")
    return value


def display_metrics(size_text, density_text):
    """The probe targets a portrait phone window; respect adb display overrides."""
    def effective(text, pattern):
        values = dict(re.findall(pattern, text))
        value = values.get("Override", values.get("Physical"))
        if value is None:
            raise ProbeError("wm did not report usable display metrics")
        return value
    size = effective(size_text, r"(Physical|Override) size:\s*(\d+x\d+)")
    density = int(effective(density_text, r"(Physical|Override) density:\s*(\d+)"))
    width, height = map(int, size.split("x"))
    scale = density / 160.0
    if not 0.5 <= scale <= 4 or width > height or width / scale - 40 < 240:
        raise ProbeError("probe requires a portrait phone display with at least 240 logical pixels of card width")
    return {"screen_width_px": width, "screen_height_px": height, "density_dpi": density,
            "layout_scale": scale, "expected_card_width_px": width - 40 * scale,
            "min_card_height_px": 72 * scale, "max_card_height_px": 440 * scale,
            "width_tolerance_px": 4, "assumption": "portrait full-window layout; no Makepad DPI override"}


def check_png_geometry(png, metrics):
    if abs(png["width"] - metrics["expected_card_width_px"]) > metrics["width_tolerance_px"]:
        raise ProbeError(f"PNG width {png['width']} differs from display-derived glance width "
                         f"{metrics['expected_card_width_px']} (tolerance 4px)")
    if not metrics["min_card_height_px"] - 1 <= png["height"] <= metrics["max_card_height_px"] + 1:
        raise ProbeError("PNG height is outside the 72..440 logical-pixel glance bounds at display density")


def runtime_profile(lines, package):
    """Use the app mount namespace's canonical path, never adb's substitute."""
    matches = [re.search(r"studio-test-profile: home=(\S+) canonical=(\S+) build=(\S+)", line)
               for line in lines if "studio-test-profile:" in line]
    if len(matches) != 1 or matches[0] is None:
        raise ProbeError("expected one complete studio-test-profile diagnostic")
    home, canonical, build = matches[0].groups()
    allowed = {f"/data/data/{package}/files/.octosense", f"/data/user/0/{package}/files/.octosense"}
    if home not in allowed or canonical not in allowed:
        raise ProbeError("runtime profile path is outside this test package")
    if build != "Development":
        raise ProbeError("studio probe requires a Development build (compile with --dev-mode)")
    return {"home": home, "canonical": canonical, "build": build}


class Device:
    def __init__(self, adb, serial, package):
        self.prefix = [adb, "-s", serial]
        self.package = package

    def call(self, *args, data=None, check=True, timeout=15):
        result = subprocess.run(self.prefix + list(args), input=data, capture_output=True, timeout=timeout)
        if check and result.returncode:
            raise ProbeError(f"adb {args[0]} failed: {result.stderr.decode(errors='replace').strip()}")
        return result

    def shell(self, *args, **kwargs):
        return self.call("shell", shlex.join(args), **kwargs)

    def private(self, script, **kwargs):
        # Shell v2 preserves stdin EOF, binary stdout, stderr, and remote exit
        # status. exec-out loses the status and can leave stdin writers waiting.
        return self.call("shell", "-T", shlex.join(["run-as", self.package, "sh", "-c", script]), **kwargs)

    def read(self, path, limit=8192):
        quoted = shlex.quote(path)
        result = self.private(f"if [ ! -e {quoted} ]; then exit 44; fi; head -c {limit + 1} {quoted}", check=False)
        if result.returncode == 44:
            return None
        if result.returncode:
            raise ProbeError("private read failed: " + result.stderr.decode(errors="replace").strip())
        if len(result.stdout) > limit:
            raise ProbeError(f"private result exceeds {limit} bytes")
        return result.stdout

    def write(self, path, data):
        self.private(f"umask 077; cat > {shlex.quote(path)}", data=data)

    def stop(self):
        self.shell("am", "force-stop", self.package)

    def launch(self, spec):
        config = json.dumps({"test_actions": [f"studio-render:{spec}"]}, separators=(",", ":"))
        result = self.shell("am", "start", "-n", f"{self.package}/.MakepadApp",
                            "--es", "makepad.APP_CONFIG", config)
        if b"Error:" in result.stdout or b"Error:" in result.stderr:
            raise ProbeError(result.stdout.decode(errors="replace"))
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            result = self.shell("pidof", self.package, check=False)
            pids = result.stdout.decode().strip().split()
            if len(pids) == 1 and pids[0].isdigit():
                return pids[0]
            time.sleep(0.05)
        raise ProbeError("test activity did not start")


class AppLog:
    """Stream this process's logs; never clear the device-wide log buffer."""
    def __init__(self, device, pid, background):
        self.lines = []
        self.background_sent = False
        self.background_error = None
        self.denied = threading.Event()
        self.process = subprocess.Popen(device.prefix + ["logcat", "--pid=" + pid, "-v", "raw"],
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        def collect():
            for line in self.process.stdout:
                # Store only probe diagnostics, not unrelated app/provider logs.
                if "studio-test-" in line or "studio-render-geometry:" in line or "studio test:" in line:
                    self.lines.append(line.rstrip())
                if "studio test action requires developer mode" in line:
                    self.denied.set()
                if background and not self.background_sent and "studio-test-started:" in line:
                    self.background_sent = True
                    try:
                        device.shell("input", "keyevent", "KEYCODE_HOME")
                    except Exception as error:
                        self.background_error = str(error)
        self.thread = threading.Thread(target=collect, daemon=True)
        self.thread.start()

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.thread.join(timeout=16)
        self.process.stdout.close()


def run_case(device, root, name, destination, timeout, metrics):
    directory = f"{root}/{name}"
    device.stop()
    device.private(f"mkdir -m 700 -p {shlex.quote(directory)}")
    device.write(directory + "/card.l0", FIXTURE.read_bytes())
    data = {"day": {"title": "Studio device probe", "as_of": "FIXTURE",
                    "pick1_title": "1. Top row", "pick1_body": "09:00",
                    "pick2_title": "2. Middle row", "pick2_body": "12:00",
                    "pick3_title": "3. Bottom row", "pick3_body": "17:00",
                    "summary": "End of fixture — no account data"}}
    device.write(directory + "/data.json", json.dumps(data).encode())
    spec = {"workspace": directory, "source_path": "card.l0", "data_path": "data.json", "dark": name == "dark"}
    device.write(directory + "/spec.json", json.dumps(spec).encode())
    pid = device.launch(directory + "/spec.json")
    logs = AppLog(device, pid, name == "background")
    record = {"case": name, "status": "failed", "pid": pid, "workspace": directory}
    try:
        deadline = time.monotonic() + timeout
        result = None
        while time.monotonic() < deadline:
            raw = device.read(directory + "/studio-result.json")
            if raw:
                try:
                    result = json.loads(raw)
                    break
                except json.JSONDecodeError:
                    pass  # Writer may be between write and fsync.
            if name == "denied" and logs.denied.is_set():
                break
            time.sleep(0.05)
        if name == "denied":
            if not logs.denied.is_set() or device.read(directory + "/studio-result.json") is not None:
                raise ProbeError("expected developer-mode denial without a result file")
            record.update(status="passed", denial="developer mode required")
        else:
            if not isinstance(result, dict):
                raise ProbeError("no complete studio-result.json before timeout")
            record["reply"] = result
            if name == "background":
                record["home_sent_after_submit"] = logs.background_sent
                if logs.background_error:
                    raise ProbeError(logs.background_error)
                if result.get("ok") is True:
                    record["status"] = "inconclusive"
                    record["reason"] = "render completed before background cancellation; rerun required"
                else:
                    error = result.get("error", {})
                    if not logs.background_sent or error.get("kind") != "studio_render_failed" or not any(
                        word in error.get("message", "") for word in ("not_foreground", "studio_cancelled", "cancelled")
                    ):
                        raise ProbeError("expected cancellation after renderer submission")
                    record["status"] = "passed"
            else:
                if result.get("ok") is not True:
                    raise ProbeError(f"renderer failed: {result.get('error')}")
                rendered = result.get("data", {})
                if rendered.get("settled") is not True:
                    raise ProbeError("renderer did not settle")
                png = device.read(directory + "/" + output_name(rendered.get("path")), PNG_LIMIT)
                if png is None:
                    raise ProbeError("renderer PNG is missing")
                metadata = png_metadata(png)
                if (metadata["width"], metadata["height"]) != (rendered.get("width"), rendered.get("height")):
                    raise ProbeError("reply dimensions differ from PNG header")
                (destination / f"{name}.png").write_bytes(png)
                record.update(png=metadata, expected_geometry=metrics)
                check_png_geometry(metadata, metrics)
                record["status"] = "passed"
        if name in ("denied", "background") and record["status"] == "passed":
            files = device.private(f"find {shlex.quote(directory)} -name '*.png' -print").stdout
            if files.strip():
                raise ProbeError("denied/cancelled render left a PNG behind")
    except Exception as error:
        record.update(status="failed", reason=str(error))
    finally:
        logs.close()
        try:
            record["profile"] = runtime_profile(logs.lines, device.package)
        except ProbeError as error:
            record.update(status="failed", reason=str(error))
        (destination / f"{name}.log").write_text("\n".join(logs.lines) + "\n")
        (destination / f"{name}.json").write_text(json.dumps(record, indent=2) + "\n")
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True, help="explicit adb serial of the authorized unlocked test device")
    parser.add_argument("--package", type=test_package, default=DEFAULT_PACKAGE)
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--output", type=Path, required=True, help="new local evidence directory")
    parser.add_argument("--timeout", type=float, default=35, help="seconds per case (minimum 30)")
    parser.add_argument("--wake", action="store_true", help="wake screen and dismiss a noncredential keyguard; never unlocks credentials")
    parser.add_argument("--dry-run", action="store_true", help="print plan without contacting adb")
    args = parser.parse_args()
    if args.timeout < 30:
        parser.error("--timeout must be at least 30 seconds")
    plan = {"serial": args.serial, "package": args.package, "cases": ["denied", "light", "dark", "background"],
            "fixture": str(FIXTURE.relative_to(ROOT)), "output": str(args.output),
            "installs_or_flashes": False, "model_image_delivery": "not tested"}
    if args.dry_run:
        print(json.dumps(plan, indent=2))
        return 0
    args.output.mkdir(parents=True, exist_ok=False)
    evidence = {**plan, "started_unix": int(time.time()), "cases": [], "status": "failed"}
    device = Device(args.adb, args.serial, args.package)
    saved = {}
    modified = False
    try:
        if device.call("get-state").stdout.strip() != b"device":
            raise ProbeError("selected device is unavailable")
        # Verify run-as before changing anything. Optimized APKs need the explicit
        # MAKEPAD_FORCE_DEBUGGABLE packaging opt-in, independent of dev-mode.
        app_root = device.private("pwd").stdout.decode().strip()
        if not PurePosixPath(app_root).is_absolute():
            raise ProbeError("run-as did not return an absolute app directory")
        device.stop()
        device.private("umask 077; mkdir -p files/.octosense files/studio-fixture")
        home = device.private("cd files/.octosense && pwd -P").stdout.decode().strip()
        for name in ("developer-profile", "dev-mode.json"):
            path = f"{home}/{name}"
            saved[path] = device.read(path, 64 * 1024)
        modified = True
        device.private("rm -f " + shlex.join(list(saved)))
        evidence["device"] = {key: device.shell("getprop", prop).stdout.decode().strip() for key, prop in (
            ("model", "ro.product.model"), ("fingerprint", "ro.build.fingerprint"))}
        permissions = device.shell("dumpsys", "package", args.package).stdout.decode(errors="replace")
        evidence["startup_location_permissions"] = [line.strip() for line in permissions.splitlines()
            if re.search(r"android\.permission\.ACCESS_(?:FINE|COARSE)_LOCATION: granted=", line)]
        if args.wake:
            device.shell("input", "keyevent", "KEYCODE_WAKEUP")
            device.shell("wm", "dismiss-keyguard", check=False)
        metrics = display_metrics(device.shell("wm", "size").stdout.decode(),
                                  device.shell("wm", "density").stdout.decode())
        evidence["display"] = metrics
        evidence["visual_inspection"] = "required: verify readable fixture text, top-to-bottom order, and light/dark appearance"
        root = app_root + "/files/studio-fixture/" + uuid.uuid4().hex
        evidence["fixture_workspace"] = root
        evidence["fixture_sha256"] = hashlib.sha256(FIXTURE.read_bytes()).hexdigest()
        denied = run_case(device, root, "denied", args.output, args.timeout, metrics)
        evidence["cases"].append(denied)
        if denied["status"] != "passed":
            raise ProbeError("pre-grant denial/profile check failed; developer mode was not provisioned")
        profile_id = denied["profile"]["canonical"]
        evidence["runtime_profile"] = denied["profile"]
        device.stop()
        device.write(home + "/developer-profile", b"studio-device-probe\n")
        device.write(home + "/dev-mode.json", json.dumps({"scope": "all", "origin": "settings",
                     "since": int(time.time()), "profile_id": profile_id}).encode())
        for name in ("light", "dark", "background"):
            evidence["cases"].append(run_case(device, root, name, args.output, args.timeout, metrics))
        cases = evidence["cases"]
        if all(case["status"] == "passed" for case in cases):
            if cases[1]["png"]["sha256"] == cases[2]["png"]["sha256"]:
                raise ProbeError("light and dark renders are identical")
            evidence["status"] = "passed"
    except Exception as error:
        evidence["error"] = str(error)
    finally:
        if modified:
            try:
                device.stop()
                for path, data in saved.items():
                    if data is None:
                        device.private("rm -f " + shlex.quote(path))
                    else:
                        device.write(path, data)
                evidence["developer_state_restored"] = True
            except Exception as error:
                evidence.update(status="failed", restore_error=str(error))
        try:
            device.shell("input", "keyevent", "KEYCODE_HOME")
        except Exception as error:
            evidence.update(status="failed", home_error=str(error))
        evidence["finished_unix"] = int(time.time())
        (args.output / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps({"status": evidence["status"], "evidence": str(args.output / "evidence.json")}, indent=2))
    return 0 if evidence["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())

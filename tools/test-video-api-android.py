#!/usr/bin/env python3
"""Exercise contained Video script callbacks in an isolated Android test APK.

Requires an explicitly assigned ADB serial and a fresh, fixed test package.
Android's Makepad HTTP remote is unavailable, so this uses the fixture's bounded
app-private command-file lane via run-as. It never injects global input, captures
the phone screen, grants device permissions, or reads another app's data.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = "dev.makepad.octosense.videoapilab.publicapis1"
PRIVATE_ROOT = "files/video-api-lab"
ACTION = {"Prepare": "prepare", "Play": "play", "Pause": "pause", "Resume": "resume",
          "Seek 2s": "seek", "Seek start": "seek-start", "Stop": "stop", "Probe": "probe",
          "Check args": "args", "Close app": "close"}


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("adb", "aapt2", "apk", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--serial", required=True, help="Assigned phone; never written to receipts")
    args = parser.parse_args()
    for name in ("adb", "aapt2", "apk"):
        if not getattr(args, name).is_file():
            parser.error("Provide existing adb, aapt2 and APK paths")
    if not args.serial.strip() or args.serial.startswith("-"):
        parser.error("Provide the assigned device serial explicitly")
    root = args.out.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    base = [str(args.adb.resolve()), "-s", args.serial]
    fixture = ROOT / "crates/video-api-smoke/resources/playback.mp4"
    receipt = {"schema": 1, "platform": "android", "package": PACKAGE, "passed": False,
               "checks": {}, "synthetic_media": True, "personal_data": False,
               "apk_sha256": hashlib.sha256(args.apk.read_bytes()).hexdigest(),
               "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
               "transport": "app_private_command_file_to_contained_button_callback",
               "physical_input": False, "network_listener_started": False,
               "not_verified": ["Android drawable capture", "physical touch/pointer input",
                                "network streaming", "device capture", "audible sound",
                                "multiple concurrent app heaps", "App Hub signed admission"],
               "command_count": 0}
    installed = False
    fresh_absence_validated = False
    install_attempted = False
    command_sequence = 0

    def adb(*values, data=None, checked=True, timeout=30):
        result = subprocess.run(base + list(values), input=data, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=timeout)
        if checked and result.returncode:
            # A subprocess exception would disclose the serial/build path when
            # rendered. Device output remains private and is never echoed.
            raise AssertionError("Assigned-device command failed; inspect the private receipt")
        return result

    def require(name, condition):
        receipt["checks"][name] = bool(condition)
        if not condition:
            raise AssertionError(name)

    def state(allow_missing=False):
        # exec-out reports exit 0 and folds remote stderr into stdout even when
        # cat fails during startup. Shell v2 without a PTY preserves both the
        # remote exit status and JSON bytes, so a missing state can be retried.
        result = adb("shell", "-T", "run-as", PACKAGE, "cat", PRIVATE_ROOT + "/state.json", checked=False)
        if result.returncode and allow_missing:
            return None
        if result.returncode:
            raise AssertionError("Owned fixture state is unavailable")
        value = json.loads(result.stdout)
        (root / "native-state.json").write_text(json.dumps(value, indent=2) + "\n")
        if value["native"]["error"] or (value.get("script") or {}).get("error"):
            raise AssertionError("Native or script video reported an error; inspect private state")
        if (value.get("bridge") or {}).get("error"):
            raise AssertionError("Fixture command bridge refused a command; inspect private state")
        return value

    def command(label):
        nonlocal command_sequence
        previous = state()["sequence"]
        command_sequence += 1
        receipt["command_count"] = command_sequence
        payload = json.dumps({"sequence": command_sequence, "action": ACTION[label]}).encode()
        if len(payload) > 512:
            raise AssertionError("Fixture command exceeded its byte bound")
        # All remote shell words are fixed constants. The payload is stdin, never
        # shell interpolation; an atomic rename publishes one complete envelope.
        operation = ("cat > " + PRIVATE_ROOT + "/command.tmp && mv " + PRIVATE_ROOT +
                     "/command.tmp " + PRIVATE_ROOT + "/command.json")
        adb("shell", "-T", "run-as " + PACKAGE + " sh -c " + shlex.quote(operation), data=payload)
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            value = state()
            bridge = value.get("bridge") or {}
            if bridge.get("sequence") == command_sequence:
                if bridge.get("action") != ACTION[label] or bridge.get("callback_queued") is not True:
                    raise AssertionError("Fixture command acknowledgment does not match")
                if bridge.get("physical_input") is not False:
                    raise AssertionError("Fixture bridge must not claim physical input")
                if label == "Close app" and value["native"]["closed"]:
                    return value
                if label != "Close app" and value["sequence"] > previous:
                    if value["script"].get("action") != ACTION[label]:
                        raise AssertionError("Contained script report does not match its command")
                    if label != "Probe" and value["script"].get("accepted") is not True:
                        raise AssertionError("Contained script control was not accepted: " + label)
                    return value
            time.sleep(0.05)
        raise AssertionError("Contained script callback did not complete: " + label)

    def wait_for(predicate, message, timeout=25, probe=False):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            value = command("Probe") if probe else state()
            if predicate(value):
                return value
            time.sleep(0.1)
        raise AssertionError(message)

    try:
        badging = subprocess.run([str(args.aapt2.resolve()), "dump", "badging", str(args.apk.resolve())],
                                  check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  timeout=30).stdout.decode()
        package = re.search(r"^package: name='([^']+)'", badging, re.MULTILINE)
        require("isolated_debuggable_apk", package is not None and package.group(1) == PACKAGE and
                "application-debuggable" in badging)
        permissions = set(re.findall(r"^uses-permission(?:-sdk-\d+)?: name='([^']+)'", badging, re.MULTILINE))
        require("no_capture_or_account_permissions", permissions <= {"android.permission.INTERNET"})
        require("assigned_device_ready", adb("get-state").stdout.strip() == b"device")
        present = adb("shell", "pm", "path", PACKAGE, checked=False)
        require("fresh_test_package", present.returncode in (0, 1) and not present.stdout.strip()
                and not present.stderr.strip())
        fresh_absence_validated = True
        receipt["model"] = adb("shell", "getprop", "ro.product.model").stdout.decode().strip()
        receipt["android"] = adb("shell", "getprop", "ro.build.version.release").stdout.decode().strip()
        # No -r, permission grant, data clearing, or replacement of an installed app.
        install_attempted = True
        install = adb("install", str(args.apk.resolve()), timeout=90)
        installed = b"Success" in install.stdout
        require("isolated_apk_installed", installed)
        adb("shell", "am", "start", "-n", PACKAGE + "/.MakepadApp")
        deadline = time.monotonic() + 90
        while time.monotonic() < deadline:
            value = state(allow_missing=True)
            if value is not None and value.get("bridge_ready") is True:
                break
            time.sleep(0.15)
        else:
            raise AssertionError("Owned Android video fixture did not initialize")
        local_clip = adb("exec-out", "run-as", PACKAGE, "cat", PRIVATE_ROOT + "/app-storage/playback.mp4").stdout
        require("device_fixture_matches_synthetic_clip", hashlib.sha256(local_clip).hexdigest() == receipt["fixture_sha256"])
        initial = command("Probe")
        require("contained_local_source", initial["contained"] is True and initial["script"]["state"] == "unprepared")
        command("Prepare")
        prepared = wait_for(lambda v: v["script"].get("state") == "prepared" and v["native"]["prepared"] == 1,
                            "Native prepare did not complete", probe=True)
        require("script_prepare_native_decoder", 4500 <= prepared["script"]["duration_ms"] <= 5500)
        invalid = command("Check args")
        require("invalid_numeric_controls_refused", invalid["script"]["accepted"] is True and
                invalid["script"]["state"] == "prepared")
        command("Play")
        playing = wait_for(lambda v: v["script"].get("state") == "playing" and
                           v["native"]["frames"] >= 3 and v["native"]["position_ms"] >= 150,
                           "Playback did not deliver advancing native frames", probe=True)
        require("script_play_advances_native_frames", playing["script"]["position_ms"] >= 100)
        require("silent_clip_stays_muted", playing["script"]["muted"] is True)
        command("Pause")
        time.sleep(0.15)
        paused = command("Probe")
        time.sleep(0.4)
        stable = command("Probe")
        require("script_pause_stable", paused["script"]["state"] == "paused" and
                stable["script"]["state"] == "paused" and
                abs(stable["native"]["position_ms"] - paused["native"]["position_ms"]) <= 100)
        require("forward_seek_has_distinct_start", stable["native"]["position_ms"] < 1000)
        sought = command("Seek 2s")
        require("script_seek_updates_position", abs(sought["script"]["position_ms"] - 2000) <= 100)
        before = sought["native"]["frames"]
        command("Resume")
        resumed = wait_for(lambda v: v["script"].get("state") == "playing" and
                           v["native"]["frames"] > before and v["native"]["position_ms"] >= 2200,
                           "Seek/resume did not reach the native decoder", probe=True)
        require("script_seek_resume_native_frames", resumed["native"]["position_ms"] < 4900)
        positions = resumed["native"]["resume_positions_ms"]
        require("forward_seek_reaches_requested_position", len(positions) > 0 and
                any(1700 <= position <= 2800 for position in positions[:3]))
        receipt["native_forward_seek_positions_ms"] = positions
        command("Pause")
        command("Seek start")
        command("Resume")
        rewound = wait_for(lambda v: len(v["native"]["resume_positions_ms"]) >= 3,
                          "Rewind delivered no native frames", probe=True)
        positions = rewound["native"]["resume_positions_ms"]
        require("script_seek_reaches_native_decoder", any(0 <= p <= 800 for p in positions))
        receipt["native_rewind_positions_ms"] = positions
        command("Stop")
        stopped = wait_for(lambda v: v["script"].get("state") == "unprepared" and v["native"]["released"] == 1,
                           "Script stop did not release native resources", probe=True)
        require("script_stop_releases_native_player", stopped["native"]["released"] == 1)
        command("Play")
        wait_for(lambda v: v["native"]["prepared"] == 2 and v["script"].get("state") == "playing" and
                 v["native"]["position_ms"] >= 150,
                 "Player could not start after resource cleanup", probe=True)
        command("Close app")
        closed = wait_for(lambda v: v["native"]["closed"] and v["native"]["heap_closed"] and
                          v["native"]["close_released"] and v["native"]["released"] >= 2,
                          "Closing the contained app did not retire its native player")
        require("app_close_releases_native_player", closed["native"]["close_released"])
        time.sleep(0.4)
        final = state()
        require("closed_app_has_no_late_frames", final["native"]["frames_after_close_release"] == 0)
        receipt["native_final"] = final
        receipt["passed"] = all(receipt["checks"].values())
    except Exception as error:
        receipt["error"] = str(error) if isinstance(error, AssertionError) else type(error).__name__
    finally:
        if fresh_absence_validated and install_attempted:
            receipt["install_attempted"] = True
            try:
                # ADB can time out after Android committed the installation.
                # Reconcile only the fixed package known absent before this run;
                # never use an uncertain install reply to skip owned cleanup.
                presence = adb("shell", "pm", "path", PACKAGE, checked=False)
                present_now = presence.returncode == 0 and presence.stdout.startswith(b"package:")
                absent_now = (presence.returncode in (0, 1) and not presence.stdout.strip()
                              and not presence.stderr.strip())
                receipt["owned_package_presence_after_run"] = (
                    "present" if present_now else "absent" if absent_now else "unknown")
                if installed or present_now:
                    receipt["owned_package_stopped"] = adb("shell", "am", "force-stop", PACKAGE,
                                                            checked=False).returncode == 0
                    result = adb("uninstall", PACKAGE, checked=False)
                    receipt["owned_package_removed"] = result.returncode == 0 and b"Success" in result.stdout
                elif absent_now:
                    receipt["owned_cleanup_not_needed"] = True
                else:
                    receipt["owned_cleanup_unverified"] = True
            except (subprocess.SubprocessError, OSError):
                receipt["owned_cleanup_unverified"] = True
            receipt["passed"] = (receipt["passed"] and receipt.get("owned_package_stopped", False)
                                 and receipt.get("owned_package_removed", False))
        (root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"passed": receipt["passed"], "checks": len(receipt["checks"]),
                      "receipt": str(root / "receipt.json")}))
    return 0 if receipt["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

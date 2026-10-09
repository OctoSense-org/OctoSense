#!/usr/bin/env python3
"""Exercise contained Splash Video controls with a silent local synthetic clip.

Drives only a hidden, owned Makepad instrument instance. No capture device,
provider, account, network media, personal library or installed app is accessed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def main():
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    host = args.host.resolve()
    if not host.is_file():
        parser.error("Build the native video acceptance host first")
    root = args.out.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=False)
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    env = os.environ.copy()
    env.pop("MAKEPAD_FORCE_FOCUS", None)
    env.update(MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=str(port))
    for name in ("OCTOSENSE_HOME", "OCTOS_APP_CORE_DIR", "OCTOSENSE_APP_DATA",
                 "RINX_DATA_DIR", "ROBRIX_DATA_DIR", "XDG_CONFIG_HOME",
                 "XDG_DATA_HOME", "XDG_CACHE_HOME"):
        directory = root / "isolated" / name.lower()
        directory.mkdir(mode=0o700, parents=True)
        env[name] = str(directory)
    fixture = ROOT / "crates/video-api-smoke/resources/playback.mp4"
    receipt = {"schema": 1, "passed": False, "checks": {}, "synthetic_media": True,
               "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(),
               "host_sha256": hashlib.sha256(host.read_bytes()).hexdigest(),
               "not_verified": ["network streaming", "device capture", "audible sound",
                                "other platform decoders", "App Hub signed admission"]}
    native = root / "native" / "state.json"

    def get(route):
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/{route}", timeout=4) as response:
            return json.load(response)

    def state():
        value = json.loads(native.read_text())
        if value["native"]["error"] or (value.get("script") or {}).get("error"):
            raise AssertionError("Native or script video reported an error; inspect private state")
        return value

    def require(name, condition):
        receipt["checks"][name] = bool(condition)
        if not condition:
            raise AssertionError(name)

    def wait_for(predicate, message, timeout=20, probe=False):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise AssertionError("Owned video host exited; inspect private log")
            if probe:
                click("Probe")
            value = state()
            if predicate(value):
                return value
            time.sleep(0.1)
        raise AssertionError(message)

    def click(label):
        snapshot = get("snap")
        matches = [w for w in snapshot.get("s", [])
                   if w.get("ty") == "Button" and w.get("t") == label]
        if len(matches) != 1:
            raise AssertionError("Expected exactly one contained script button: " + label)
        x, y, width, height = matches[0]["r"]
        previous = state()["sequence"]
        get(f"click?x={x + width / 2}&y={y + height / 2}&wait=1")
        if label == "Close app":
            return
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            value = state()
            if value["sequence"] > previous:
                expected = {"Prepare": "prepare", "Play": "play", "Pause": "pause",
                            "Resume": "resume", "Seek 2s": "seek", "Seek start": "seek-start",
                            "Check args": "args",
                            "Stop": "stop", "Probe": "probe"}[label]
                if value["script"].get("action") != expected:
                    raise AssertionError("Unexpected script report after " + label)
                if label != "Probe" and value["script"].get("accepted") is not True:
                    raise AssertionError("Script control was not accepted: " + label)
                return value
            time.sleep(0.03)
        raise AssertionError("Script button produced no host report: " + label)

    def capture(name):
        image = get("g?scale=1")
        destination = root / (name + ".png")
        shutil.copyfile(image["png"], destination)
        require(name + "_native_capture", destination.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"))

    with (root / "native.log").open("w") as log:
        process = subprocess.Popen([str(host), "--out=" + str(root / "native")],
                                   cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise AssertionError("Video host exited during startup")
                try:
                    snapshot = get("snap")
                    if native.exists() and any(w.get("t") == "Video API Lab" for w in snapshot.get("s", [])):
                        break
                except (OSError, ValueError):
                    pass
                time.sleep(0.1)
            else:
                raise AssertionError("Hidden native video host did not initialize")
            initial = click("Probe")
            require("contained_local_source", initial["contained"] and initial["script"]["state"] == "unprepared")
            click("Prepare")
            prepared = wait_for(lambda v: v["script"].get("state") == "prepared" and v["native"]["prepared"] == 1,
                                "Native prepare did not complete", probe=True)
            require("script_prepare_native_decoder", 4500 <= prepared["script"]["duration_ms"] <= 5500)
            invalid = click("Check args")
            require("invalid_numeric_controls_refused", invalid["script"]["accepted"] is True and
                    invalid["script"]["state"] == "prepared")
            click("Play")
            playing = wait_for(lambda v: v["script"].get("state") == "playing" and
                               v["native"]["frames"] >= 3 and v["native"]["position_ms"] >= 150,
                               "Playback did not deliver advancing native frames", probe=True)
            require("script_play_advances_native_frames", playing["script"]["position_ms"] >= 100)
            require("silent_clip_stays_muted", playing["script"]["muted"] is True)
            click("Pause")
            time.sleep(0.15)
            paused = click("Probe")
            time.sleep(0.4)
            stable = click("Probe")
            require("script_pause_stable", paused["script"]["state"] == "paused" and
                    stable["script"]["state"] == "paused" and
                    abs(stable["native"]["position_ms"] - paused["native"]["position_ms"]) <= 100)
            capture("paused")
            require("forward_seek_has_distinct_start", stable["native"]["position_ms"] < 1000)
            sought = click("Seek 2s")
            require("script_seek_updates_position", abs(sought["script"]["position_ms"] - 2000) <= 100)
            before = sought["native"]["frames"]
            click("Resume")
            resumed = wait_for(lambda v: v["script"].get("state") == "playing" and
                               v["native"]["frames"] > before and v["native"]["position_ms"] >= 2200,
                               "Seek/resume did not reach the native decoder", probe=True)
            require("script_seek_resume_native_frames", resumed["native"]["position_ms"] < 4900)
            positions = resumed["native"]["resume_positions_ms"]
            require("forward_seek_reaches_requested_position", len(positions) > 0 and
                    any(1700 <= position <= 2800 for position in positions[:3]))
            receipt["native_forward_seek_positions_ms"] = positions
            # A forward seek's optimistic script value or eventually reaching 2s
            # is insufficient. Seek backwards from native >2s and require early
            # native frames near zero; a non-looping player cannot do that alone.
            click("Pause")
            click("Seek start")
            click("Resume")
            rewound = wait_for(lambda v: len(v["native"]["resume_positions_ms"]) >= 3,
                              "Rewind delivered no native frames", probe=True)
            positions = rewound["native"]["resume_positions_ms"]
            require("script_seek_reaches_native_decoder", any(0 <= p <= 800 for p in positions))
            receipt["native_rewind_positions_ms"] = positions
            capture("resumed")
            click("Stop")
            stopped = wait_for(lambda v: v["script"].get("state") == "unprepared" and v["native"]["released"] == 1,
                               "Script stop did not release native resources", probe=True)
            require("script_stop_releases_native_player", stopped["native"]["released"] == 1)
            click("Play")
            wait_for(lambda v: v["native"]["prepared"] == 2 and v["script"].get("state") == "playing" and
                     v["native"]["position_ms"] >= 150,
                     "Player could not start after resource cleanup", probe=True)
            click("Close app")
            closed = wait_for(lambda v: v["native"]["closed"] and v["native"]["heap_closed"] and
                              v["native"]["close_released"] and v["native"]["released"] >= 2,
                              "Closing the app did not retire its native player")
            require("app_close_releases_native_player", closed["native"]["close_released"])
            time.sleep(0.4)
            final = state()
            require("closed_app_has_no_late_frames", final["native"]["frames_after_close_release"] == 0)
            receipt["native_final"] = final
            receipt["passed"] = all(receipt["checks"].values())
        except Exception as error:
            receipt["error"] = str(error) if isinstance(error, AssertionError) else type(error).__name__
            raise
        finally:
            try:
                get("quit")
            except (OSError, ValueError):
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)
            receipt["owned_host_exit_code"] = process.returncode
            (root / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({"passed": receipt["passed"], "checks": len(receipt["checks"]),
                      "receipt": str(root / "receipt.json")}))


if __name__ == "__main__":
    main()

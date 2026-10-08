#!/usr/bin/env python3
"""Exercise a signed app -> app tool -> native host API in a hidden macOS window.

Uses real OS permission status, but never approves access or captures media.
The explicit native test caller enters the tool queue after signed installation;
this is not a model/peer-consent test. All profiles and ephemeral signatures are
isolated. A failure always leaves a result.json receipt and retained evidence.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

REPO = Path(__file__).resolve().parents[1]


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", type=Path, default=REPO / "target/release/examples/host-api-lab")
    parser.add_argument("--hub", type=Path, required=True, help="Contract 1.6 compatible hub binary")
    parser.add_argument("--output", type=Path, help="New evidence directory; defaults to a private temporary directory")
    args = parser.parse_args()
    root = args.output.resolve() if args.output else Path(tempfile.mkdtemp(prefix="octosense-host-api-native-"))
    if args.output:
        root.mkdir(mode=0o700, parents=True, exist_ok=False)
    result = {"schema": 1, "result": "failed", "verified": [], "not_verified": [
        "model reasoning and peer consent", "physical permission approval", "camera capture",
        "Android runtime", "Linux and Windows device services", "public host release"]}
    try:
        require(sys.platform == "darwin", "This native OS status acceptance currently requires macOS")
        host, hub = args.host.resolve(), args.hub.resolve()
        require(host.is_file() and hub.is_file(), "Build the host-api-lab example and hub first")
        bundle = root / "bundle"
        shutil.copytree(REPO / "tools/fixtures/host-api-lab/bundle", bundle)
        (bundle / "screenshots").mkdir(exist_ok=True)

        def stamp():
            subprocess.run([str(hub), "stamp", str(bundle)], check=True, capture_output=True, text=True)

        def run(phase):
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                port = sock.getsockname()[1]
            env = os.environ.copy()
            for name in ["MAKEPAD_FORCE_FOCUS", "OCTOSENSE_HUB_ANCHOR"]:
                env.pop(name, None)
            env.update(MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=str(port))
            command = [str(host), f"--bundle={bundle}", f"--app-data={root / phase}"]
            receipt = root / "native-result.json"
            command += ["--preview"] if phase == "preview" else [f"--receipt={receipt}"]
            log_path = root / f"{phase}.log"
            with log_path.open("w") as log:
                process = subprocess.Popen(command, cwd=REPO, env=env, stdout=log, stderr=subprocess.STDOUT)

                def get(route):
                    with urllib.request.urlopen(f"http://127.0.0.1:{port}/{route}", timeout=4) as response:
                        return json.load(response)

                try:
                    deadline = time.monotonic() + 50
                    while time.monotonic() < deadline:
                        require(process.poll() is None, f"{phase} host exited; inspect {phase}.log")
                        try:
                            snap = get("snap")
                            if any(w.get("ty") == "Label" and w.get("t") == "Host API Lab" for w in snap.get("s", [])) and (phase == "preview" or receipt.exists()):
                                break
                        except (OSError, ValueError):
                            pass
                        time.sleep(0.1)
                    else:
                        raise AssertionError(f"{phase} native UI/result timed out")
                    time.sleep(0.3)
                    snapshot = get("snap")
                    (root / f"{phase}-snapshot.json").write_text(json.dumps(snapshot, indent=2))
                    capture = get("g?scale=1")
                    shutil.copyfile(capture["png"], root / f"{phase}.png")
                    if phase == "preview":
                        shutil.copyfile(root / "preview.png", bundle / "screenshots/01-main.png")
                    else:
                        native = json.loads(receipt.read_text())
                        require(native["signed_install"], "Native query did not use signed installation")
                        require("Ok" in native["tool_result"], "Own app tool failed")
                        data = native["tool_result"]["Ok"]
                        require(data["account"] == "device", "Wrong tool account")
                        require(data["camera"]["capability"] == "camera" and data["camera"]["supported"], "No native camera permission status")
                        require(data["camera"]["app_policy_granted"] and not data["camera"]["app_consent"], "Capability confused with consent")
                        require(data["camera"]["os_permission"] in ["granted", "not_determined", "denied", "settings_required"], "Not a native OS status")
                        require(data["discovery"]["supported"] and data["discovery"]["descriptor"]["name"] == "camera.permission.status", "API discovery mismatch")
                        require(not data["missing"]["implemented"] and not data["missing"]["supported"], "Uncompiled Rust function advertised")
                        require(not data["microphone_allowed"] and data["microphone_error"], "Undeclared capability was allowed")
                        require(not data["background_prompt_allowed"] and "background" in data["background_error"], "Tool callback gained prompt authority")
                        require(not native["host_sheet_visible"], "Background tool opened a native sheet")
                        require("account_scope" in native["refusals"]["wrong_account"], "Cross-account call accepted")
                        require("tool_not_declared" in native["refusals"]["undeclared_tool"], "Undeclared tool accepted")
                        require("invalid_arguments" in native["refusals"]["invalid_arguments"], "Invalid tool input accepted")
                        require("app_not_running" in native["closed_app"], "Closed app retained its tool endpoint")
                        require(any(w.get("ty") == "Label" and w.get("t") == "Completed native queries: 1" for w in snapshot.get("s", [])), "Tool did not update live app state")
                        button = next(w for w in snapshot["s"] if w.get("ty") == "Button" and w.get("t") == "Read permission status")
                        x, y, width, height = button["r"]
                        get(f"click?x={x + width / 2}&y={y + height / 2}&wait=1")
                        deadline = time.monotonic() + 5
                        while time.monotonic() < deadline:
                            updated = get("snap")
                            if any(w.get("ty") == "Label" and w.get("t") == "Completed native queries: 2" for w in updated.get("s", [])):
                                break
                            time.sleep(0.1)
                        else:
                            raise AssertionError("Native UI button did not query the host service")
                        (root / "ui-snapshot.json").write_text(json.dumps(updated, indent=2))
                        shutil.copyfile(get("g?scale=1")["png"], root / "ui.png")
                        result["bundle_digest"] = native["bundle_digest"]
                        result["verified"] = ["signed admission and launch", "own Splash tool completion", "native OS permission status", "live app UI update", "native UI button host call", "API discovery and missing-function fallback", "undeclared capability refusal", "background callback prompt refusal", "cross-account and schema refusal", "closed-app refusal"]
                finally:
                    if process.poll() is None:
                        try:
                            get("quit")
                        except (OSError, ValueError):
                            pass
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            process.terminate()
                            try:
                                process.wait(timeout=5)
                            except subprocess.TimeoutExpired:
                                process.kill()
                                process.wait(timeout=5)
                            raise AssertionError(f"{phase} host required forced termination")
                    require(process.returncode == 0, f"{phase} host exited with status {process.returncode}")
            text = log_path.read_text(errors="replace")
            require("on_render closure failed" not in text and "callback error" not in text, f"{phase} script callback failed")

        stamp()
        run("preview")
        stamp()
        run("signed")
        result["result"] = "pass"
    except Exception as error:
        result["error"] = str(error)
    finally:
        (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps({"result": result["result"], "evidence": str(root), "error": result.get("error")}))
    return 0 if result["result"] == "pass" else 1


if __name__ == "__main__":
    sys.exit(main())

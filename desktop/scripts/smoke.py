#!/usr/bin/env python3
"""Native app-local smoke test. Build with cargo build --release --workspace first.

Uses the host for every input event; the child's snapshot only observes results.
All windows/processes opened by this script are closed in finally. Artifacts are
kept in a printed temporary directory. --cargo-run exercises the exact cargo run
entry point with remote control and isolated state supplied by the environment.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlencode
from urllib.error import HTTPError
from urllib.request import urlopen


def get(port, route, **params):
    query = "?" + urlencode(params) if params else ""
    for attempt in range(3):
        try:
            with urlopen(f"http://127.0.0.1:{port}/{route}{query}", timeout=8) as response:
                result = json.load(response)
            break
        except HTTPError as error:
            body = error.read().decode(errors="replace")
            # Native remote.rs applies input BEFORE trying to present its
            # frame. A presentation refusal must never replay a click/key.
            # A separate read-only grab provides the missing frame barrier.
            if (error.code == 404 and route in ("click", "k") and
                    "requested input frame could not be submitted; retry" in body):
                capture = {"w": params["w"]} if "w" in params else {}
                get(port, "g", **capture)
                return {"ok": 1}
            if (attempt < 2 and error.code == 404 and route == "g" and
                    "grab frame could not be submitted at arming; retry" in body):
                time.sleep(0.2)
                continue
            raise ValueError(f"/{route}: HTTP {error.code}: {body}") from error
    if "err" in result:
        raise ValueError(f"/{route}: {result['err']}")
    return result


def wait_for(description, fn, timeout=30):
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        try:
            result = fn()
            if result:
                return result
        except (OSError, ValueError, KeyError, IndexError) as error:
            last = error
        time.sleep(0.1)
    raise AssertionError(f"Timed out: {description}; last error: {last}")


def remote(log):
    match = re.search(r"listening on 127\.0\.0\.1:(\d+) pid=(\d+)", log.read_text(errors="replace"))
    return (int(match[1]), int(match[2])) if match else None


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def save_grab(artifacts, name, grab):
    """Keep frames with the report rather than relying on remote temp files."""
    paths = grab["png"] if isinstance(grab["png"], list) else [grab["png"]]
    saved = []
    for index, source in enumerate(paths):
        destination = artifacts / f"{name}-{index}.png"
        shutil.copyfile(source, destination)
        saved.append(str(destination))
    metadata = dict(grab, png=saved if isinstance(grab["png"], list) else saved[0])
    (artifacts / f"{name}.json").write_text(json.dumps(metadata))


def assert_no_runtime_errors(text):
    failures = [line for line in text.splitlines() if any(marker in line for marker in (
        "[E]", "panicked at", "failed to compile and will NOT be drawn"))]
    assert not failures, "Runtime rendering errors:\n" + "\n".join(failures)


def client_logs(state, host_pid):
    return set((state / "logs/clients").glob(f"octosense-{host_pid}-client-*.log"))


def hosted_widgets(port, log, client):
    """A hosted app's widgets, asked through the shell: Reference is a `network: none`
    app, whose sandbox refuses it the listening socket of an instrument of its own."""
    def answer():
        start = log.stat().st_size
        get(port, "event", data="client-widgets:reference")
        deadline = time.monotonic() + 1.5
        while time.monotonic() < deadline:
            with log.open("rb") as file:
                file.seek(start)
                text = file.read().decode(errors="replace")
            # A line still being written has no newline yet.
            for line in reversed(text.split("\n")[:-1]):
                match = re.search(rf"client-widgets {client} (\[.*\])\s*$", line)
                if match:
                    return json.loads(match[1])
            time.sleep(0.02)
        return None
    return wait_for(f"client {client} answers with its widgets", answer)


def hosted_widget(port, log, client, name):
    return next((item for item in hosted_widgets(port, log, client) if item.get("id") == name), None)


def shows_count(port, log, client, count):
    return (hosted_widget(port, log, client, "count") or {}).get("text") == f"Count: {count}"


def largest_tile(port):
    return max(get(port, "snap", q="MpRunView")["s"], key=lambda item: item["r"][2] * item["r"][3])


def settled_window_size(port):
    # Layout and child resize messages arrive asynchronously. A visible tile
    # during a workspace animation can still have an intermediate size.
    last = None
    since = time.monotonic()
    def settled():
        nonlocal last, since
        size = largest_tile(port)["r"][2:]
        now = time.monotonic()
        if size != last:
            last, since = size, now
        return size if now - since >= 0.5 else None
    return wait_for("child window size settles", settled)


def check_styles(port, child, state, host_pid, log, artifacts):
    """Exercise live capture/style changes through the host with a retained app."""
    count = 1
    clients = client_logs(state, host_pid)
    # Each style reaches its styles its own way (a Mac's bar menu, a phone's top bar, a
    # launcher that also finds apps), so this runs the row they all run, by its test route.
    styles = ["octosense-dark", "octosense", "macos", "windows", "windows-2000", "nextstep",
              "ios", "android", "omarchy", "octosense-dark", "omarchy"]
    for index, style in enumerate(styles):
        offset = len(log.read_text(errors="replace"))
        get(port, "event", data="mac-row:desktop." + style)
        wait_for(style + " applied", lambda: f"wm: desktop style {style} applied" in log.read_text(errors="replace")[offset:])
        time.sleep(0.9)  # Let the framebuffer transition and child restyle settle.
        if style in ("omarchy", "octosense", "octosense-dark"):
            wait_for(style + " wallpaper visible", lambda: get(port, "snap", q="bg_image").get("s"))
        assert shows_count(port, log, child, count), style
        assert client_logs(state, host_pid) == clients, "style launched an extra client"
        save_grab(artifacts, f"style-{index}-{style}", get(port, "g", scale=0.5))
        if style in ("octosense", "octosense-dark"):
            tile = largest_tile(port)
            button = hosted_widget(port, log, child, "increment")
            get(port, "click", x=tile["r"][0] + button["x"] + button["width"]/2,
                y=tile["r"][1] + button["y"] + button["height"]/2, wait=1)
            count += 1
            wait_for("input inside glass window", lambda: shows_count(port, log, child, count))
            get(port, "k", c="Space", cmd=1, wait=1)
            save_grab(artifacts, f"style-{index}-{style}-menu", get(port, "g", scale=0.5))
            get(port, "k", c="Escape", wait=1)
            if index < 2:
                width = get(port, "s")["w"][0]["sz"][0]
                get(port, "click", x=width/2, y=13, wait=1)
                save_grab(artifacts, f"{style}-calendar", get(port, "g", scale=0.5))
                # Flyouts close on outside clicks; Escape only closes menus.
                get(port, "click", x=width-10, y=100, wait=1)
                # The lean catalog has no assistant: this requests a local
                # notification, exercising its glass without launching an app.
                get(port, "k", c="F10", wait=1)
                save_grab(artifacts, f"{style}-notification", get(port, "g", scale=0.5))
        print(f"PASS: {style} renders and preserves the hosted app without extra launches", flush=True)
        assert_no_runtime_errors(log.read_text(errors="replace"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-run", action="store_true")
    parser.add_argument("--default-catalog", action="store_true", help="Use the shipped catalog; skip injected failure/build fixtures")
    parser.add_argument("--styles", action="store_true", help="Exercise all desktop styles, including OctoSense glass, with a retained app")
    parser.add_argument("--artifacts-dir", type=Path, help="New directory for retained logs, state, and frames")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if args.artifacts_dir:
        artifacts = args.artifacts_dir.resolve()
        artifacts.mkdir(parents=True, exist_ok=False)
    else:
        artifacts = Path(tempfile.mkdtemp(prefix="octosense-smoke-"))
    state = artifacts / "state"
    state.mkdir()
    manifest = str(root.parent / "apps/reference/Cargo.toml")  # the workspace's apps/
    slow = artifacts / "slow"
    (slow / "src").mkdir(parents=True)
    (slow / "Cargo.toml").write_text('[package]\nname="octosense-smoke-slow"\nversion="0.1.0"\nedition="2021"\n[workspace]\n')
    (slow / "src/main.rs").write_text("fn main() {}\n")
    marker = artifacts / "build.pid"
    # The shell builds an app with a plain environment: the script's source says where.
    (slow / "build.rs").write_text('fn main() {\n'
        f'std::fs::write({json.dumps(str(marker))}, std::process::id().to_string()).unwrap();\n'
        'std::thread::sleep(std::time::Duration::from_secs(45));\n}\n')
    catalog = [
        {"id": "reference", "label": "Reference", "manifest": manifest,
         "package": "octosense-reference", "bin": "octosense-reference", "policy": "new"},
        {"id": "broken", "label": "Broken startup test", "manifest": manifest,
         "package": "octosense-package-does-not-exist", "bin": "missing"},
        {"id": "slow", "label": "Slow build", "manifest": str(slow / "Cargo.toml"),
         "package": "octosense-smoke-slow", "bin": "octosense-smoke-slow"},
    ]
    if not args.default_catalog:
        (state / "apps.json").write_text(json.dumps(catalog))
    env = dict(os.environ, OCTOSENSE_HOME=str(state), MAKEPAD_HIDE_WINDOWS="1", CARGO_NET_OFFLINE="true")
    for key in ["MAKEPAD_HOME", "MAKEPAD_REMOTE", "MAKEPAD_WM_ROOT", "MAKEPAD_WM_TEST_APP", "MAKEPAD_WM_THEME"]:
        env.pop(key, None)
    # The flag, not the variable: the shell's children would inherit that and log their refused port.
    command = (["cargo", "run", "--"] if args.cargo_run else [str(root.parent / "target/release/octosense")]) + ["--remote"]
    # A Mac starts in its own style, whose keys go to the focused app. These checks
    # drive the tiling keys, so they start in the style that has them.
    if sys.platform == "darwin":
        command += ["--test-action", "style:octosense"]
    log = artifacts / "host.log"
    port = None
    child_pids = []
    owned_groups = []
    print(f"Artifacts: {artifacts}", flush=True)
    with log.open("wb") as output:
        process = subprocess.Popen(command, cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        port, host_pid = wait_for("host remote startup", lambda: remote(log), timeout=120)
        status = wait_for("desktop window", lambda: get(port, "s").get("w"))
        assert "OctoSense" in status[0]["t"], status
        wait_for("desktop first frame", lambda: get(port, "snap", q="main_window").get("s"))
        assert not client_logs(state, host_pid), "unexpected startup child"
        print("PASS: desktop starts without child apps", flush=True)
        wait_for("OctoSense light selected at startup",
                 lambda: "wm: desktop style octosense applied" in log.read_text(errors="replace"))
        wait_for("bundled OctoSense wallpaper visible on clean startup",
                 lambda: get(port, "snap", q="bg_image").get("s"), timeout=10)
        def wallpaper_frame():
            grab = get(port, "g", scale=0.5)
            paths = grab["png"] if isinstance(grab["png"], list) else [grab["png"]]
            # The empty desktop gradient compresses to a few KB. Wait for
            # the detailed raster image, which decodes after Image is visible.
            return grab if sum(Path(path).stat().st_size for path in paths) > 50_000 else None
        save_grab(artifacts, "startup-wallpaper",
                  wait_for("startup wallpaper decoded and drawn", wallpaper_frame, timeout=10))
        print("PASS: OctoSense light starts with its bundled wallpaper", flush=True)

        # Open Apps through the shorter root menu: the larger submenu must
        # recenter, and scrolling must keep the last application reachable.
        get(port, "k", c="Space", cmd=1, wait=1)
        save_grab(artifacts, "launcher-root", get(port, "g", scale=0.5))
        get(port, "k", c="enter", wait=1)
        save_grab(artifacts, "launcher-apps", get(port, "g", scale=0.5))
        # Each key is acknowledged after dispatch. Capture the final layout
        # below instead of waiting for a separate presented frame per row.
        for _ in range(19):
            get(port, "k", c="ArrowDown")
        save_grab(artifacts, "launcher-apps-bottom", get(port, "g", scale=0.5))
        get(port, "k", c="Escape", wait=1)

        def launch(name):
            get(port, "k", c="Space", cmd=1, wait=1)
            # The inherited menu filters KeyDown characters, not TextInput.
            for char in name.lower():
                get(port, "k", c="Space" if char == " " else "Key" + char.upper(), wait=1)
            get(port, "k", c="enter", wait=1)

        launch("Reference")
        def new_reference():
            """A Reference client that has drawn, and its process."""
            shown = re.findall(r"wm: reference client (\d+) first frame", log.read_text(errors="replace"))
            clients = [int(client) for client in shown if int(client) not in seen_clients]
            running = subprocess.run(["pgrep", "-P", str(host_pid), "-f", "octosense-reference"],
                                     capture_output=True, text=True).stdout.split()
            pids = [int(pid) for pid in running if int(pid) not in child_pids]
            return (clients[0], pids[0]) if clients and pids else None
        seen_clients = []
        child, child_pid = wait_for("reference app starts", new_reference, timeout=120)
        seen_clients.append(child)
        child_pids.append(child_pid)
        owned_groups.append(os.getpgid(child_pid))
        wait_for("reference input", lambda: hosted_widget(port, log, child, "increment"))
        tile = wait_for("hosted tile", lambda: get(port, "snap", q="MpRunView").get("s")) and largest_tile(port)

        def click_child(name):
            item = hosted_widget(port, log, child, name)
            x, y = tile["r"][:2]
            get(port, "click", x=x + item["x"] + item["width"]/2, y=y + item["y"] + item["height"]/2, wait=1)

        click_child("increment")
        wait_for("forwarded pointer increments counter", lambda: shows_count(port, log, child, 1))
        click_child("message")
        get(port, "t", t="Hello through OctoSense", wait=1)
        wait_for("forwarded keyboard updates echo", lambda: (hosted_widget(port, log, child, "echo") or {}).get("text") == "Hello through OctoSense")
        print("PASS: pointer and keyboard input cross the host/client boundary", flush=True)
        # Let the shell's arrival animation finish before recording a frame.
        time.sleep(0.6)
        grab = get(port, "g", scale=0.5)
        save_grab(artifacts, "frame", grab)

        # Workspace movement goes through the WM's own keyboard handler.
        get(port, "k", c="Key2", cmd=1, shift=1, wait=1)
        get(port, "k", c="Key1", cmd=1, wait=1)
        # Native widget snapshots traverse hidden captured windows and can
        # panic on their retired draw areas. Record the empty workspace as
        # pixels; inspect the child again after its workspace is visible.
        save_grab(artifacts, "workspace-empty", get(port, "g", scale=0.5))
        get(port, "k", c="Key2", cmd=1, wait=1)
        wait_for("app visible on destination workspace", lambda: get(port, "snap", q="MpRunView")["s"])
        size = settled_window_size(port)
        get(port, "k", c="KeyF", cmd=1, wait=1)
        wait_for("fullscreen expands child", lambda: largest_tile(port)["r"][2] > size[0] + 10)
        settled_window_size(port)
        get(port, "k", c="KeyF", cmd=1, wait=1)
        wait_for(f"fullscreen restores child size {size}", lambda: largest_tile(port)["r"][2:] == size)
        assert alive(child_pid), "client died on workspace/fullscreen changes"
        print("PASS: workspace and fullscreen operations retain the app", flush=True)

        launch("Reference")
        second, second_pid = wait_for("second reference instance", new_reference, timeout=120)
        seen_clients.append(second)
        child_pids.append(second_pid)
        owned_groups.append(os.getpgid(second_pid))
        wait_for("second instance has independent state", lambda: shows_count(port, log, second, 0))
        assert shows_count(port, log, child, 1)
        get(port, "k", c="KeyW", cmd=1, wait=1)
        wait_for("close window reaps second instance", lambda: not alive(second_pid))
        assert alive(child_pid), "closing one instance closed both"
        print("PASS: separate instances keep independent state and close individually", flush=True)

        if args.styles:
            check_styles(port, child, state, host_pid, log, artifacts)

        slow_group = None
        if not args.default_catalog:
            launch("Broken startup test")
            wait_for("failed Cargo launch diagnostic", lambda: "octosense-package-does-not-exist" in log.read_text(errors="replace"), timeout=30)
            assert get(port, "s")["w"], "failed app launch stopped host"
            print("PASS: failed app build leaves the desktop running", flush=True)
            launch("Slow build")
            slow_pid = wait_for("Cargo build script started", lambda: int(marker.read_text()) if marker.exists() else None)
            slow_group = os.getpgid(slow_pid)
            owned_groups.append(slow_group)
        final = get(port, "gq", scale=0.5)
        save_grab(artifacts, "final-frame", final)
        process.wait(timeout=10)
        port = None
        for pid in child_pids:
            wait_for(f"child {pid} reaped on host quit", lambda: not alive(pid), timeout=5)
        if slow_group:
            wait_for("build process group removed on host quit", lambda: not alive(-slow_group), timeout=5)
            print("PASS: host quit reaps its hosted app and an unfinished Cargo build", flush=True)
        else:
            print("PASS: host quit reaps its hosted app", flush=True)
        assert_no_runtime_errors(log.read_text(errors="replace"))
        for path in client_logs(state, host_pid):
            assert_no_runtime_errors(path.read_text(errors="replace"))
    except Exception:
        if port:
            for route, params in [("snap", {"all": 1}), ("g", {"scale": 0.5})]:
                try:
                    result = get(port, route, **params)
                    if route == "g":
                        save_grab(artifacts, "failure-g", result)
                    else:
                        (artifacts / f"failure-{route}.json").write_text(json.dumps(result))
                except (OSError, ValueError):
                    pass
        raise
    finally:
        if port and process.poll() is None:
            try:
                get(port, "quit")
            except (OSError, ValueError):
                pass
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, 15)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, 9)
                process.wait(timeout=5)
        for pid in child_pids:
            if alive(pid):
                os.kill(pid, 9)
        for group in owned_groups:
            if alive(-group):
                os.killpg(group, 9)
        print(f"Logs and app-provided frames: {artifacts}", flush=True)


if __name__ == "__main__":
    main()

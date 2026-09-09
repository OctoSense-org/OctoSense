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
import tempfile
import time
from urllib.parse import urlencode
from urllib.request import urlopen


def get(port, route, **params):
    query = "?" + urlencode(params) if params else ""
    with urlopen(f"http://127.0.0.1:{port}/{route}{query}", timeout=8) as response:
        result = json.load(response)
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


def settled_window_size(port):
    # Layout and child resize messages arrive asynchronously. A visible tile
    # during a workspace animation can still have an intermediate size.
    last = None
    since = time.monotonic()
    def settled():
        nonlocal last, since
        size = get(port, "s")["w"][0]["sz"]
        now = time.monotonic()
        if size != last:
            last, since = size, now
        return size if now - since >= 0.5 else None
    return wait_for("child window size settles", settled)


def check_styles(port, child_port, host_pid, log, artifacts):
    """Exercise live capture/style changes through the host with a retained app."""
    current = "omarchy"
    count = 1
    clients = set(Path(tempfile.gettempdir()).glob(f"makeos-{host_pid}-client-*.log"))
    styles = [("makeos", "MakeOS"), ("macos", "macOS"), ("windows", "Windows"),
              ("windows-2000", "Windows 2000"), ("nextstep", "NeXTSTEP"),
              ("ios", "iOS"), ("android", "Android"), ("omarchy", "Omarchy"),
              ("makeos", "MakeOS"), ("omarchy", "Omarchy")]
    for index, (style, label) in enumerate(styles):
        offset = log.stat().st_size
        if current in ("ios", "android"):
            # Mobile Home consumes keyboard shortcuts; its top bar opens styles.
            get(port, "click", x=130, y=13, wait=1)
        else:
            get(port, "k", c="Space", cmd=1, wait=1)
        if current == "nextstep":
            # The catalog-filtered Workspace menu starts with Applications,
            # then Appearance. Select Appearance before typing a style name.
            get(port, "k", c="ArrowDown", wait=1)
            get(port, "k", c="enter", wait=1)
        for char in label.lower():
            get(port, "k", c="Space" if char == " " else "Key" + char.upper(), wait=1)
        get(port, "k", c="enter", wait=1)
        wait_for(style + " applied", lambda: f"wm: desktop style {style} applied" in log.read_text(errors="replace")[offset:])
        current = style
        time.sleep(0.9)  # Let the framebuffer transition and child restyle settle.
        assert any(item.get("t") == f"Count: {count}" for item in get(child_port, "snap", q="count")["s"]), style
        assert set(Path(tempfile.gettempdir()).glob(f"makeos-{host_pid}-client-*.log")) == clients, "style launched an extra client"
        save_grab(artifacts, f"style-{index}-{style}", get(port, "g", scale=0.5))
        if style == "makeos":
            tile = max(get(port, "snap", q="MpRunView")["s"], key=lambda item: item["r"][2] * item["r"][3])
            button = get(child_port, "snap", q="increment")["s"][0]["r"]
            get(port, "click", x=tile["r"][0] + button[0] + button[2]/2,
                y=tile["r"][1] + button[1] + button[3]/2, wait=1)
            count += 1
            wait_for("input inside glass window", lambda: any(item.get("t") == f"Count: {count}" for item in get(child_port, "snap", q="count")["s"]))
            get(port, "k", c="Space", cmd=1, wait=1)
            save_grab(artifacts, f"style-{index}-makeos-menu", get(port, "g", scale=0.5))
            get(port, "k", c="Escape", wait=1)
            if index == 0:
                width = get(port, "s")["w"][0]["sz"][0]
                get(port, "click", x=width/2, y=13, wait=1)
                save_grab(artifacts, "makeos-calendar", get(port, "g", scale=0.5))
                # Flyouts close on outside clicks; Escape only closes menus.
                get(port, "click", x=width-10, y=100, wait=1)
                # The lean catalog has no assistant: this requests a local
                # notification, exercising its glass without launching an app.
                get(port, "k", c="F10", wait=1)
                save_grab(artifacts, "makeos-notification", get(port, "g", scale=0.5))
        print(f"PASS: {style} renders and preserves the hosted app without extra launches", flush=True)
        assert_no_runtime_errors(log.read_text(errors="replace"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-run", action="store_true")
    parser.add_argument("--default-catalog", action="store_true", help="Use the shipped catalog; skip injected failure/build fixtures")
    parser.add_argument("--styles", action="store_true", help="Exercise all desktop styles, including MakeOS glass, with a retained app")
    parser.add_argument("--artifacts-dir", type=Path, help="New directory for retained logs, state, and frames")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    if args.artifacts_dir:
        artifacts = args.artifacts_dir.resolve()
        artifacts.mkdir(parents=True, exist_ok=False)
    else:
        artifacts = Path(tempfile.mkdtemp(prefix="makeos-smoke-"))
    state = artifacts / "state"
    state.mkdir()
    manifest = str(root / "apps/reference/Cargo.toml")
    slow = artifacts / "slow"
    (slow / "src").mkdir(parents=True)
    (slow / "Cargo.toml").write_text('[package]\nname="makeos-smoke-slow"\nversion="0.1.0"\nedition="2021"\n[workspace]\n')
    (slow / "src/main.rs").write_text("fn main() {}\n")
    (slow / "build.rs").write_text('fn main() {\n'
        'std::fs::write(std::env::var("MAKEOS_SMOKE_BUILD_MARKER").unwrap(), std::process::id().to_string()).unwrap();\n'
        'std::thread::sleep(std::time::Duration::from_secs(45));\n}\n')
    marker = artifacts / "build.pid"
    catalog = [
        {"id": "reference", "label": "Reference", "manifest": manifest,
         "package": "makeos-reference", "bin": "makeos-reference", "policy": "new"},
        {"id": "broken", "label": "Broken startup test", "manifest": manifest,
         "package": "makeos-package-does-not-exist", "bin": "missing"},
        {"id": "slow", "label": "Slow build", "manifest": str(slow / "Cargo.toml"),
         "package": "makeos-smoke-slow", "bin": "makeos-smoke-slow"},
    ]
    if not args.default_catalog:
        (state / "apps.json").write_text(json.dumps(catalog))
    env = dict(os.environ, MAKEOS_HOME=str(state), MAKEPAD_REMOTE="true", CARGO_NET_OFFLINE="true",
               MAKEOS_SMOKE_BUILD_MARKER=str(marker))
    for key in ["MAKEPAD_HOME", "MAKEPAD_WM_ROOT", "MAKEPAD_WM_TEST_APP", "MAKEPAD_WM_THEME"]:
        env.pop(key, None)
    command = ["cargo", "run"] if args.cargo_run else [str(root / "target/release/makeos")]
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
        assert "MakeOS" in status[0]["t"], status
        wait_for("desktop first frame", lambda: get(port, "snap", q="main_window").get("s"))
        assert not list(Path(tempfile.gettempdir()).glob(f"makeos-{host_pid}-client-*.log")), "unexpected startup child"
        print("PASS: desktop starts without child apps", flush=True)

        def launch(name):
            get(port, "k", c="Space", cmd=1, wait=1)
            # The inherited menu filters KeyDown characters, not TextInput.
            for char in name.lower():
                get(port, "k", c="Space" if char == " " else "Key" + char.upper(), wait=1)
            get(port, "k", c="enter", wait=1)

        launch("Reference")
        def child_remote():
            for path in Path(tempfile.gettempdir()).glob(f"makeos-{host_pid}-client-*.log"):
                result = remote(path)
                if result and result[1] not in child_pids:
                    return result
            return None
        child_port, child_pid = wait_for("reference app remote", child_remote, timeout=120)
        child_pids.append(child_pid)
        owned_groups.append(os.getpgid(child_pid))
        wait_for("reference input", lambda: get(child_port, "snap", q="increment").get("s"))
        tiles = wait_for("hosted tile", lambda: get(port, "snap", q="MpRunView").get("s"))
        tile = max(tiles, key=lambda item: item["r"][2] * item["r"][3])

        def click_child(widget):
            rect = get(child_port, "snap", q=widget)["s"][0]["r"]
            x, y = tile["r"][:2]
            get(port, "click", x=x + rect[0] + rect[2]/2, y=y + rect[1] + rect[3]/2, wait=1)

        click_child("increment")
        wait_for("forwarded pointer increments counter", lambda: any(item.get("t") == "Count: 1" for item in get(child_port, "snap", q="count")["s"]))
        click_child("message")
        get(port, "t", t="Hello through MakeOS", wait=1)
        wait_for("forwarded keyboard updates echo", lambda: any(item.get("t") == "Hello through MakeOS" for item in get(child_port, "snap", q="echo")["s"]))
        print("PASS: pointer and keyboard input cross the host/client boundary", flush=True)
        # Let the shell's arrival animation finish before recording a frame.
        time.sleep(0.6)
        grab = get(port, "g", scale=0.5)
        save_grab(artifacts, "frame", grab)

        # Workspace movement goes through the WM's own keyboard handler.
        get(port, "k", c="Key2", cmd=1, shift=1, wait=1)
        get(port, "k", c="Key1", cmd=1, wait=1)
        wait_for("original workspace is empty", lambda: not get(port, "snap", q="MpRunView")["s"])
        get(port, "k", c="Key2", cmd=1, wait=1)
        wait_for("app visible on destination workspace", lambda: get(port, "snap", q="MpRunView")["s"])
        size = settled_window_size(child_port)
        get(port, "k", c="KeyF", cmd=1, wait=1)
        wait_for("fullscreen expands child", lambda: get(child_port, "s")["w"][0]["sz"][0] > size[0] + 10)
        settled_window_size(child_port)
        get(port, "k", c="KeyF", cmd=1, wait=1)
        wait_for(f"fullscreen restores child size {size}", lambda: get(child_port, "s")["w"][0]["sz"] == size)
        assert alive(child_pid), "client died on workspace/fullscreen changes"
        print("PASS: workspace and fullscreen operations retain the app", flush=True)

        launch("Reference")
        second_port, second_pid = wait_for("second reference instance", child_remote, timeout=120)
        child_pids.append(second_pid)
        owned_groups.append(os.getpgid(second_pid))
        wait_for("second instance has independent state", lambda: any(item.get("t") == "Count: 0" for item in get(second_port, "snap", q="count")["s"]))
        assert any(item.get("t") == "Count: 1" for item in get(child_port, "snap", q="count")["s"])
        get(port, "k", c="KeyW", cmd=1, wait=1)
        wait_for("close window reaps second instance", lambda: not alive(second_pid))
        assert alive(child_pid), "closing one instance closed both"
        print("PASS: separate instances keep independent state and close individually", flush=True)

        if args.styles:
            check_styles(port, child_port, host_pid, log, artifacts)

        slow_group = None
        if not args.default_catalog:
            launch("Broken startup test")
            wait_for("failed Cargo launch diagnostic", lambda: "makeos-package-does-not-exist" in log.read_text(errors="replace"), timeout=30)
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
        for path in Path(tempfile.gettempdir()).glob(f"makeos-{host_pid}-client-*.log"):
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
        # Client logs are created by the host in its normal temporary location.
        # Copy this test host's logs before handing the report to the caller.
        host_remote = remote(log)
        if host_remote:
            for path in Path(tempfile.gettempdir()).glob(f"makeos-{host_remote[1]}-client-*.log"):
                shutil.copyfile(path, artifacts / path.name)
        print(f"Logs and app-provided frames: {artifacts}", flush=True)


if __name__ == "__main__":
    main()

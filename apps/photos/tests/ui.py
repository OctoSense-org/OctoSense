#!/usr/bin/env python3
"""Hidden-shell Memories smoke test. Uses only a local mock AI provider.

Build from the repo root:
  cargo build --locked -p octosense --no-default-features --features app-hub
Run:
  python3 apps/photos/tests/ui.py
Screenshots/logs go to target/photos-memories-ui. No personal state is used.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlencode
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[3]
STORY = {
    "title": "Summer by the water",
    "summary": "A July collection from the coast, with beach walks and a kite by the dunes.",
    "photos": ["coast", "beach", "lily-beach", "family-four-beach"],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/octosense")
    parser.add_argument("--output", type=Path, default=ROOT / "target/photos-memories-ui")
    args = parser.parse_args()
    binary = args.binary.resolve()
    assert binary.is_file(), f"Build the shell first: {binary}"
    args.output.mkdir(parents=True, exist_ok=True)
    requests = []

    class Provider(BaseHTTPRequestHandler):
        def do_POST(self):
            requests.append(json.loads(self.rfile.read(int(self.headers["Content-Length"]))))
            # Long enough to test navigation while an asynchronous call runs.
            time.sleep(1)
            body = json.dumps({"choices": [{"message": {"content": json.dumps({"memories": [STORY]})}}],
                               "usage": {"prompt_tokens": 300, "completion_tokens": 60}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    provider = ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=provider.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="photos-memories-ui-") as directory:
        state = Path(directory)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]

        def remote(route, **query):
            url = f"http://127.0.0.1:{port}/{route}?{urlencode(query)}"
            with urlopen(url, timeout=10) as response:
                return json.load(response)

        def widgets():
            return [w for w in remote("snap")["s"] if w["ty"] != "Splash"]

        def wait_for(text, kind=None, timeout=20):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                try:
                    for widget in widgets():
                        if widget.get("t") == text and (kind is None or widget["ty"] == kind):
                            return widget
                except (OSError, ValueError):
                    pass
                time.sleep(0.15)
            raise AssertionError(f"No visible {kind or 'widget'}: {text}")

        def click_widget(widget):
            x, y, width, height = widget["r"]
            remote("click", x=x + width / 2, y=y + height / 2, wait=1)

        def click(text, kind="Button"):
            click_widget(wait_for(text, kind))

        def capture(name):
            time.sleep(0.2)
            shutil.copyfile(remote("g")["png"], args.output / f"{name}.png")

        env = os.environ.copy()
        env.update(MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=str(port),
                   OCTOSENSE_HOME=str(state / "home"), OCTOS_APP_CORE_DIR=str(state / "core"),
                   OCTOSENSE_LLM_VAULT="file", MAKEPAD_STUDIO_HTTP="")
        # An inherited kernel/provider configuration must not escape the test.
        env.pop("OCTOS_APP_CORE_BIN", None)
        env.pop("MAKEPAD_APP_CONFIG", None)
        env.pop("MAKEPAD_WM_TEST_APP", None)
        try:
            for attempt in range(2):
                log_path = args.output / f"shell-{attempt}.log"
                with log_path.open("w") as log:
                    process = subprocess.Popen([str(binary), "--test-action", "phone:ios", "--test-action", "launch-photos"],
                                               cwd=ROOT, env=env, stdout=log, stderr=log)
                    try:
                        wait_for("Collections", "Label")
                        if attempt == 1:
                            wait_for(STORY["title"], "Label")
                            assert len(requests) == 1, "opening Photos must not call AI"
                            capture("reopened")
                            continue
                        click("Memories")
                        click("Create memories")
                        wait_for("Set up a model in Settings → AI providers, then try again.", "Label")
                        capture("no-provider")
                        profile = state / "core/profiles/_main.json"
                        profile.parent.mkdir(parents=True, exist_ok=True)
                        profile.write_text(json.dumps({"id": "_main", "name": "Test", "enabled": True,
                            "config": {"llm": {"primary": {"family_id": "local", "model_id": "mock-model",
                            "route": {"base_url": f"http://127.0.0.1:{provider.server_port}/v1", "api_type": "openai"}}}}}))
                        field = next(w for w in widgets() if w["ty"] == "TextInput")
                        click_widget(field)
                        remote("t", t="summer with family", wait=1)
                        click("Create memories")
                        click("Collections")
                        wait_for(STORY["title"], "Label")
                        click("Memories")
                        wait_for("1 new memory saved.", "Label")
                        capture("memories")
                        click(STORY["title"], "Label")
                        wait_for(STORY["summary"], "Label")
                        capture("story")
                        click("Play memory")
                        click("Pause")
                        click("Next")
                        wait_for("2 of 4", "Label")
                        capture("slideshow")
                        click("‹ Done")
                        click("‹ Back")
                        wait_for("Create memories", "Button")
                        saved = list((state / "home").rglob("memories.json"))
                        assert len(saved) == 1, saved
                        assert json.loads(saved[0].read_text())["memories"][0]["title"] == STORY["title"]
                        assert len(requests) == 1
                        assert "summer with family" in json.dumps(requests[0])
                        assert "{{assets}}" not in json.dumps(requests[0])
                    finally:
                        try:
                            remote("quit")
                        except OSError:
                            pass
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.terminate()
                            process.wait(timeout=5)
                        log.flush()
                        errors = [line for line in log_path.read_text().splitlines()
                                  if "[E]" in line or "on_render closure failed" in line or "callback error" in line]
                        assert not errors, "\n".join(errors)
        finally:
            provider.shutdown()
            provider.server_close()
    print(f"PASS: no provider, prompt, host completion, navigation during generation, saved story, slideshow and reopening. Captures: {args.output}")


if __name__ == "__main__":
    main()

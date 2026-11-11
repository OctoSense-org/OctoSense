#!/usr/bin/env python3
"""AI Providers end to end on a Mac, driven through Makepad's remote (instrument)
mode: no OS screenshots, only the app's own frames and rows.

    cargo build --release -p octosense
    cargo build --release -p octosense-llm-service --example read_qr_image
    desktop/scripts/ai_providers_remote.py [--output DIR]

Runs the release binary hidden with its own OCTOSENSE_HOME, octos core dir and
file vaults under the output folder (neither ~/.octosense nor the login keychain
is touched). Every key in it is fake, and the binary runs under sandbox-exec with
outbound HTTPS denied: no fake key reaches a real provider, and a provider test
fails as a network failure. A local fake endpoint stands in for a provider that
answers. The flow:

  1. AI Providers opens; the host reports an image picker.
  W1. Add DeepSeek through the sheet: search, model, route, a fake key, Test
      Connection. The provider can't be reached, so Save stays off until Save
      Without Testing saves it as the primary.
  W2. Change its model in Details (DeepSeek V4 Pro).
  W3. Add Z.ai as a fallback on another model (GLM-4.7) and a custom endpoint, the
      fake one. The test passes and Add as Fallback saves it.
  W4. The list and _main.json say so; Test Connection from Details passes; both are
      removed with Remove.
  2. Import into the empty list by paste: an old single-provider code becomes the
     primary.
  3. Add OpenAI through the sheet, saved without testing.
  4. Import by drop: the fixture QR-A dropped on the sheet through the native drop
     path (/drop), PIN typed. An import only adds: the primary and OpenAI stay
     first, QR-A's providers follow as fallbacks, and its DeepSeek key gets a slot
     of its own (DEEPSEEK_2_API_KEY), so the primary's key is not overwritten.
  5. Import QR-A again by Choose Image (OCTOSENSE_LLM_TEST_IMAGE answers instead of
     the open panel): a wrong PIN is refused and changes nothing, the right one
     changes nothing either ("Already saved").
  6. Show QR Code, grab the sheet, read the QR out of the grab (the service's own
     image search, rqrr) and open it with the PIN on the sheet: it equals the saved
     set and keys, both DeepSeek slots included. The countdown ticks, and at 0 the
     sheet closes by itself (OCTOSENSE_LLM_QR_SECONDS shortens five minutes to
     --qr-seconds).
  7. No key text in /snap, /d, /log or the host log; no panic, no Splash error;
     /gq exits.

Artifacts: grabs/, export.png (the export sheet at full scale) and export-pin.txt,
for scanning on a phone.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/connected-e2e"))
from native import Native

FIXTURE = ROOT / "apps/ai-providers/config/tests/fixtures/qr-a.png"
FIXTURE_PIN = "7K3M-9QX2"
FAKE_KEY = "sk-test-0000000000001234"
ZAI_KEY = "sk-test-0000000000005678"
# The old single-provider code the empty list is filled from (its key is fake).
LEGACY_CODE = '{"llm_family":"deepseek","llm_model":"deepseek-v4-flash","llm_key":"sk-test-legacy-00000000fcb0"}'
# What a Splash script error logs.
SPLASH_ERRORS = re.compile(r"pop_stack_resolved|mes empty|splash host callback error|\[E\] |ScriptError")
# Every key the run handles: none may show up in the UI tree or the logs.
SECRETS = re.compile(r"sk-test-|zai-test-|0000000000001234|0000000000005678")
SANDBOX = '(version 1)(allow default)(deny network-outbound (remote tcp "*:443"))'
IMPORT_ROWS = ("Import Code from Image…", "Paste Code…", "Scan QR Code…")


def passed(text):
    print("PASS: " + text, flush=True)


class Instrument(Native):
    def call(self, route, **query):
        try:
            return super().call(route, **query)
        except urllib.error.HTTPError as error:
            # The input was applied; only the wait for its frame failed, while the
            # renderer's queue was full for a moment. A retry would press twice.
            detail = error.read().decode("utf-8", errors="replace")
            if "requested input frame could not be submitted" not in detail:
                error.msg = detail or error.msg
                raise
            time.sleep(.3)
            return {}


class Endpoint(http.server.BaseHTTPRequestHandler):
    """A provider that answers: OpenAI-compatible chat completions for one key."""

    def answer(self, ok, body):
        data = json.dumps(body if ok else {"error": {"message": "Incorrect API key"}}).encode()
        self.send_response(200 if ok else 401)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        self.rfile.read(int(self.headers.get("content-length", 0)))
        self.answer(self.headers.get("authorization") == "Bearer " + ZAI_KEY, {"choices": []})

    def do_GET(self):
        self.answer(self.headers.get("authorization") == "Bearer " + ZAI_KEY, {"data": [{"id": "glm-4.7"}]})

    def log_message(self, *_):
        pass


class Run:
    def __init__(self, binary, read_qr, work, qr_seconds):
        self.work = work
        self.read_qr = read_qr
        self.qr_seconds = qr_seconds
        self.profile = work / "octos/profiles/_main.json"
        (work / "home").mkdir(parents=True, exist_ok=True)
        (work / "octos").mkdir(parents=True, exist_ok=True)
        (work / "grabs").mkdir(parents=True, exist_ok=True)
        env = {k: v for k, v in os.environ.items() if k not in ("MAKEPAD_HOME", "MAKEPAD_WM_ROOT", "MAKEPAD_WM_THEME")}
        env.update(OCTOSENSE_HOME=str(work / "home"), OCTOS_APP_CORE_DIR=str(work / "octos"),
                   OCTOSENSE_MAIL_VAULT="file", OCTOSENSE_LLM_VAULT="file",
                   OCTOSENSE_LLM_TEST_IMAGE=str(FIXTURE), OCTOSENSE_LLM_QR_SECONDS=str(qr_seconds))
        self.app = Instrument("sandbox-exec", ["-p", SANDBOX, str(binary)], work, "host", env=env, cwd=work)
        self.log = work / "host.log"

    # ---- rows ----------------------------------------------------------------

    def rows(self):
        return self.app.rows()

    def wait(self, predicate, what, timeout=20):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            found = predicate()
            if found:
                return found
            time.sleep(.1)
        self.grab("failed")
        raise AssertionError("never happened: " + what)

    def row(self, text=None, kind=None, ident=None):
        """A sheet draws last, so its control is the last of that name."""
        found = [r for r in self.rows() if (text is None or r.get("t") == text)
                 and (kind is None or r.get("ty") == kind) and (ident is None or r.get("i") == ident)]
        return found[-1] if found else None

    def said(self, words):
        return next((r for r in self.rows() if words in (r.get("t") or "") and r.get("ty") == "Label"), None)

    def shown(self, text, kind=None, timeout=20):
        return self.wait(lambda: self.row(text, kind), repr(text) + " on screen", timeout)

    def click(self, text, kind="Button", timeout=20):
        self.app.click_row(self.shown(text, kind, timeout))

    def field(self, ident, words):
        self.app.click_row(self.wait(lambda: self.row(ident=ident, kind="TextInput"), "the field " + ident))
        self.app.call("k", k="press", c="KeyA", cmd=1, wait=1)
        self.app.call("k", k="press", c="Backspace", wait=1)
        self.app.call("t", t=words, wait=1)
        time.sleep(.6)

    def grab(self, name, scale=.5):
        for _ in range(5):
            png = self.app.call("g", scale=scale).get("png")
            if png:
                shutil.copyfile(png, self.work / "grabs" / (name + ".png"))
                return self.work / "grabs" / (name + ".png")
            time.sleep(1)
        raise AssertionError("no frame for " + name)

    def wait_log(self, words, timeout=20):
        self.wait(lambda: words in self.log.read_text(errors="replace"), "the log says: " + words, timeout)

    # ---- the profile ---------------------------------------------------------

    def config(self):
        return json.loads(self.profile.read_text())["config"]

    def saved(self):
        llm = self.config().get("llm") or {}
        return [s for s in [llm.get("primary")] + llm.get("fallbacks", []) if s]

    def families(self):
        return " ".join(s["family_id"] for s in self.saved())

    def routes(self):
        return " ".join("%s:%s" % (s.get("model_id", "-"), (s.get("route") or {}).get("api_key_env", "-")) for s in self.saved())

    def keys_end_with(self, **want):
        env = self.config()["env_vars"]
        assert sorted(env) == sorted(want) and all(env[k].endswith(v) for k, v in want.items()), \
            "the keys are " + ", ".join(sorted(env))

    def profile_text(self):
        return self.profile.read_text() if self.profile.exists() else ""

    def changed_from(self, before, what):
        self.wait(lambda: self.profile_text() != before, what + " changes the profile")

    # ---- the app -------------------------------------------------------------

    def add_model(self, name, family, key, action, model=None, route=None, base=None):
        """The add sheet, driven like a person. Returns how it saved: tested or untested."""
        before = self.profile_text()
        self.click("Add Model…")
        self.wait(lambda: self.row(ident="search", kind="TextInput"), "the sheet's search field")
        time.sleep(1)
        self.field("search", family)
        time.sleep(1)
        self.app.click_row(self.shown(family, "Label"))
        time.sleep(1)
        self.grab(name + "-1-family")
        self.click("Next")
        time.sleep(1)
        # However long the provider's list, Next stays in reach, inside the sheet.
        self.shown("Next")
        if model:
            self.app.click_row(self.shown(model, "Label"))
            time.sleep(.6)
        self.grab(name + "-2-model")
        self.click("Next")
        time.sleep(1)
        self.shown("Official API", "Label")
        if route:
            self.app.click_row(self.shown(route, "Label"))
            time.sleep(.6)
        if base:
            self.field("base_url", base)
        self.grab(name + "-3-route")
        self.click("Next")
        time.sleep(1)
        self.field("key", key)
        self.grab(name + "-4-key")
        self.click("Next")
        time.sleep(1)
        self.shown("Test Connection")
        # Off until a test: a press on it does nothing.
        self.click(action)
        time.sleep(1)
        assert self.profile_text() == before and self.row("Test Connection"), name + ": saved before a test"
        self.grab(name + "-5-review")
        self.click("Test Connection")
        how = self.wait(lambda: "tested" if self.said("Connected") else "untested" if self.row("Save Without Testing") else None,
                        name + ": the test ended", 40)
        self.grab(name + "-5-tested")
        if how == "tested":
            self.click(action)
        else:
            assert self.said("Couldn't connect"), name + ": no failure line"
            self.click("Save Without Testing")
        self.changed_from(before, name + "'s save")
        return how

    def sheet_closes(self, marker):
        self.wait(lambda: not self.row(ident=marker), "the sheet closes")

    def details_rows(self):
        return sorted((r for r in self.rows() if r.get("t") == "Details…"), key=lambda r: r["r"][1])

    def details_of(self, name):
        title = self.shown(name, "Label")
        self.app.click_row(min(self.details_rows(), key=lambda r: abs(r["r"][1] - title["r"][1])))
        self.shown("Done")

    def done(self):
        self.click("Done")
        self.wait(lambda: not self.row("Model:"), "Details closes")

    def open_import(self):
        self.app.call("m", k="scroll", x=500, y=400, dy=3000, wait=1)
        time.sleep(1)
        self.app.click_row(self.wait(lambda: next((r for r in self.rows() if r.get("t") in IMPORT_ROWS), None), "the import button"))
        self.wait(lambda: self.row(ident="pin", kind="TextInput"), "the import sheet")
        time.sleep(1)

    def type_pin(self, pin):
        self.field("pin", pin)
        self.app.click_row(self.wait(lambda: self.row(ident="import_btn"), "Import"))

    def flow(self):
        app = self.app
        self.wait(lambda: app.call("snap", q="main_window").get("s"), "the desktop", 30)
        passed("desktop up")

        # 1. AI Providers opens and offers the image import.
        app.call("k", k="press", c="Space", cmd=1, wait=1)
        for c in "ai providers":
            app.call("k", k="press", c="Space" if c == " " else "Key" + c.upper(), wait=1)
        time.sleep(.5)
        app.call("k", k="press", c="ReturnKey", wait=1)
        self.wait_log("card: os.ai-providers running under", 40)
        self.shown("No Models Yet", "Label", 30)
        self.shown("Import Code from Image…", "Button")
        passed("AI Providers opened; the host has an image picker (Import Code from Image…)")

        # W1. DeepSeek through the sheet; the provider is unreachable (sandbox).
        how = self.add_model("w1", "DeepSeek", FAKE_KEY, "Save as Primary", model="DeepSeek V4 Flash")
        assert how == "untested", "DeepSeek's test passed under the sandbox"
        self.shown("Details…", "Button")
        assert self.config()["llm"] == {"fallbacks": [], "primary": {"family_id": "deepseek", "model_id": "deepseek-v4-flash"}}, self.config()["llm"]
        self.keys_end_with(DEEPSEEK_API_KEY="1234")
        passed("DeepSeek added through the sheet: search, model (DeepSeek V4 Flash), route (Official API), key, Test Connection failed, Save Without Testing")

        # W2. The model pop-up in Details.
        self.details_of("DeepSeek V4 Flash")
        app.click_row(self.shown("DeepSeek V4 Flash", "Button"))
        self.shown("DeepSeek V4 Pro", "PopupMenuItem")
        self.grab("w2-menu")
        app.click_row(self.row("DeepSeek V4 Pro", "PopupMenuItem"))
        self.shown("DeepSeek V4 Pro", "Label")
        self.done()
        assert self.config()["llm"] == {"fallbacks": [], "primary": {"family_id": "deepseek", "model_id": "deepseek-v4-pro"}}, self.config()["llm"]
        passed("Details' model pop-up lists DeepSeek's models and switched to DeepSeek V4 Pro")

        # W3. Z.ai as a fallback on GLM-4.7 and a custom endpoint, the fake one: the test passes.
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        base = "http://127.0.0.1:%d/v1" % server.server_address[1]
        how = self.add_model("w3", "Z.ai", ZAI_KEY, "Add as Fallback", model="GLM-4.7", route="Custom Endpoint…", base=base)
        assert how == "tested", "Z.ai was not saved after a passing test"
        self.wait(lambda: len(self.details_rows()) == 2, "two rows")
        want = {"fallbacks": [{"family_id": "zai", "model_id": "glm-4.7", "route": {"api_type": "openai", "base_url": base}}],
                "primary": {"family_id": "deepseek", "model_id": "deepseek-v4-pro"}}
        assert self.config()["llm"] == want, self.config()["llm"]
        self.keys_end_with(DEEPSEEK_API_KEY="1234", ZAI_API_KEY="5678")
        passed("Z.ai added as a fallback on GLM-4.7 and a custom endpoint; Test Connection passed, Add as Fallback saved it")

        # W4. The list shows both; the fallback's test passes; both are removed.
        self.grab("w4-saved-models")
        self.details_of("GLM-4.7")
        self.click("Test Connection")
        self.wait(lambda: self.said("Connected"), "Details says Connected", 30)
        self.grab("w4-row-tested")
        self.done()
        for name in ("DeepSeek V4 Pro", "GLM-4.7"):
            self.details_of(name)
            self.click("Remove…")
            self.shown("Cancel")
            time.sleep(.5)
            self.click("Remove")
            self.wait(lambda: not self.row("Model:"), "Details closes after Remove")
            time.sleep(1)
        self.shown("No Models Yet", "Label")
        assert "llm" not in self.config(), self.config()
        self.grab("w4-empty")
        passed("the list showed DeepSeek V4 Pro (primary) and GLM-4.7 (fallback); the fallback's test said Connected; both removed")

        # 2. Import into the empty list by pasting an old single-provider code.
        self.open_import()
        self.field("code", LEGACY_CODE)
        app.click_row(self.wait(lambda: self.row(ident="import_btn"), "Import"))
        self.sheet_closes("import_btn")
        assert self.routes() == "deepseek-v4-flash:-", self.routes()
        self.keys_end_with(DEEPSEEK_API_KEY="fcb0")
        self.grab("imported-by-paste")
        passed("an old single-provider code pasted into the empty list: DeepSeek deepseek-v4-flash is the primary")

        # 3. OpenAI through the sheet.
        how = self.add_model("add1", "OpenAI", FAKE_KEY, "Add as Fallback")
        assert how == "untested", "OpenAI's test passed under the sandbox"
        assert self.families() == "deepseek openai", self.families()
        self.keys_end_with(DEEPSEEK_API_KEY="fcb0", OPENAI_API_KEY="1234")
        self.grab("added-openai")
        passed("OpenAI added through the sheet (its default model, official route), saved without testing")

        # 4. Import QR-A by a drop on the sheet: appended, no confirm step.
        self.open_import()
        pin = self.row(ident="pin", kind="TextInput")
        # No wait for a frame: the reply says at once who took the drop, and a retry would drop twice.
        drop = app.call("drop", path=str(FIXTURE), x=pin["r"][0] + pin["r"][2] / 2, y=pin["r"][1])
        assert drop.get("drop_handled") is True and drop.get("drag_response") == "copy", "the sheet did not take the drop: %s" % drop
        self.wait_log("llm: image dropped on the import sheet")
        self.wait(lambda: self.said("Code read from the image"), "the sheet asks for the PIN")
        self.grab("drop-read")
        self.type_pin(FIXTURE_PIN)
        self.sheet_closes("import_btn")
        assert self.families() == "deepseek openai deepseek zai", self.families()
        assert self.routes() == "deepseek-v4-flash:- gpt-4o:- deepseek-chat:DEEPSEEK_2_API_KEY glm-4.6:-", self.routes()
        self.keys_end_with(DEEPSEEK_API_KEY="fcb0", OPENAI_API_KEY="1234", DEEPSEEK_2_API_KEY="0000", ZAI_API_KEY="0000")
        self.grab("imported-by-drop")
        passed("dropped QR-A appended as fallbacks with no confirm step: QR-A's DeepSeek in DEEPSEEK_2_API_KEY, the primary's key untouched")

        # 5. QR-A again by Choose Image, a wrong PIN first: the same code changes nothing.
        before = self.profile_text()
        self.open_import()
        self.click("Choose Image…")
        self.wait(lambda: self.said("Code read from the image"), "the sheet asks for the PIN")
        self.type_pin("0000-0000")
        time.sleep(4)
        assert self.row(ident="import_btn"), "a wrong PIN closed the sheet"
        assert self.profile_text() == before, "a wrong PIN changed the profile"
        self.grab("wrong-pin")
        self.type_pin(FIXTURE_PIN)
        self.wait(lambda: self.said("Already saved: DeepSeek · deepseek-chat, Z.ai · glm-4.6."), "the second import is answered")
        self.sheet_closes("import_btn")
        assert self.profile_text() == before, "importing QR-A again changed the profile"
        self.grab("reimported")
        self.click("OK")
        passed("QR-A chosen and imported again after a refused wrong PIN: nothing changed, no duplicates")

        # 6. The phone's QR: show the code, read it from the grab.
        mark = len(self.log.read_text(errors="replace").splitlines())
        self.click("Show QR Code…")
        pin = self.wait(lambda: self.row(ident="pin", kind="Label"), "the export sheet", 40)["t"]
        shown_at = time.monotonic()
        time.sleep(1)
        export = self.grab("export", scale=1)
        shutil.copyfile(export, self.work / "export.png")
        (self.work / "export-pin.txt").write_text(pin + "\n")
        read = subprocess.run([str(self.read_qr), str(export), pin, str(self.work / "octos")], capture_output=True, text=True)
        assert read.returncode == 0, "the exported QR does not match the profile: " + read.stdout + read.stderr
        for words in ('"same_as_profile":true', '"family":"openai"', '"DEEPSEEK_2_API_KEY":"••••0000"', '"DEEPSEEK_API_KEY":"••••fcb0"'):
            assert words in read.stdout, words + " is not in " + read.stdout
        passed("export QR read back from the grab (rqrr) with the sheet's PIN: same set and keys as _main.json, both DeepSeek slots included")
        first = self.row(ident="countdown")["t"]
        self.wait(lambda: (self.row(ident="countdown") or {}).get("t") != first, "the countdown ticks", 6)
        self.wait(lambda: not self.row(ident="countdown"), "the export sheet closes", self.qr_seconds * 2 + 10)
        closed = time.monotonic() - shown_at
        assert closed <= self.qr_seconds + 2, "the export sheet closed after %.0f s, not by its own countdown" % closed
        self.grab("export-closed")
        late = [l for l in self.log.read_text(errors="replace").splitlines()[mark:] if SPLASH_ERRORS.search(l)]
        assert not late, "Splash errors while the export sheet was up: " + str(late[:5])
        passed("the countdown ticked and the sheet closed itself after %.0f s (%d s lifetime); no Splash error during the export" % (closed, self.qr_seconds))

        # 7. No key text anywhere the UI or logs show; no panic.
        port = app.endpoint
        def text_of(route):
            with urllib.request.urlopen(port + "/" + route, timeout=20) as response:
                return response.read().decode("utf-8", "replace")
        checked = {"snap.json": text_of("snap?all=1"), "dump.txt": text_of("d"), "remote-log.json": text_of("log?n=5000")}
        for name, text in checked.items():
            (self.work / name).write_text(text)
        checked["host.log"] = self.log.read_text(errors="replace")
        for name, text in checked.items():
            leak = SECRETS.search(text)
            assert not leak, "key text in " + name + ": " + text[max(0, leak.start() - 40):leak.end() + 10]
        assert "panicked" not in checked["host.log"] and "panicked" not in checked["remote-log.json"], "panic in the log"
        errors = [l for l in checked["host.log"].splitlines() if SPLASH_ERRORS.search(l)]
        assert not errors, "Splash errors in the host log: " + str(errors[:5])
        passed("no key text in /snap, /d, /log or the host log; no panic, no Splash error")

        try:
            app.call("gq", scale=.5)
        except Exception:
            pass
        self.wait(lambda: app.child.poll() is not None, "the shell exits after /gq", 15)
        passed("shell exited after /gq; grabs in %s, export.png and export-pin.txt in %s" % (self.work / "grabs", self.work))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/octosense")
    parser.add_argument("--read-qr", type=Path, default=ROOT / "target/release/examples/read_qr_image")
    parser.add_argument("--output", type=Path, help="Artifacts folder; a new temporary one by default")
    parser.add_argument("--qr-seconds", type=int, default=20, help="The phone QR's lifetime: long enough to grab and read the code")
    args = parser.parse_args()
    for path, build in ((args.binary, "cargo build --release -p octosense"),
                        (args.read_qr, "cargo build --release -p octosense-llm-service --example read_qr_image")):
        if not path.exists():
            raise SystemExit("no %s: %s" % (path, build))
    work = (args.output or Path(tempfile.mkdtemp(prefix="octosense-ai-providers"))).resolve()
    print("artifacts: %s" % work, flush=True)
    run = Run(args.binary.resolve(), args.read_qr.resolve(), work, args.qr_seconds)
    try:
        run.flow()
    except AssertionError as error:
        print("FAIL: %s" % error, flush=True)
        raise SystemExit(1)
    finally:
        run.app.close()


if __name__ == "__main__":
    main()

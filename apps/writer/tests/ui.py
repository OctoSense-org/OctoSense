#!/usr/bin/env python3
"""Writer in App Hub's card-host: every screen and state, light and dark.

    python3 apps/writer/tests/ui.py --card-host <App Hub>/target/release/card-host
        [--output target/writer-ui] [--appearance light|dark|both] [--size 1100x760] [--port 8913]

card-host serves no host services, so there is no `word` engine there. Run A
uses the shipped bundle as it is: writing, the document list, autosave across
a restart, deleting, and the "Writer can't … here" answers to Save and
Preview. Runs B and C use a scratch copy of the bundle in which engine() is
replaced by the dev fixture's stand-in engine (`dev-fixture/engine.splash`),
with its settings in the app's storage as `dev/fixture.json`: B answers slowly
(saving, preview, outline, export), C refuses word.convert (the save and
export errors). The shipped bundle carries no fixture code. The real engine's
flows are checked in a shell, not here.

Each run starts its own hidden card-host on 127.0.0.1:<port> with the app data
of the run before it (MAKEPAD_HIDE_WINDOWS=1, MAKEPAD_REMOTE), drives it over
the remote bridge (/snap, /click, /t, /k), saves grabs (/g) into
<output>/<appearance>/, ends with /gq, and checks that the process exited and
that its log holds no script error. A handler that overran its 64 ms budget
(on a busy machine) is listed apart in the receipt. It refuses to start while
something else answers on the port.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

APP = Path(__file__).resolve().parent.parent
FIXTURE = APP / "dev-fixture"

DOC1 = """# Field notes from the Alder Street studio
Every Thursday the old print shop fills with people who make things. Painters set up by the north windows and somebody always brings bread.
## What we agreed
- Clean as you go.
- Label what is yours.
- Ask before you borrow.
> A studio is a place where unfinished things are welcome.
## Next steps
Book the kiln two days ahead and leave a note on the board by the sink."""

DOC2 = """Letter to the neighbours
Dear neighbours,
We are planning a small open studio on Saturday the 18th, from two until six. There will be tea, a few prints for sale, and a table where children can try letterpress.
If the press is too loud, knock on the side door and we will close the windows.
Warmly,
Ana and the Alder Street members"""

DOC3 = """# Winter reading list
Five books the studio passed around this year.
1. The Craftsman by Richard Sennett
2. A Pattern Language by Christopher Alexander
3. Ways of Seeing by John Berger
4. The Shape of Design by Frank Chimero
5. Bird by Bird by Anne Lamott"""

DOC4 = """Things to buy
- Ink for the proofing press
- A ream of cotton paper"""


# ---- the remote bridge -------------------------------------------------------

class Remote:
    def __init__(self, port):
        self.base = f"http://127.0.0.1:{port}"

    def get(self, path, timeout=60, **params):
        url = self.base + path + ("?" + urllib.parse.urlencode(params) if params else "")
        try:
            with urllib.request.urlopen(url, timeout=timeout) as r:
                data = r.read()
        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", "replace")
            # A hidden window sometimes can't submit the frame an input waits
            # for; the input itself was applied, so it must not be sent again.
            if e.code == 404 and "could not be submitted" in body:
                time.sleep(0.4)
                return {"ok": 1, "frame": "not submitted"}
            raise RuntimeError(f"{path} {params}: {e.code} {body}")
        try:
            return json.loads(data, strict=False)
        except ValueError:
            return data.decode("utf-8", "replace")

    def up(self):
        try:
            self.get("/s", timeout=2)
            return True
        except (OSError, RuntimeError):
            return False

    def widgets(self):
        snap = self.get("/snap")
        return [w for w in snap.get("s", []) if w.get("r", [0, 0, 0, 0])[2] > 0 and w.get("r", [0, 0, 0, 0])[3] > 0]

    def texts(self):
        return [(w["ty"], w.get("t")) for w in self.widgets() if w.get("t") and w["ty"] != "Splash"]

    def has(self, needle):
        return any(needle in (t or "") for _, t in self.texts())

    def find(self, text=None, ty=None, contains=None):
        for w in self.widgets():
            if ty is not None and w.get("ty") != ty:
                continue
            if text is not None and w.get("t") != text:
                continue
            if contains is not None and contains not in (w.get("t") or ""):
                continue
            return w
        return None

    def tap_at(self, x, y):
        self.get("/click", x=round(x, 1), y=round(y, 1), wait=1)
        time.sleep(0.25)

    def tap(self, text=None, ty=None, contains=None):
        w = self.find(text, ty, contains)
        if w is None:
            have = [t for _, t in self.texts()][:40]
            raise LookupError(f"no widget text={text!r} ty={ty} contains={contains!r}; have {have}")
        x, y, ww, hh = w["r"]
        self.tap_at(x + ww / 2, y + hh / 2)

    def type_text(self, text):
        # /t takes text; a newline goes as a Return key press.
        parts = text.split("\n")
        for n, part in enumerate(parts):
            if part:
                self.get("/t", t=part, wait=1)
            if n < len(parts) - 1:
                self.key("ReturnKey")
        time.sleep(0.2)

    def key(self, code):
        self.get("/k", k="press", c=code, wait=1)

    def wait(self, cond, timeout=20.0, what="condition"):
        end = time.time() + timeout
        while time.time() < end:
            try:
                if cond():
                    return
            except (OSError, RuntimeError):
                pass
            time.sleep(0.25)
        raise TimeoutError(f"timed out waiting for {what}")

    def wait_text(self, needle, timeout=20.0):
        self.wait(lambda: self.has(needle), timeout, repr(needle))


# ---- one appearance ----------------------------------------------------------

class Journey:
    def __init__(self, args, appearance, scratch_bundle):
        self.args = args
        self.appearance = appearance
        self.scratch_bundle = scratch_bundle
        self.remote = Remote(args.port)
        self.out = Path(args.output) / appearance
        self.data = Path(args.output) / "data" / appearance
        shutil.rmtree(self.out, ignore_errors=True)
        shutil.rmtree(self.data, ignore_errors=True)
        self.out.mkdir(parents=True)
        (self.data / "os.writer").mkdir(parents=True)
        self.proc = None
        self.run_name = None
        self.steps = []
        self.runs = []

    def launch(self, run, bundle, fixture=None):
        if self.remote.up():
            raise SystemExit(f"something already answers on port {self.args.port}")
        dev = self.data / "os.writer" / "dev"
        shutil.rmtree(dev, ignore_errors=True)
        if fixture is not None:
            dev.mkdir()
            (dev / "fixture.json").write_text(json.dumps(fixture))
        self.run_name = run
        env = dict(os.environ, MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=f"127.0.0.1:{self.args.port}",
                   MAKEPAD_WIDGET_STYLE="macos-dark" if self.appearance == "dark" else "macos")
        self.log_path = self.out / f"card-host-{run}.log"
        log = open(self.log_path, "w")
        argv = [self.args.card_host, "--bundle", str(bundle), "--system", "--app-data", str(self.data)]
        if self.args.size:
            argv += ["--size", self.args.size]
        self.proc = subprocess.Popen(argv, env=env, stdout=log, stderr=subprocess.STDOUT)
        end = time.time() + 60
        while not self.remote.up():
            if self.proc.poll() is not None:
                raise RuntimeError(f"card-host exited early; see {self.log_path}")
            if time.time() > end:
                raise RuntimeError("card-host's remote never answered")
            time.sleep(0.5)
        self.remote.wait(lambda: self.remote.find("Writer", "Label") or self.remote.find("New document"), 30, "Writer")
        time.sleep(0.6)

    def finish(self):
        reply = self.remote.get("/gq")
        try:
            self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"card-host {self.proc.pid} did not exit")
        log = self.log_path.read_text(errors="replace").splitlines()
        flagged = [l for l in log if "[E]" in l or "splash:" in l or "callback error" in l or "on_render closure failed" in l]
        overruns = [l for l in flagged if "script time budget exceeded" in l]
        self.runs.append({"run": self.run_name, "exit_code": self.proc.returncode,
                          "gq": reply.get("quit") if isinstance(reply, dict) else None,
                          "script_errors": [l for l in flagged if l not in overruns], "budget_overruns": overruns})
        self.proc = None

    def close_after_failure(self):
        if self.proc is None or self.proc.poll() is not None:
            return
        try:
            self.remote.get("/quit", timeout=10)
            self.proc.wait(timeout=15)
        except Exception:
            self.proc.terminate()
            self.proc.wait(timeout=15)

    def grab(self, name, note, settle=0.4):
        time.sleep(settle)
        for attempt in range(12):
            try:
                reply = self.remote.get("/g")
                break
            except RuntimeError:
                # A hidden window can miss a present; ask again.
                if attempt == 11:
                    raise
                time.sleep(0.3)
        png = reply["png"]
        shutil.copy(png[0] if isinstance(png, list) else png, self.out / f"{name}.png")
        self.steps.append({"run": self.run_name, "grab": f"{name}.png", "note": note,
                           "texts": [t for _, t in self.remote.texts() if t != "Card host [remote]"][:60]})
        print(f"{self.appearance} {name}: {note}", flush=True)

    def open_doc(self, title_start):
        self.remote.tap(contains=title_start)
        time.sleep(0.4)

    def to_text_end(self):
        w = max((w for w in self.remote.widgets() if w["ty"] == "TextInput"), key=lambda w: w["r"][2] * w["r"][3])
        x, y, ww, hh = w["r"]
        self.remote.tap_at(x + ww / 2, y + min(hh / 2, 120))
        for _ in range(8):
            self.remote.key("PageDown")
        self.remote.key("End")

    def type_into_editor(self, text):
        self.remote.tap(ty="TextInput")
        self.remote.type_text(text)

    def run(self):
        r = self.remote
        shipped = APP / "bundle"

        # ---- A: the shipped bundle; card-host has no word engine ------------
        self.launch("A", shipped)
        self.grab("01-docs-empty", "first launch: the empty state, one action")
        r.tap("New document", "Button")
        self.grab("02-editor-blank", "a new document: blank page, the formatting tip")
        self.type_into_editor(DOC1)
        time.sleep(1.2)
        self.grab("03-editor-writing", "typed; the draft autosaves")
        r.tap("Save", "Button")
        r.wait_text("Writer can't save Word documents")
        self.grab("04-editor-save-unavailable", "Save without a word engine: a clear message, the draft kept")
        r.tap("OK", "Button")
        r.tap("Preview", "Button")
        r.wait_text("Writer can't show a preview")
        self.grab("05-editor-preview-unavailable", "Preview without a word engine")
        r.tap("‹ Documents", "Button")
        for text in (DOC2, DOC3, DOC4):
            r.tap("New document", "Button")
            self.type_into_editor(text)
            time.sleep(1.0)
            r.tap("‹ Documents", "Button")
            time.sleep(0.4)
        self.grab("06-docs-list", "four documents, newest first")
        self.open_doc("Things to buy")
        r.tap("Delete", "Button")
        self.grab("07-editor-delete-confirm", "Delete asks for a second tap")
        r.tap("Tap again to delete", "Button")
        time.sleep(0.5)
        self.grab("08-docs-after-delete", "the document is gone")
        self.finish()
        self.launch("A-restart", shipped)
        self.grab("09-docs-after-restart", "after a restart the list and the drafts are back")
        self.open_doc("Letter to the neighbours")
        self.grab("10-editor-reopened", "a reopened draft, exactly as typed")
        self.finish()

        # ---- B: the stand-in engine, slow answers ---------------------------
        self.launch("B", self.scratch_bundle, {"delay": 3})
        self.open_doc("Field notes from the Alder")
        r.tap("Save", "Button")
        time.sleep(0.3)
        self.grab("11-editor-saving", "Save in progress")
        r.wait_text("Saved as DOCX", 20)
        self.grab("12-editor-saved", "saved as a Word document")
        r.tap("Preview", "Button")
        time.sleep(0.3)
        self.grab("13-editor-opening-preview", "the engine reads the saved document back")
        r.wait(lambda: r.find("Export", "Button"), 30, "the preview")
        self.grab("14-preview", "the outline from its headings, the document as saved")
        rows = sorted((w for w in r.widgets() if w.get("t") == "What we agreed" and w["ty"] == "Label"), key=lambda w: w["r"][1])
        x, y, ww, hh = rows[0]["r"]
        r.tap_at(x + ww / 2, y + hh / 2)
        self.grab("15-preview-section", "an outline heading shows its section")
        r.tap("Show whole document", "Button")
        r.tap("Export", "Button")
        self.grab("16-export", "four formats, one line each")
        r.tap("PDF", "Label")
        time.sleep(0.3)
        self.grab("17-export-busy", "exporting a PDF")
        r.wait_text("Saved in Writer: exports/", 20)
        r.tap("OpenDocument", "Label")
        r.wait_text("Field notes from the Alder Street studio.odt", 20)
        self.grab("18-export-done", "two copies made, each saved in Writer")
        r.tap("‹ Preview", "Button")
        r.tap("‹ Edit", "Button")
        r.tap("‹ Documents", "Button")
        self.grab("19-docs-saved-chip", "the saved document carries a DOCX chip")
        self.open_doc("Field notes from the Alder")
        self.to_text_end()
        r.type_text("\nThe kettle is on from nine.")
        time.sleep(1.2)
        self.grab("20-editor-changed", "edited after saving")
        r.tap("‹ Documents", "Button")
        self.grab("21-docs-edited-chip", "the list says it changed since the save")
        self.open_doc("Field notes from the Alder")
        r.tap("Save", "Button")
        r.wait_text("Saved as DOCX", 20)
        self.finish()

        # ---- C: the stand-in engine refusing word.convert --------------------
        self.launch("C", self.scratch_bundle, {"delay": 0.4, "fail": ["word.convert"]})
        self.open_doc("Winter reading list")
        r.tap("Save", "Button")
        r.wait_text("Writer couldn't save Word documents", 15)
        self.grab("22-editor-save-error", "the engine refuses: its message, the draft kept")
        r.tap("‹ Documents", "Button")
        self.open_doc("Field notes from the Alder")
        r.tap("Preview", "Button")
        r.wait(lambda: r.find("Export", "Button"), 30, "the preview")
        r.tap("Export", "Button")
        r.tap("PDF", "Label")
        r.wait_text("Writer couldn't export copies", 15)
        self.grab("23-export-error", "a failed export says why and offers to try again")
        self.finish()

    def receipt(self):
        path = self.out / "receipt.json"
        path.write_text(json.dumps({"appearance": self.appearance, "size": self.args.size, "runs": self.runs, "steps": self.steps}, indent=1))
        return path


def scratch_bundle(output):
    """A scratch copy of the bundle with the stand-in engine in place of engine()."""
    dst = Path(output) / "bundle"
    shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(APP / "bundle", dst)
    main = dst / "main.splash"
    text = main.read_text()
    start = text.index("fn engine(")
    depth, end = 0, None
    for i in range(text.index("{", start), len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                end = i + 1
                break
    if end is None or "host.request(method" not in text[start:end] or text.count("fn engine(") != 1:
        raise SystemExit("engine() in main.splash is not what this script replaces: update scratch_bundle()")
    main.write_text(text[:start] + (FIXTURE / "engine.splash").read_text().rstrip() + text[end:])
    return dst


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--card-host", required=True, help="App Hub's card-host binary")
    p.add_argument("--output", default="target/writer-ui", help="where grabs, receipts and app data go")
    p.add_argument("--appearance", choices=["light", "dark", "both"], default="both")
    p.add_argument("--size", default="1100x760", help="card-host window size (Writer is a desktop app)")
    p.add_argument("--port", type=int, default=8913)
    args = p.parse_args()
    bundle = scratch_bundle(args.output)
    ok = True
    for appearance in (["light", "dark"] if args.appearance == "both" else [args.appearance]):
        journey = Journey(args, appearance, bundle)
        try:
            journey.run()
        finally:
            journey.close_after_failure()
            print("receipt", journey.receipt(), flush=True)
        for run in journey.runs:
            if run["exit_code"] != 0 or run["script_errors"]:
                ok = False
                print("FAILED", appearance, run, file=sys.stderr)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Quick Deck in App Hub's card-host: every screen and state, light and dark.

    python3 apps/quickdeck/tests/ui.py --card-host <App Hub>/target/release/card-host
        [--output target/quickdeck-ui] [--appearance light|dark|both] [--port 8912] [--runs A B ...]

card-host serves no host services, so there is no `deck` engine there. Run A
uses the shipped bundle as it is and shows the engine-unavailable state. The
other runs use a scratch copy of the bundle in which deck_call() is replaced
by the dev fixture's stand-in engine (`dev-fixture/engine.splash`), with the
fixture's settings and pictures copied into the app's storage as `dev/`. The
shipped bundle carries no fixture code.

Each run starts its own hidden card-host on 127.0.0.1:<port> with fresh app
data (MAKEPAD_HIDE_WINDOWS=1, MAKEPAD_REMOTE), drives it over the remote
bridge (/snap, /click, /t, /k, /m), saves grabs (/g) into
<output>/<appearance>/, ends with /gq, and checks that the process exited
and that its log holds no script error. It refuses to start while something
else answers on the port.
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

OUTLINE = [
    ("Northern Lights", ["A short tour of the aurora", "Where it comes from and how to photograph it"]),
    ("What we will cover", ["The science: solar wind meets the magnetosphere", "Where and when to see it", "Photographing the aurora", "Safety and etiquette in the field"]),
    ("The science", ["Charged particles stream from the Sun", "Earth's magnetic field funnels them to the poles", "Oxygen glows green and red; nitrogen adds blue and purple"]),
    ("Best months to look", ["September to March, on clear dark nights", "High latitudes: Tromso, Fairbanks, Yellowknife", "Check the Kp index and the cloud forecast"]),
    ("Field checklist", ["Tripod and spare batteries", "Headlamp with a red mode", "Warm layers and a thermos", "Leave no trace"]),
]


# ---- the remote bridge -------------------------------------------------------

class Remote:
    def __init__(self, port):
        self.base = f"http://127.0.0.1:{port}"

    def get(self, path, **params):
        url = self.base + path + ("?" + urllib.parse.urlencode(params) if params else "")
        try:
            with urllib.request.urlopen(url, timeout=60) as r:
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
            # /snap text can hold raw control characters.
            return json.loads(data, strict=False)
        except ValueError:
            return data.decode("utf-8", "replace")

    def up(self):
        try:
            self.get("/s")
            return True
        except (OSError, RuntimeError):
            return False

    def widgets(self):
        for _ in range(120):
            try:
                snap = self.get("/snap")
                break
            except RuntimeError as e:
                if "app busy" not in str(e):
                    raise
                time.sleep(1.0)
        else:
            raise RuntimeError("the app stayed busy for two minutes")
        return [w for w in snap.get("s", []) if w.get("ty") != "Splash"]

    def find(self, text, ty=None, exact=True, nth=0):
        hits = [w for w in self.widgets() if ((w.get("t") or "") == text if exact else text in (w.get("t") or "")) and (ty is None or w["ty"] == ty)]
        if len(hits) <= nth:
            have = [w.get("t") for w in self.widgets() if w.get("t")][:40]
            raise LookupError(f"no widget {text!r} ({ty}); have {have}")
        return hits[nth]

    def tap(self, text, ty=None, exact=True, nth=0):
        x, y, w, h = self.find(text, ty, exact, nth)["r"]
        return self.get("/click", x=round(x + w / 2, 1), y=round(y + h / 2, 1), wait=1)

    def type_text(self, text):
        return self.get("/t", t=text, wait=1)

    def key(self, code):
        # down, then up: the shell's remote takes no "press".
        self.get("/k", k="down", c=code)
        return self.get("/k", k="up", c=code, wait=1)

    def scroll(self, dy):
        self.get("/m", k="scroll", x=200, y=500, dy=dy, wait=1)
        time.sleep(0.25)

    def wait_for(self, text, ty=None, exact=True, timeout=10.0):
        end = time.time() + timeout
        while time.time() < end:
            try:
                return self.find(text, ty, exact)
            except LookupError:
                time.sleep(0.2)
        raise LookupError(f"timed out waiting for {text!r}")


# ---- one appearance ----------------------------------------------------------

class Journey:
    def __init__(self, args, appearance, dev_bundle):
        self.args = args
        self.appearance = appearance
        self.dev_bundle = dev_bundle
        self.remote = Remote(args.port)
        self.out = Path(args.output) / appearance
        self.data_root = Path(args.output) / "data"
        self.out.mkdir(parents=True, exist_ok=True)
        self.data_root.mkdir(parents=True, exist_ok=True)
        self.proc = None
        self.report = []

    def data(self, name, settings=None, fresh=True):
        data = self.data_root / f"{self.appearance}-{name}"
        if fresh:
            shutil.rmtree(data, ignore_errors=True)
        (data / "os.quickdeck").mkdir(parents=True, exist_ok=True)
        if settings is not None:
            dev = data / "os.quickdeck" / "dev"
            shutil.rmtree(dev, ignore_errors=True)
            shutil.copytree(FIXTURE, dev, ignore=shutil.ignore_patterns("engine.splash"))
            base = json.loads((dev / "fixture.json").read_text())
            base.update(settings)
            (dev / "fixture.json").write_text(json.dumps(base))
        return data

    def launch(self, data, bundle, *extra):
        if self.remote.up():
            raise SystemExit(f"something already answers on port {self.args.port}")
        env = dict(os.environ, MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=f"127.0.0.1:{self.args.port}",
                   MAKEPAD_WIDGET_STYLE="macos-dark" if self.appearance == "dark" else "macos")
        log = open(f"{data}.log", "w")
        self.proc = subprocess.Popen([self.args.card_host, "--bundle", str(bundle), "--system", "--app-data", str(data), *extra],
                                     env=env, stdout=log, stderr=subprocess.STDOUT)
        end = time.time() + 60
        while not self.remote.up():
            if self.proc.poll() is not None:
                raise RuntimeError(f"card-host exited early; see {data}.log")
            if time.time() > end:
                raise RuntimeError("card-host's remote never answered")
            time.sleep(0.5)
        self.remote.wait_for("Quick Deck", "Label", timeout=15)
        time.sleep(0.6)

    def finish(self, data):
        reply = self.remote.get("/gq")
        try:
            self.proc.wait(timeout=15)
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"card-host {self.proc.pid} did not exit")
        log = Path(f"{data}.log").read_text(errors="replace").splitlines()
        errors = [line for line in log if "[E]" in line or "splash:" in line or "callback error" in line]
        self.report.append({"run": data.name, "pid": self.proc.pid, "gq": reply.get("quit") if isinstance(reply, dict) else None,
                            "exited": True, "script_errors": errors})
        self.proc = None

    def close_after_failure(self):
        if self.proc is None or self.proc.poll() is not None:
            return
        try:
            self.remote.get("/quit")
            self.proc.wait(timeout=15)
        except Exception:
            self.proc.terminate()
            self.proc.wait(timeout=15)

    def grab(self, name, settle=0.5, tries=5):
        """A hidden window sometimes refuses a present: ask again."""
        time.sleep(settle)
        for _ in range(tries):
            reply = self.remote.get("/g")
            if isinstance(reply, dict) and "png" in reply:
                shutil.copyfile(reply["png"], self.out / f"{name}.png")
                print(f"{self.appearance}/{name}.png", flush=True)
                return
            time.sleep(0.4)
        raise RuntimeError(f"grab {name} failed {tries} times")

    def tap_until(self, text, ty, expect, expect_ty=None, nth=0, tries=3, timeout=4.0):
        """Tap, then wait for the next state; a hidden window's click is
        sometimes lost, so tap again if it was."""
        for _ in range(tries):
            self.remote.tap(text, ty, nth=nth)
            try:
                return self.remote.wait_for(expect, expect_ty, timeout=timeout)
            except LookupError:
                pass
        raise LookupError(f"{text!r} never led to {expect!r}")

    def type_points(self, points):
        for i, line in enumerate(points):
            if i:
                self.remote.key("ReturnKey")
            self.remote.type_text(line)

    def fill_outline(self, grab_first=None):
        r = self.remote
        title, points = OUTLINE[0]
        r.tap("Deck title, e.g. Northern Lights", "TextInput")
        r.type_text(title)
        r.tap("Points, one per line (optional)", "TextInput")
        self.type_points(points)
        if grab_first:
            self.grab(grab_first)
        for title, points in OUTLINE[1:]:
            r.scroll(3000)
            r.tap("+ Add slide", "Button")
            time.sleep(0.3)
            r.scroll(3000)
            r.tap("Slide title", "TextInput")
            r.type_text(title)
            r.tap("Points, one per line", "TextInput")
            self.type_points(points)

    def generate(self):
        self.remote.scroll(3000)
        self.tap_until("Generate deck", "Button", "STEP 2 OF 4 · GENERATE")
        self.remote.wait_for("STEP 3 OF 4 · REVIEW", timeout=20)

    # -- the runs --

    def run_a(self):
        """The shipped bundle, no engine: empty, typing, the refusal, a draft."""
        r = self.remote
        data = self.data("A")
        self.launch(data, APP / "bundle")
        self.grab("01-decks-empty")
        self.tap_until("New deck", "Button", "STEP 1 OF 4 · OUTLINE")
        self.grab("02-outline-empty")
        self.fill_outline(grab_first="03-outline-first-slide")
        r.scroll(3000)
        self.grab("04-outline-filled")
        r.tap("Generate deck", "Button")
        r.wait_for("Try again", "Button", timeout=10)
        self.grab("05-generate-refused")
        r.tap("Edit outline", "Button")
        r.wait_for("STEP 1 OF 4 · OUTLINE")
        r.tap("‹ Decks", "Button")
        r.wait_for("YOUR DECKS")
        self.grab("06-decks-draft")
        self.finish(data)

    def run_b(self):
        """The stand-in engine: progress, review, slide view, export, a restart, delete."""
        r = self.remote
        data = self.data("B", {})
        self.launch(data, self.dev_bundle)
        self.tap_until("New deck", "Button", "STEP 1 OF 4 · OUTLINE")
        self.fill_outline()
        r.scroll(3000)
        r.tap("Generate deck", "Button")
        r.wait_for("STEP 2 OF 4 · GENERATE")
        time.sleep(1.0)
        self.grab("07-generate-progress", settle=0)
        r.wait_for("STEP 3 OF 4 · REVIEW", timeout=20)
        self.grab("08-review")
        r.tap("3", "Label")
        r.wait_for("3 of 5", "Label")
        self.grab("09-slide-view", settle=0.8)
        r.tap("Next", "Button")
        r.wait_for("4 of 5", "Label")
        r.tap("‹ All slides", "Button")
        r.wait_for("STEP 3 OF 4 · REVIEW")
        self.tap_until("Export", "Button", "STEP 4 OF 4 · EXPORT")
        self.grab("10-export")
        r.tap("Export as PowerPoint", "Button")
        r.wait_for("Saved in Quick Deck's storage.", "Label")
        r.tap("PDF", "Label")
        r.tap("Export as PDF", "Button")
        time.sleep(1.0)
        self.grab("11-exported")
        r.tap("Done", "Button")
        r.wait_for("YOUR DECKS")
        self.grab("12-decks-made")
        self.finish(data)
        kept = self.data_root / f"{self.appearance}-B-kept"
        shutil.rmtree(kept, ignore_errors=True)
        shutil.copytree(data, kept)
        # A restart on the same storage: the deck, its pictures and exports are back.
        self.launch(self.data("B", {}, fresh=False), self.dev_bundle)
        self.grab("13-restart-decks")
        self.tap_until("Northern Lights", "Label", "STEP 3 OF 4 · REVIEW")
        r.tap("Delete deck", "Button")
        self.grab("14-delete-confirm")
        r.tap("Tap again to delete “Northern Lights”", "Button")
        r.wait_for("Turn an outline into slides", "Label")
        self.grab("15-decks-after-delete")
        self.finish(data)

    def run_c(self):
        """The engine answers but leaves no picture: each slide shows its text."""
        r = self.remote
        data = self.data("C", {"unreadable": True})
        self.launch(data, self.dev_bundle)
        self.tap_until("New deck", "Button", "STEP 1 OF 4 · OUTLINE")
        self.fill_outline()
        self.generate()
        self.grab("16-review-text-standins", settle=0.8)
        r.tap("1", "Label")
        r.wait_for("1 of 5", "Label")
        self.grab("17-slide-text-standin", settle=0.8)
        self.finish(data)

    def run_d(self):
        """Validation, then a run where the engine draws no slide."""
        r = self.remote
        data = self.data("D", {"fail": "render"})
        self.launch(data, self.dev_bundle)
        self.tap_until("New deck", "Button", "STEP 1 OF 4 · OUTLINE")
        self.fill_outline()
        r.scroll(3000)
        r.tap("+ Add slide", "Button")
        time.sleep(0.3)
        r.scroll(3000)
        r.tap("Points, one per line", "TextInput")
        r.type_text("A point without a slide title")
        r.scroll(3000)
        r.tap("Generate deck", "Button")
        r.wait_for("Slide 6 has points but no title.", "Label")
        self.grab("18-outline-validation")
        r.tap("Remove", "Button", nth=-1)
        self.generate()
        self.grab("19-review-render-failed", settle=0.8)
        self.finish(data)

    def run_e(self):
        """An export the engine refuses."""
        r = self.remote
        data = self.data("E", {"fail": "convert"})
        self.launch(data, self.dev_bundle)
        self.tap_until("New deck", "Button", "STEP 1 OF 4 · OUTLINE")
        self.fill_outline()
        self.generate()
        self.tap_until("Export", "Button", "STEP 4 OF 4 · EXPORT")
        r.tap("Export as PowerPoint", "Button")
        r.wait_for("Couldn't export: deck.convert: the dev fixture refuses this call", "Label")
        self.grab("20-export-failed")
        self.finish(data)

    def run_g(self):
        """A desktop-sized window (card-host calls no on_app_resize)."""
        r = self.remote
        kept = self.data_root / f"{self.appearance}-B-kept"
        data = self.data_root / f"{self.appearance}-G"
        shutil.rmtree(data, ignore_errors=True)
        shutil.copytree(kept, data)
        self.launch(data, self.dev_bundle, "--size", "1024x720")
        self.grab("21-desktop-decks")
        self.tap_until("Northern Lights", "Label", "STEP 3 OF 4 · REVIEW")
        self.grab("22-desktop-review", settle=0.8)
        r.tap("2", "Label")
        r.wait_for("2 of 5", "Label")
        self.grab("23-desktop-slide", settle=0.8)
        self.finish(data)

    def run(self, names):
        runs = {"A": self.run_a, "B": self.run_b, "C": self.run_c, "D": self.run_d, "E": self.run_e, "G": self.run_g}
        try:
            for name in names:
                runs[name]()
        except BaseException:
            self.close_after_failure()
            raise
        (self.out / "receipt.json").write_text(json.dumps(self.report, indent=2))
        return all(not r["script_errors"] for r in self.report)


def dev_bundle(output):
    """A scratch copy of the bundle with the stand-in engine in place of deck_call()."""
    dst = Path(output) / "bundle"
    shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(APP / "bundle", dst)
    main = dst / "main.splash"
    text = main.read_text()
    start = text.index("fn deck_call(")
    depth, end = 0, None
    for i in range(text.index("{", start), len(text)):
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                end = i + 1
                break
    if end is None or 'host.request("deck."' not in text[start:end] or text.count("fn deck_call(") != 1:
        raise SystemExit("deck_call() in main.splash is not what this script replaces: update dev_bundle()")
    main.write_text(text[:start] + (FIXTURE / "engine.splash").read_text().rstrip() + text[end:])
    return dst


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--card-host", required=True, help="App Hub's card-host binary")
    p.add_argument("--output", default="target/quickdeck-ui", help="where grabs, receipts and app data go")
    p.add_argument("--appearance", choices=["light", "dark", "both"], default="both")
    p.add_argument("--port", type=int, default=8912)
    p.add_argument("--runs", nargs="+", default=["A", "B", "C", "D", "E", "G"], choices=["A", "B", "C", "D", "E", "G"])
    args = p.parse_args()
    args.output = str(Path(args.output).resolve())
    bundle = dev_bundle(args.output)
    ok = True
    for appearance in (["light", "dark"] if args.appearance == "both" else [args.appearance]):
        ok &= Journey(args, appearance, bundle).run(args.runs)
    if not ok:
        print("script errors in a card-host log: see the receipts", file=sys.stderr)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

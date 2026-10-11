#!/usr/bin/env python3
"""PDF Tools end to end: the real pdf engine, in a hidden OctoSense desktop shell.

The shell is the one this checkout builds (`cargo build --locked -p octosense`).
For each run the driver makes a fresh OctoSense home, writes sample PDFs into
PDF Tools' storage there (octosense-pdf-service's `pdftools_fixture` example),
starts the shell hidden from a Terminal.app tab (never as a child of this
process), opens the app with MAKEPAD_WM_TEST_APP and walks it over Makepad's
remote bridge, saving original /g grabs. Every engine call is real.

  shell    the samples in light, then dark through the shell's own style menu
  restart  the same home again, with a PDF placed in the library between the
           runs as files.import leaves one (`Imported PDF.pdf`): the path an
           opened file takes, without the host's dialog
  full     storage with no room left: the engine's refusal, then a removal
  empty    no PDFs: the empty library and "Open a PDF from this device". A
           hidden window never has focus, so the files service refuses the
           dialog before it opens, and the app shows that refusal
  missing  App Hub's card-host, which serves no host services: no engine and
           no files service

Shell runs hold a lock folder, so one hidden shell runs on this machine at a
time, and refuse to start while another OctoSense runs.

  cargo build --locked -p octosense
  python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
  python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --only missing --output target/pdftools-ui
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[3]
BUNDLE = ROOT / "apps/pdftools/bundle"
APP = "os.pdftools"
LIBRARY = "accounts/device/library"
# The storage PDF Tools gets: App Hub's system ceiling (no storage.max_bytes).
QUOTA = 64 * 1024 * 1024
ICON_UP = ""


def fixture(*args):
    """octosense-pdf-service's pdftools_fixture example."""
    subprocess.run(["cargo", "run", "--locked", "-q", "-p", "octosense-pdf-service", "--example", "pdftools_fixture",
                    "--", *map(str, args)], cwd=ROOT, check=True)


def port_free(port):
    with socket.socket() as probe:
        return probe.connect_ex(("127.0.0.1", port)) != 0


def octosense_running():
    """A running OctoSense: the shell's own binary, or the packaged app."""
    found = []
    for pattern in (["-x", "octosense"], ["-f", "OctoSense.app/Contents/MacOS"]):
        result = subprocess.run(["pgrep", *pattern], capture_output=True, text=True)
        found += result.stdout.split()
    return found


class Driver:
    """The remote bridge, and the app inside whatever host answers it."""

    scale = 1.0

    def __init__(self, port):
        self.port = port

    def remote(self, route, timeout=20, **query):
        url = f"http://127.0.0.1:{self.port}/{route}"
        if query:
            url += "?" + urlencode(query)
        # The engine works on the UI thread (#399): while it renders, no frame
        # is presented and the bridge asks to try again.
        for attempt in range(60):
            try:
                with urlopen(url, timeout=timeout) as response:
                    data = response.read()
                break
            except HTTPError as error:
                body = error.read().decode(errors="replace")
                if error.code == 404 and "retry" in body and attempt < 59:
                    # The bridge applies input before it presents the frame,
                    # and says so: a grab is the frame barrier, input is
                    # never sent twice.
                    if route in ("click", "k", "t", "m"):
                        self.remote("g")
                        return {"ok": 1}
                    time.sleep(0.5)
                    continue
                raise AssertionError(f"/{route}: HTTP {error.code}: {body}") from error
        try:
            return json.loads(data)
        except ValueError:
            return data

    def size(self):
        return self.remote("s")["w"][0]["sz"]

    def ready(self, timeout=90.0):
        """Wait until the bridge answers."""
        deadline = time.monotonic() + timeout
        while True:
            try:
                return self.remote("s", timeout=5)
            except (OSError, URLError, AssertionError):
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.5)

    def area(self, timeout=60.0):
        """The app's surface: its window in a shell, the whole window in card-host."""
        deadline = time.monotonic() + timeout
        while True:
            try:
                snap = self.remote("snap")["s"]
                break
            except (OSError, URLError, AssertionError):
                # A busy UI thread (an engine call, a first load) answers late.
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.5)
        views = [w["r"] for w in snap if w["ty"] == "MpModuleView"]
        if views:
            return max(views, key=lambda r: r[2] * r[3])
        return next(w["r"] for w in snap if w["ty"] == "Window")

    def widgets(self):
        return [w for w in self.remote("snap")["s"] if w["ty"] != "Splash" and w["r"][2] > 0 and w["r"][3] > 0]

    def find(self, text, kind=None, below=0.0, timeout=30.0, exact=True):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                found = [w for w in self.widgets()
                         if (w.get("t") == text if exact else text in (w.get("t") or ""))
                         and (kind is None or w["ty"] == kind) and w["r"][1] >= below]
                if found:
                    buttons = [w for w in found if w["ty"] in ("Button", "ButtonFlat")]
                    return (buttons or found)[0]
            except (OSError, ValueError, AssertionError):
                pass
            time.sleep(0.2)
        raise AssertionError(f"no visible {kind or 'widget'} {text!r} below y={below}")

    def gone(self, text, timeout=30.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if not any(w.get("t") == text for w in self.widgets()):
                return
            time.sleep(0.2)
        raise AssertionError(f"{text!r} is still on screen")

    def images(self, count, timeout=60.0):
        """Wait for at least `count` pictures: the engine's page renders."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if sum(1 for w in self.widgets() if w["ty"] == "Image") >= count:
                time.sleep(0.4)
                return
            time.sleep(0.3)
        raise AssertionError(f"fewer than {count} page pictures after {timeout} s")

    def click(self, x, y):
        # Never outside the app: in a shell that is the desk, the dock or another window.
        ax, ay, aw, ah = self.area()
        assert ax <= x <= ax + aw and ay <= y <= ay + ah, f"({x}, {y}) is outside the app's window {[ax, ay, aw, ah]}"
        self.remote("click", x=x, y=y, wait=1)
        time.sleep(0.4)

    def tap(self, text, kind=None, below=0.0, exact=True):
        x, y, w, h = self.find(text, kind, below, exact=exact)["r"]
        self.click(x + w / 2, y + h / 2)

    def tap_above(self, text, rise, below=0.0):
        """Tap `rise` points above a label: a page tile above its number."""
        x, y, w, h = self.find(text, "Label", below)["r"]
        self.click(x + w / 2, y - rise)

    def reveal(self, text, kind="Label", tries=12, exact=True):
        """Scroll until `text` is drawn below the screen's header and clear of
        its bottom bar, or as far as the screen scrolls: down first when it is
        not drawn at all, then back up."""
        ax, ay, aw, ah = self.area()
        top = ay + 170
        # The screen's bottom bar, when it has one: its buttons sit in the
        # last 90 points of the app.
        bar = [w["r"][1] for w in self.widgets()
               if w["ty"] in ("Button", "ButtonFlat") and w["r"][1] > ay + ah - 90]
        floor = min(bar) - 4 if bar else ay + ah - 4
        step, last = 240, None
        for attempt in range(tries):
            try:
                found = self.find(text, kind, timeout=1.0, exact=exact)
            except AssertionError:
                if attempt == tries // 2:
                    step = -step
                self.scroll(step)
                last = None
                continue
            x, y, w, h = found["r"]
            if found["ty"] in ("Button", "ButtonFlat") and y > ay + ah - 90:
                return  # the bottom bar itself
            # A row the pane cuts off is reported only as tall as what shows.
            if top <= y and y + h <= floor and h >= 12:
                return
            if last is not None and abs(last - y) < 1:
                return  # the screen scrolls no further: this is where it shows
            last = y
            self.scroll(-240 if y < top else 240)
        raise AssertionError(f"could not scroll {text!r} into view")

    def see(self, text, kind="Label", exact=True, timeout=60.0):
        """Wait until `text` is on screen, scrolling to it: what the engine
        answers can take a moment, and a long screen hides its end."""
        deadline = time.monotonic() + timeout
        while True:
            try:
                return self.reveal(text, kind, tries=6, exact=exact)
            except AssertionError:
                if time.monotonic() > deadline:
                    raise

    def press(self, text, kind="Label"):
        """Tap a row once it is on screen."""
        self.see(text, kind)
        self.tap(text, kind)

    def scroll(self, dy):
        x, y, w, h = self.area()
        self.remote("m", k="scroll", x=x + w / 2, y=y + h / 2, dy=dy, wait=1)
        time.sleep(0.4)

    def type(self, text):
        self.remote("t", t=text, wait=1)
        time.sleep(0.5)

    def key(self, code, **mods):
        self.remote("k", c=code, wait=1, **mods)
        time.sleep(0.25)

    def grab(self, out, name):
        time.sleep(0.8)
        self.remote("g")  # a frame barrier: the next grab is of what is on screen now
        path = out / f"{name}.png"
        path.write_bytes(self.remote("g", raw=1, scale=self.scale))
        # Where the app was in the frame, in points.
        path.with_suffix(".json").write_text(json.dumps({"area": self.area(), "window": self.size()}))
        print("  ", path)


class Shell(Driver):
    """The desktop shell, started hidden from a Terminal.app tab."""

    def __init__(self, binary, home, port, log):
        super().__init__(port)
        self.log = log
        env = {
            "OCTOSENSE_HOME": home,
            # Its own kernel folder: a home of OctoSense's own would otherwise
            # copy the person's provider settings from ~/octos-home once.
            "OCTOS_APP_CORE_DIR": home / "octos-home" / ".octos",
            "MAKEPAD_HOME": home / "makepad",
            "MAKEPAD_HIDE_WINDOWS": "1",
            "MAKEPAD_REMOTE": f"127.0.0.1:{port}",
            "MAKEPAD_WM_TEST_APP": "pdftools",
        }
        command = "cd {} && env {} {} > {} 2>&1; exit".format(
            shlex.quote(str(ROOT)), " ".join(f"{k}={shlex.quote(str(v))}" for k, v in env.items()),
            shlex.quote(str(binary)), shlex.quote(str(log)))
        script = 'tell application "Terminal" to do script "{}"'.format(command.replace("\\", "\\\\").replace('"', '\\"'))
        answer = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, check=True).stdout
        found = re.search(r"window id (\d+)", answer)
        self.window = found[1] if found else None
        self.pid = None
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            text = log.read_text(errors="replace") if log.exists() else ""
            listening = re.search(r"listening on 127\.0\.0\.1:(\d+) pid=(\d+)", text)
            if listening:
                self.pid = int(listening[2])
                break
            time.sleep(0.5)
        assert self.pid, f"the shell did not start its remote bridge; see {log}"
        self.ready()

    def alive(self):
        try:
            os.kill(self.pid, 0)
            return True
        except ProcessLookupError:
            return False

    def logged(self, needle, since=0, timeout=30.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if needle in self.log.read_text(errors="replace")[since:]:
                return
            time.sleep(0.3)
        raise AssertionError(f"the shell never logged {needle!r}")

    def menu(self, words):
        """The shell's menu (⌘Space), filtered by typing, then Return."""
        self.key("Space", cmd=1)
        for char in words.lower():
            self.key("Space" if char == " " else "Key" + char.upper())
        self.key("Return")

    def style(self, name, words):
        since = len(self.log.read_text(errors="replace"))
        self.menu(words)
        self.logged(f"wm: desktop style {name} applied", since)
        time.sleep(1.5)

    def quit(self):
        try:
            self.remote("quit", timeout=10)
        except (OSError, AssertionError):
            pass
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline and self.alive():
            time.sleep(0.5)
        exited = not self.alive()
        if self.window:
            subprocess.run(["osascript", "-e", f'tell application "Terminal" to close window id {self.window}'],
                           capture_output=True)
        assert exited, f"the shell (pid {self.pid}) did not exit after /quit"


class CardHost(Driver):
    def __init__(self, binary, data, port, style, log):
        super().__init__(port)
        env = os.environ.copy()
        env.update(MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=f"127.0.0.1:{port}", MAKEPAD_WIDGET_STYLE=style)
        self.log = log
        self.handle = log.open("w")
        self.process = subprocess.Popen([str(binary), "--bundle", str(BUNDLE), "--system", "--app-data", str(data)],
                                        cwd=ROOT, env=env, stdout=self.handle, stderr=self.handle)
        self.ready()

    def quit(self):
        try:
            assert self.remote("gq").get("quit") == 1
        except (OSError, AssertionError, URLError):
            pass
        try:
            self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)
            raise AssertionError("card-host did not exit after /gq")
        finally:
            self.handle.close()


# ---------------------------------------------------------------- journeys

def journey(app, out, dark):
    """Every screen on the samples, with the real engine."""
    p = "d" if dark else ""
    app.see("4 pages  ·  9 KB")
    app.scroll(-3000)
    app.images(3)
    app.see("PDFs up to 64 MB")
    app.grab(out, f"{p}01-library")
    app.tap("Open a PDF from this device", "Label")
    app.see("Couldn't open a PDF from this device")
    app.see("Bring PDF Tools to the front, then choose the file again.")
    app.see("foreground_required", exact=False)
    app.grab(out, f"{p}02-open-from-device")
    app.tap("OK")
    app.gone("Couldn't open a PDF from this device")

    app.press("Field guide")
    app.see("7 pages  ·  16 KB")
    app.see("Split into files", kind=None)
    app.images(5)
    app.grab(out, f"{p}03-pages")
    app.tap_above("2", 120, below=300)
    app.see("Page 2 of 7")
    app.images(1)
    app.grab(out, f"{p}04-page")
    app.tap("Next")
    app.see("Page 3 of 7")
    app.grab(out, f"{p}05-page-next")
    app.tap("‹ Pages")

    app.tap("Text")
    app.see("PAGE 1")
    app.grab(out, f"{p}06-text")
    app.tap("Find in the text", "TextInput")
    app.type("wren")
    app.see("2 pages contain “wren”.")
    app.grab(out, f"{p}07-text-find")
    app.tap("Info")
    app.see("A4, 210 × 297 mm")
    app.grab(out, f"{p}08-info")
    app.see("Helvetica, Helvetica-Bold, Times-Italic, Times-Roman")
    app.see("Not protected")
    app.grab(out, f"{p}08b-info-end")

    app.tap("Pages")
    app.tap("Split into files")
    app.see("Makes 4 files")
    if dark:
        # The light run left Field guide-part2: this split replaces it.
        app.see("Replaces 1 file of the same name in your library.")
    app.grab(out, f"{p}09-split-every")
    app.scroll(-3000)
    app.tap("Where I choose")
    app.see("Tap a page to split there", kind=None)
    app.tap_above("2", 120, below=300)
    app.see("File 2 starts here")
    app.see("Split into 2 files", kind=None)
    app.grab(out, f"{p}10-split-where")
    if dark:
        app.scroll(-3000)
        app.tap("Every few pages")
        app.tap("Split into 4 files")
        app.see("Split into 4 files")
        app.see("Page 7")
    else:
        app.tap("Split into 2 files")
        app.see("Split into 2 files")
        app.see("Pages 2–7")
    app.scroll(-3000)
    app.grab(out, f"{p}11-split-done")
    app.tap("Done")
    app.see("Field guide-part2")

    first, second, name = ("Apartment lease", "Board minutes", "Merged 2") if dark else ("Quarterly report", "Board minutes", "Merged")
    app.tap("Merge PDFs")
    if not dark:
        app.press("Damaged scan")
        app.see("“Damaged scan” can't be opened, so it can't be merged.")
        app.gone("“Damaged scan” can't be opened, so it can't be merged.", timeout=20)
    app.press(first)
    app.press(second)
    app.see("Next: order 2 files", kind=None)
    app.grab(out, f"{p}12-merge-choose")
    app.tap("Next: order 2 files")
    app.see(f"One PDF of {'4' if dark else '6'} pages, saved in your library.")
    app.grab(out, f"{p}13-merge-order")
    app.scroll(-3000)
    board = app.find("Board minutes", "Label")["r"]
    up = [w for w in app.widgets() if w.get("t") == ICON_UP and abs(w["r"][1] - board[1]) < 30]
    assert up, "no move-up button on the Board minutes row"
    x, y, w, h = up[0]["r"]
    app.click(x + w / 2, y + h / 2)
    app.tap("Merge 2 PDFs")
    app.see("Merged into one PDF")
    app.see(name)
    app.images(1)
    app.grab(out, f"{p}14-merge-done")
    app.tap("Open")
    app.see(name)
    app.see("Split into files", kind=None)
    app.images(4)
    app.grab(out, f"{p}15-merged-pages")
    app.tap("‹ Library")

    part = "Field guide-part1"
    app.press(part)
    app.tap("Remove")
    app.see("Remove this PDF from PDF Tools?")
    app.see("Keep it", kind=None)
    app.grab(out, f"{p}16-remove-ask")
    app.tap("Remove")
    app.see(f"Removed “{part}” from PDF Tools.")
    app.gone(part)
    app.grab(out, f"{p}17-removed")
    # The note goes by itself after 8 s; a tap could land on the row under it.
    app.gone(f"Removed “{part}” from PDF Tools.", timeout=20)

    app.press("Damaged scan")
    app.see("Couldn't open this PDF")
    app.grab(out, f"{p}18-damaged")
    app.tap("‹ Library")


def restart(app, out):
    """The same storage after a restart, plus a PDF placed as an import leaves one."""
    app.see("Merged")
    app.see("Merged 2")
    app.see("Imported PDF")
    # Read by the engine on this launch: the index never knew it.
    app.see("3 pages  ·  ", exact=False)
    app.images(4)
    app.grab(out, "r01-library")
    app.press("Imported PDF")
    app.see("Split into files", kind=None)
    app.images(3)
    app.grab(out, "r02-imported-pages")
    app.tap("Text")
    app.see("Garden plan", exact=False)
    app.grab(out, "r03-imported-text")
    app.tap("Pages")
    app.tap_above("1", 120, below=300)
    app.see("Page 1 of 3")
    app.images(1)
    app.grab(out, "r04-imported-page")
    app.tap("‹ Pages")
    app.tap("‹ Library")


def full(app, out):
    """No room left: the engine refuses to write, and the app says so."""
    app.see("PDF Tools' storage is full")
    app.grab(out, "f01-library-full")
    app.tap("Merge PDFs")
    app.press("Board minutes")
    app.press("Apartment lease")
    app.tap("Next: order 2 files")
    app.tap("Merge 2 PDFs")
    app.see("Couldn't merge: There isn't room for it in PDF Tools' storage. Remove a PDF you no longer need, then try again.")
    app.grab(out, "f02-merge-full")
    app.tap("Cancel")
    app.press("Apartment lease")
    app.tap("Remove")
    app.tap("Remove")
    # The library tries the first pages again at once, on the UI thread
    # (#399), so the 8 s note may be gone before the bridge answers.
    app.see("4 PDFs")
    app.gone("Apartment lease")
    app.grab(out, "f03-removed")


def empty(app, out, dark=False):
    p = "d" if dark else ""
    app.see("No PDFs here yet")
    app.see("PDFs up to 64 MB")
    app.grab(out, f"{p}e01-empty")
    app.tap("Open a PDF from this device")
    app.see("Bring PDF Tools to the front, then choose the file again.")
    app.grab(out, f"{p}e02-empty-open")
    app.tap("OK")


def missing(app, out):
    app.see("This device has no PDF engine")
    app.see("Not available here")
    app.grab(out, "m01-missing-library")
    app.press("Field guide")
    app.see("This device has no PDF engine")
    app.grab(out, "m02-missing-document")
    app.tap("‹ Library")
    app.tap("Merge PDFs")
    app.see("This device has no PDF engine", exact=True)
    app.grab(out, "m03-missing-merge")


# ---------------------------------------------------------------- runs

def check_log(log):
    errors = [line for line in log.read_text(errors="replace").splitlines()
              if "[E]" in line and ("splash" in line.lower() or "script" in line.lower() or "pdftools" in line.lower())
              or "on_render closure failed" in line or "callback error" in line or "script time budget exceeded" in line]
    assert not errors, f"script errors in {log}:\n" + "\n".join(errors)


def filler(jail, room):
    """Fill `jail` to within `room` bytes of the quota."""
    used = sum(f.stat().st_size for f in jail.rglob("*") if f.is_file())
    blob = jail / "cache/filler.bin"
    blob.parent.mkdir(parents=True, exist_ok=True)
    with blob.open("wb") as file:
        file.truncate(QUOTA - used - room)


class Lock:
    """One hidden shell on this machine at a time: a folder, made atomically."""

    def __init__(self, path):
        self.path = path

    def __enter__(self):
        while True:
            try:
                self.path.mkdir()
                return self
            except FileExistsError:
                print(f"waiting for {self.path} (another shell run holds it)", flush=True)
                time.sleep(20)

    def __exit__(self, *exc):
        self.path.rmdir()


def shell_runs(args, runs):
    binary = args.shell.resolve()
    assert binary.is_file(), f"build the shell first: {binary}"
    scratch = Path(tempfile.mkdtemp(prefix="pdftools-shell-", dir=args.work))
    print("homes in", scratch)

    def session(name, home, steps):
        # Never beside another OctoSense: a person's, or another run's that
        # takes no lock. Wait for it a while, then give up.
        deadline = time.monotonic() + 1800
        while octosense_running():
            assert time.monotonic() < deadline, "another OctoSense is still running: quit it first"
            print("waiting for another OctoSense to quit", flush=True)
            time.sleep(20)
        assert port_free(args.port), f"port {args.port} is taken"
        out = args.output / name
        out.mkdir(parents=True, exist_ok=True)
        log = out / "shell.log"
        app = Shell(binary, home, args.port, log)
        try:
            app.find("PDF Tools", "Label", timeout=90)
            # Where things are, for whoever reads the grabs: the window's widgets.
            (out / "snap.json").write_text(json.dumps(app.remote("snap"), indent=1))
            steps(app, out)
        finally:
            app.quit()
        check_log(log)

    with Lock(args.lock):
        if "shell" in runs or "restart" in runs:
            home = scratch / "home"
            fixture(home / "apps")

            def light_then_dark(app, out):
                journey(app, out, dark=False)
                app.style("octosense-dark", "octosense dark")
                journey(app, out, dark=True)
            session("shell", home, light_then_dark)
            if "restart" in runs:
                fixture("--import-sample", home / "apps" / APP / LIBRARY / "Imported PDF.pdf")
                session("restart", home, restart)
        if "full" in runs:
            home = scratch / "full"
            fixture(home / "apps")
            filler(home / "apps" / APP, 8 * 1024)
            session("full", home, full)
        if "empty" in runs:
            home = scratch / "empty"

            def both(app, out):
                empty(app, out)
                app.style("octosense-dark", "octosense dark")
                empty(app, out, dark=True)
            session("empty", home, both)


def card_host_runs(args, runs):
    binary = args.card_host.resolve()
    assert binary.is_file(), f"build App Hub's card-host first: {binary}"
    assert port_free(args.port), f"port {args.port} is taken"
    if "missing" in runs:
        with tempfile.TemporaryDirectory(prefix="pdftools-card-host-", dir=args.work) as scratch:
            data = Path(scratch)
            fixture(data)
            out = args.output / "missing"
            out.mkdir(parents=True, exist_ok=True)
            app = CardHost(binary, data, args.port, "macos", out / "card-host.log")
            try:
                missing(app, out)
            finally:
                app.quit()
            check_log(out / "card-host.log")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--shell", type=Path, help="the desktop shell binary (target/debug/octosense)")
    parser.add_argument("--card-host", type=Path, help="App Hub's card-host, for the run without host services")
    parser.add_argument("--lock", type=Path, default=Path(tempfile.gettempdir()) / "octosense-shell.lock",
                        help="the folder that marks a hidden shell run on this machine")
    parser.add_argument("--port", type=int, default=0, help="the remote bridge's port (8915 for a shell, 8911 for card-host)")
    parser.add_argument("--output", type=Path, default=ROOT / "target/pdftools-ui")
    parser.add_argument("--work", type=Path, help="where the runs' OctoSense homes go (default: the system's temporary folder)")
    parser.add_argument("--only", choices=["shell", "restart", "full", "empty", "missing"], action="append")
    parser.add_argument("--grab-scale", type=float, default=1.0, help="the grabs' scale (the bridge's /g scale)")
    args = parser.parse_args()
    Driver.scale = args.grab_scale
    card_host_only = ("missing",)
    runs = args.only or (["shell", "restart", "full", "empty"] if args.shell else []) + (list(card_host_only) if args.card_host else [])
    assert runs, "give --shell, --card-host or both"
    if any(run not in card_host_only for run in runs):
        assert args.shell, "these runs need --shell"
        args.port = args.port or 8915
        shell_runs(args, runs)
    if any(run in card_host_only for run in runs):
        assert args.card_host, "the missing run needs --card-host"
        args.port = 8911 if args.port in (0, 8915) else args.port
        card_host_runs(args, runs)
    print("all journeys passed; grabs in", args.output)


if __name__ == "__main__":
    main()

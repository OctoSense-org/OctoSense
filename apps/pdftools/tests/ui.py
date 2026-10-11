#!/usr/bin/env python3
"""PDF Tools v2 end to end, in a hidden OctoSense desktop shell and in App Hub's card-host.

Shell runs use the shell this checkout builds, with the real pdf engine
(`cargo build --locked -p octosense --no-default-features --features
app-hub,craft-engines`). For each run the driver makes a fresh OctoSense home,
writes sample PDFs into PDF Tools' storage there (octosense-pdf-service's
`pdftools_fixture` example), starts the shell hidden from a Terminal.app tab
(never as a child of this process), opens the app with MAKEPAD_WM_TEST_APP and
walks it over Makepad's remote bridge, saving original /g grabs. Every engine
call is real.

  shell    every mode on the samples in the shell's own window, light, then
           dark through the shell's own style menu (the dark pass works on
           what the light pass left): Home, "Open a PDF from this device"
           refused, reading and thumbnails, find, a highlight and its reply,
           a rotation and its undo, combine with a page range, initials
           placed by Fill & Sign, a text edit, save, a damaged file
  restart  the same home again: the library, the titles, the page a PDF was
           left at, and a PDF placed as files.import leaves one
           (`Imported PDF.pdf`)
  full     storage with no room left: the engine's refusal, then a removal
  empty    no PDFs: the empty library and "Open a PDF from this device" (a
           hidden window never has focus, so the files service refuses the
           dialog and the app shows that refusal)

card-host runs serve no host services:

  missing  the shipped bundle: no engine and no files service
  fixture  a scratch copy of the bundle with dev-fixture/engine.splash in
           place of engine(), and make_fixture.py's sample documents (the
           approved designs' sample text): every designed screen at the
           designs' 1536 x 1024, light and dark, each grab put beside its
           design in compare/, and the damaged, protected and unsaved states

Shell runs hold a lock folder, so one hidden shell runs on this machine at a
time, and refuse to start while another OctoSense runs.

  python3 apps/pdftools/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/pdftools-ui
  python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/pdftools-ui
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
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[3]
APP_DIR = ROOT / "apps/pdftools"
BUNDLE = APP_DIR / "bundle"
FIXTURE = APP_DIR / "dev-fixture"
DESIGNS = APP_DIR / "design/source"
APP = "os.pdftools"
LIBRARY = "accounts/device/library"
# The storage PDF Tools gets: App Hub's system ceiling (no storage.max_bytes).
QUOTA = 64 * 1024 * 1024


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
    # MAKEPAD_SPLASH_BUDGET_MS for the hosts these runs start, or None for the
    # runtime's 64 ms: the budget is wall-clock time, so a loaded machine can
    # overrun it in any handler (--budget-ms).
    budget_ms = None

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
        """The app's surface: its window in a shell, its Splash view in card-host."""
        deadline = time.monotonic() + timeout
        while True:
            try:
                snap = self.remote("snap")["s"]
                views = [w["r"] for w in snap if w["ty"] == "MpModuleView"]
                if views:
                    return max(views, key=lambda r: r[2] * r[3])
                # card-host: the app's Splash view (older hosts reported a Window).
                roots = [w["r"] for w in snap if w["ty"] in ("Splash", "Window")]
                if roots:
                    return max(roots, key=lambda r: r[2] * r[3])
            except (OSError, URLError, AssertionError):
                pass
            # A busy UI thread (an engine call, a first load) answers late.
            if time.monotonic() > deadline:
                raise AssertionError("the app's surface never appeared in /snap")
            time.sleep(0.5)

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

    def pictures(self):
        """The app's own pictures (the engine's renders), not the shell's
        wallpaper or dock: inside the app's surface and narrower than it."""
        ax, ay, aw, ah = self.area()
        return [w for w in self.widgets() if w["ty"] == "Image" and w["r"][2] < aw - 1
                and ax - 1 <= w["r"][0] <= ax + aw and ay - 1 <= w["r"][1] <= ay + ah]

    def images(self, count, timeout=60.0):
        """Wait for at least `count` pictures: the engine's page renders."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                if len(self.pictures()) >= count:
                    time.sleep(0.4)
                    return
            except (OSError, ValueError, AssertionError):
                pass
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
        # PDF Tools' status line takes the last 44 points of the app.
        floor = ay + ah - 48
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
        if Driver.budget_ms:
            env["MAKEPAD_SPLASH_BUDGET_MS"] = str(Driver.budget_ms)
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
    def __init__(self, binary, bundle, data, port, style, log, size="1536x1024"):
        super().__init__(port)
        env = os.environ.copy()
        env.update(MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=f"127.0.0.1:{port}", MAKEPAD_WIDGET_STYLE=style)
        if Driver.budget_ms:
            env["MAKEPAD_SPLASH_BUDGET_MS"] = str(Driver.budget_ms)
        self.log = log
        self.handle = log.open("w")
        self.process = subprocess.Popen([str(binary), "--bundle", str(bundle), "--system", "--app-data", str(data), "--size", size],
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


# ---------------------------------------------------------------- helpers

def top_page(app):
    """The first page image on the canvas: [x, y, w, h] in points (what
    shows of it: a scroll view reports only its visible part)."""
    imgs = [w for w in app.pictures() if w["r"][2] > 300]
    assert imgs, "no page on the canvas"
    return min(imgs, key=lambda w: w["r"][1])["r"]


def zoom(app):
    """The pill's zoom, as a scale: '90%' is 0.9."""
    for w in app.widgets():
        found = re.fullmatch(r"(\d+)%", w.get("t") or "")
        if found:
            return int(found[1]) / 100
    raise AssertionError("no zoom on the pill")


def click_page(app, px, py, page_width=None):
    """Click at a point of the top page, given in the page's points. The run
    of pages starts at the top of the canvas, so its top left shows."""
    x, y, w, h = top_page(app)
    k = zoom(app)
    app.click(x + px * k, y + py * k)


def pill_next(app):
    """The pill's next page: the first icon button right of 'N / M'."""
    label = next(w for w in app.widgets() if re.fullmatch(r"\d+ / \d+", w.get("t") or ""))
    row, (x, y, w, h) = icon_buttons_near(app, label["t"], "Label")
    bx, by, bw, bh = [r for r in row if r[0] >= x + w - 1][0]
    app.click(bx + bw / 2, by + bh / 2)


def range_fields(app):
    """Combine's page-range fields, top to bottom (not the search field)."""
    ax, ay, aw, ah = app.area()
    return sorted((w for w in app.widgets() if w["ty"] == "TextInput" and w["r"][2] < 200 and w["r"][1] > ay + 110),
                  key=lambda w: w["r"][1])


def press_combine(app):
    """Combine's own button, at the end of its card (not the mode's tab)."""
    app.scroll(600)
    buttons = [w for w in app.widgets() if w["ty"] in ("Button", "ButtonFlat") and w.get("t") == "Combine"]
    x, y, w, h = max(buttons, key=lambda b: b["r"][1])["r"]
    app.click(x + w / 2, y + h / 2)


def icon_buttons_near(app, text, kind="Button"):
    """The text-less (icon) buttons on the same row as a labelled control, left to right."""
    x, y, w, h = app.find(text, kind)["r"]
    row = [b["r"] for b in app.widgets() if b["ty"] in ("Button", "ButtonFlat") and not b.get("t")
           and abs((b["r"][1] + b["r"][3] / 2) - (y + h / 2)) < 12]
    return sorted(row, key=lambda r: r[0]), (x, y, w, h)


def click_icon_left_of(app, text, nth=1, kind="Button"):
    """Click the nth icon button left of a labelled control (1 = the nearest)."""
    row, (x, y, w, h) = icon_buttons_near(app, text, kind)
    left = [r for r in row if r[0] + r[2] <= x + 1]
    bx, by, bw, bh = left[-nth]
    app.click(bx + bw / 2, by + bh / 2)


def tap_tile(app, number, rise=70):
    """Pages mode: tap a page tile above its number."""
    app.tap_above(str(number), rise, below=150)


# The fixture run's storage, for the boxes make_fixture.py recorded.
FIXTURE_JAIL = None


def fixture_paragraph(ident, page, n):
    """The centre of paragraph n of a fixture page, in the page's points."""
    lines = json.loads((FIXTURE_JAIL / "dev/lines" / ident / f"{page}.json").read_text())
    x, y, w, h = lines["paragraphs"][n - 1]["box"]
    return x + w / 2, y + h / 2


def name_once(app):
    """Comment as: the name comments carry, asked once and kept."""
    try:
        app.find("Your name", "TextInput", timeout=3)
    except AssertionError:
        return
    app.tap("Your name", "TextInput")
    app.type("Maya Chen")
    app.tap("Save", "Button", below=150)
    app.gone("Comment as")


# ---------------------------------------------------------------- fixture journey (card-host)

def fixture_journey(app, out, dark):
    """The approved designs' states on the dev fixture, one grab per screen."""
    p = "d" if dark else ""
    app.see("Q3 2026 Board Report.pdf")
    app.images(5)
    # 01-home shows Home with the report open in a tab.
    app.press("Q3 2026 Board Report.pdf")
    app.images(3)
    app.tap("Open", "Button")
    app.scroll(-3000)
    app.images(5)
    app.grab(out, f"{p}01-home")

    # The other screens show five PDFs open: open them in the designs'
    # order, then come back to the first.
    for name in ("Riverside Lease 2026.pdf", "Field Guide to Garden Birds.pdf", "Invoice INV-2041.pdf", "Site Survey Photos.pdf"):
        app.press(name)
        app.images(1)
        app.tap("Open", "Button")
    app.tap("Q3 2026 Board Report.pdf", "Label")
    app.images(3)
    app.tap("3", "Label", below=150)
    app.see("3 / 24")
    app.images(4)
    app.grab(out, f"{p}02-reading")

    app.tap("Search in document", "TextInput")
    app.type("revenue")
    app.key("Return")
    app.see("matches on 7 pages", exact=False)
    click_icon_left_of(app, "Done")  # the down arrow: the second match
    app.see("2 of 11")
    app.images(2)
    app.grab(out, f"{p}03-find")
    app.tap("Done")

    app.tap("Comment", "Button")
    name_once(app)
    app.tap("Outline", "Label")
    app.tap("Revenue", "Label")
    app.tap("Outline", "Label")
    app.see("3 / 24")
    app.images(1)
    app.grab(out, f"{p}04-comment")

    app.tap("Pages", "Button")
    app.images(12)
    tap_tile(app, 5)
    tap_tile(app, 6)
    app.see("2 selected")
    app.grab(out, f"{p}05-pages")
    tap_tile(app, 5)
    tap_tile(app, 6)

    app.tap("Combine", "Button")
    app.tap("Add files", "Label")
    app.tap("Appendix A - Figures.pdf", "Label")
    app.tap("Add files", "Label")
    app.tap("Cover Letter.pdf", "Label")
    ranges = sorted((w for w in app.widgets() if w["ty"] == "TextInput" and w["r"][2] < 200), key=lambda w: w["r"][1])
    x, y, w, h = ranges[1]["r"]
    app.click(x + w / 2, y + h / 2)
    app.type("1-4, 9")
    app.see("(5 pages)")
    app.grab(out, f"{p}06-combine")

    app.tap("Edit", "Button")
    app.tap("Outline", "Label")
    app.tap("Summary", "Label")
    app.tap("Outline", "Label")
    app.see("2 / 24")
    app.images(1)
    click_page(app, *fixture_paragraph("board", 2, 2), 612)  # "The board met on 21 September 2026…"
    app.see("Paragraph 2")
    app.grab(out, f"{p}08-edit")
    app.tap("Cancel", "Button")

    app.tap("Open", "Button")
    app.press("Riverside Lease 2026.pdf")
    app.tap("Fill & Sign", "Button")
    app.see("This form has 7 fields", exact=False)
    app.images(1)
    app.grab(out, f"{p}07-fill-sign")

    app.tap("Q3 2026 Board Report.pdf", "Label")
    app.tap("Read", "Button")
    app.tap("Outline", "Label")
    app.tap("Revenue", "Label")
    app.see("3 / 24")
    app.images(1)
    app.grab(out, f"{p}09-outline")


def fixture_states(app, out, dark):
    """The states the brief lists, on the fixture: damaged, protected, unsaved."""
    p = "d" if dark else ""
    app.tap("Open", "Button")
    app.press("Damaged scan.pdf")
    app.see("Couldn't open this PDF")
    app.grab(out, f"{p}s1-damaged")
    app.tap("Close this tab", "Button")
    app.tap("Open", "Button")
    app.press("Payroll 2026.pdf")
    app.see("This PDF is protected with a password")
    app.grab(out, f"{p}s2-protected")
    app.tap("Close this tab", "Button")
    # a change, then closing its tab asks first
    app.tap("Riverside Lease 2026.pdf", "Label")
    app.tap("Monthly rent *", "Label")
    app.tap("Type the value", "TextInput")
    app.type("1850")
    app.tap("Apply", "Button")
    app.see("Edited")
    tab = app.find("Riverside Lease 2026.pdf •", "Label")["r"]
    app.click(tab[0] + tab[2] + 22, tab[1] + tab[3] / 2)  # its close cross
    app.see("Save the changes to “Riverside Lease 2026.pdf”?")
    app.grab(out, f"{p}s3-unsaved")
    app.tap("Cancel", "Button")


# ---------------------------------------------------------------- shell journey (the real engine)

def shell_journey(app, out, dark):
    """Every mode on the samples, with the real pdf engine, in the shell's
    own window size. The dark pass runs on what the light pass left: the
    same tabs, the comment name and initials it kept, its edits."""
    p = "d" if dark else ""
    if dark:
        app.tap("Open", "Button")  # Home: the light pass ended on a tab
    app.see("Quarterly report.pdf")
    app.images(3)
    app.scroll(-3000)
    app.grab(out, f"{p}01-home")
    if not dark:
        app.tap("Open a PDF from this device", "Button")
        app.find("Couldn't open a PDF from this device", "Label")
        app.find("Bring PDF Tools to the front, then choose the file again.", "Label")
        app.grab(out, "01b-open-from-device")
        app.tap("OK", "Button")
        app.gone("Couldn't open a PDF from this device")

    app.press("Quarterly report.pdf")
    app.images(3)
    app.find(" / 4", "Label", exact=False, timeout=60)
    app.tap("1", "Label", below=150)
    app.find("1 / 4", "Label")
    app.images(3)
    app.grab(out, f"{p}02-reading")
    app.tap("2", "Label", below=150)
    app.find("2 / 4", "Label")
    app.images(2)
    app.grab(out, f"{p}02b-page-2")

    app.tap("Search in document", "TextInput")
    app.type("revenue")
    app.key("Return")
    app.find("9 matches on 4 pages", "Label", timeout=60)
    app.images(1)
    app.grab(out, f"{p}03-find")
    app.tap("Done")

    app.tap("Comment", "Button")
    name_once(app)
    app.find("1 / 4", "Label")  # find went to the first match
    pill_next(app)
    app.find("2 / 4", "Label")
    app.images(1)
    before = comment_count(app)
    click_page(app, 120, 128)  # the Summary's first line
    wait_until(lambda: comment_count(app) == before + 1, "the highlight never joined the comments")
    app.find("Maya Chen", "Label")
    app.grab(out, f"{p}04-comment")
    app.tap("Reply...", "TextInput")
    app.type("Checked against the ledger.")
    app.tap("Post", "Button")
    app.find("Checked against the ledger.", "Label")
    app.grab(out, f"{p}04b-replied")

    app.tap("Pages", "Button")
    app.images(3)
    upright = app.find("2", "Label", below=150)["r"][1]
    try:
        app.find("1 selected", "Label", timeout=2)  # the dark pass: still chosen from the light one
    except AssertionError:
        tap_tile(app, 2)
        app.find("1 selected", "Label")
    app.tap("Rotate right", "Button")
    # A page turned on its side draws a wide, low tile: its number rises.
    wait_until(lambda: app.find("2", "Label", below=150)["r"][1] < upright - 20, "page 2 never turned")
    app.images(3)
    app.grab(out, f"{p}05-pages-rotated")
    click_icon_left_of(app, "Save", nth=2)  # Undo: two icon buttons left of Save
    wait_until(lambda: abs(app.find("2", "Label", below=150)["r"][1] - upright) < 2, "Undo never turned page 2 back")
    app.grab(out, f"{p}05b-undone")

    app.tap("Combine", "Button")
    app.tap("Add files", "Label")
    app.tap("Board minutes.pdf", "Label")
    x, y, w, h = range_fields(app)[1]["r"]
    app.click(x + w / 2, y + h / 2)
    app.type("1")
    app.find("(1 page)", "Label")
    app.grab(out, f"{p}06-combine")
    app.scroll(600)  # the card's end: the total, the name, Combine
    app.find("One PDF of 5 pages", "Label")
    app.grab(out, f"{p}06a-combine-end")
    press_combine(app)
    app.find("Combined into", "Label", exact=False, timeout=60)
    app.images(1)
    app.grab(out, f"{p}06b-combined")

    app.tap("Open", "Button")
    app.press("Apartment lease.pdf")
    app.images(1)
    app.tap("Fill & Sign", "Button")
    app.find("This PDF has no form fields", "Label", exact=False)
    app.find("Saved", "Label")
    app.tap("Add initials", "Label")
    try:
        app.find("Initials", "TextInput", timeout=3)  # empty: the light pass
        app.tap("Initials", "TextInput")
        app.type("MC")
    except AssertionError:
        app.find("MC", "TextInput")  # kept from the light pass
    click_page(app, 360, 330)  # beside clause 3
    app.find("Edited", "Label")
    app.images(1)
    app.grab(out, f"{p}07-fill-sign")

    app.tap("Edit", "Button")
    app.find("Click a paragraph on the page to edit its text.", "Label")
    click_page(app, 120, 168)  # clause 1
    app.find("Paragraph", "Label", exact=False)
    editor = [w for w in app.widgets() if w["ty"] == "TextInput" and (w.get("t") or "").startswith("This agreement")][0]["r"]
    app.click(editor[0] + editor[2] - 6, editor[1] + editor[3] - 7)  # after its last word
    app.type(" (amended)")
    app.grab(out, f"{p}08-edit")
    app.tap("Apply", "Button")
    app.find("Click a paragraph on the page to edit its text.", "Label")
    app.images(1)
    app.grab(out, f"{p}08b-edited")

    app.tap("Save", "Button")
    app.find("Saved", "Label")
    app.grab(out, f"{p}09-saved")

    app.tap("Open", "Button")
    app.press("Damaged scan.pdf")
    app.find("Couldn't open this PDF", "Label")
    app.grab(out, f"{p}10-damaged")
    app.tap("Close this tab", "Button")


def wait_until(check, failure, timeout=30.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if check():
                return
        except (AssertionError, OSError, ValueError):
            pass
        time.sleep(0.4)
    raise AssertionError(failure)


def comment_count(app):
    """The count beside the comment panel's heading."""
    ax, ay, aw, ah = app.area()
    head = [w for w in app.widgets() if w["ty"] == "Label" and w.get("t") == "Comments" and w["r"][0] > ax + aw / 2]
    assert head, "no comment panel"
    hx, hy, hw, hh = head[0]["r"]
    for w in app.widgets():
        if w["ty"] == "Label" and (w.get("t") or "").isdigit() and w["r"][0] > hx and abs(w["r"][1] - hy) < 10:
            return int(w["t"])
    raise AssertionError("no comment count")


def restart(app, out):
    """The same storage after a restart, plus a PDF placed as an import leaves one."""
    app.see("Imported PDF.pdf")
    app.see("Opened today", exact=False)
    app.images(3)
    app.scroll(-3000)
    app.grab(out, "r01-home")
    app.press("Imported PDF.pdf")
    app.images(1)
    app.find("1 / 3", "Label", timeout=60)
    app.grab(out, "r02-imported")
    app.tap("Open", "Button")
    app.press("Quarterly report.pdf")
    app.find("2 / 4", "Label", timeout=60)  # where the shell run left it
    app.images(1)
    app.grab(out, "r03-place-kept")


def full(app, out):
    """No room left: the engine refuses to write, and the app says so."""
    app.find("PDF Tools' storage is full", "Label", timeout=90)
    app.grab(out, "f01-home-full")
    app.scroll(400)
    time.sleep(1)
    trash = sorted((w for w in app.widgets() if w["ty"] in ("Button", "ButtonFlat") and not w.get("t") and w["r"][2] < 30),
                   key=lambda w: (w["r"][1], w["r"][0]))
    assert trash, "no remove button on the cards"
    x, y, w, h = trash[0]["r"]
    app.click(x + w / 2, y + h / 2)
    app.find("Remove it from PDF Tools' storage?", "Label")
    app.grab(out, "f02-remove-asks")
    app.tap("Remove", "Button")
    app.find("from PDF Tools.", "Label", exact=False, timeout=20)
    app.grab(out, "f03-removed")


def empty(app, out, dark=False):
    p = "d" if dark else ""
    app.find("No PDFs here yet", "Label", timeout=60)
    app.grab(out, f"{p}e01-empty")
    app.tap("Open a PDF from this device", "Button")
    app.find("Bring PDF Tools to the front, then choose the file again.", "Label")
    app.grab(out, f"{p}e02-empty-open")
    app.tap("OK", "Button")


def missing(app, out):
    app.see("This device has no PDF engine")
    app.see("Not available here")
    app.grab(out, "m01-missing-home")
    app.press("Field guide.pdf")
    app.see("This device has no PDF engine")
    app.grab(out, "m02-missing-document")
    app.tap("Close this tab", "Button")


# ---------------------------------------------------------------- runs

def compare(out, names):
    """Each grab beside its design, the same height: compare/NN-name.png."""
    from PIL import Image
    folder = out / "compare"
    folder.mkdir(exist_ok=True)
    for design, grab in names:
        a = Image.open(DESIGNS / f"{design}.png").convert("RGB")
        b = Image.open(out / f"{grab}.png").convert("RGB")
        b = b.resize((int(b.width * a.height / b.height), a.height))
        sheet = Image.new("RGB", (a.width + b.width + 12, a.height), "#ff00ff")
        sheet.paste(a, (0, 0))
        sheet.paste(b, (a.width + 12, 0))
        sheet.save(folder / f"{grab}.png")


def keep_failure(app, out, name):
    """On a failed step: the screen and its widgets, for whoever reads the run."""
    try:
        app.grab(out, f"failed-{name}")
        (out / f"failed-{name}.snap.json").write_text(json.dumps(app.remote("snap"), indent=1))
    except Exception as error:  # the failure itself matters more
        print("could not keep the failure:", error)


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
        except BaseException:
            keep_failure(app, out, name)
            raise
        finally:
            app.quit()
        check_log(log)

    with Lock(args.lock):
        if "shell" in runs or "restart" in runs:
            home = scratch / "home"
            fixture(home / "apps")

            def light_then_dark(app, out):
                shell_journey(app, out, dark=False)
                app.style("octosense-dark", "octosense dark")
                shell_journey(app, out, dark=True)
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


def dev_bundle(out):
    """A scratch copy of the bundle with the stand-in engine in place of engine()."""
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(BUNDLE, out)
    main = out / "main.splash"
    text = main.read_text()
    start = text.index("fn engine(method, args, done){")
    end = text.index("\n}\n", start) + 3
    if 'host.request("pdf." + method' not in text[start:end] or text.count("fn engine(method, args, done){") != 1:
        raise SystemExit("engine() in main.splash is not what this script replaces: update dev_bundle()")
    main.write_text(text[:start] + (FIXTURE / "engine.splash").read_text().rstrip() + "\n" + text[end:])
    return out


DESIGNED = [("01-home", "01-home"), ("02-reading", "02-reading"), ("03-find", "03-find"), ("04-comment", "04-comment"),
            ("05-pages", "05-pages"), ("06-combine", "06-combine"), ("07-fill-sign", "07-fill-sign"), ("08-edit", "08-edit"),
            ("09-dark-reading", "09-outline")]


def card_host_runs(args, runs):
    binary = args.card_host.resolve()
    assert binary.is_file(), f"build App Hub's card-host first: {binary}"
    assert port_free(args.port), f"port {args.port} is taken"
    with tempfile.TemporaryDirectory(prefix="pdftools-card-host-", dir=args.work) as scratch:
        scratch = Path(scratch)
        if "missing" in runs:
            data = scratch / "missing"
            fixture(data)
            out = args.output / "missing"
            out.mkdir(parents=True, exist_ok=True)
            app = CardHost(binary, BUNDLE, data, args.port, "macos", out / "card-host.log")
            try:
                missing(app, out)
            except BaseException:
                keep_failure(app, out, "missing")
                raise
            finally:
                app.quit()
            check_log(out / "card-host.log")
        if "fixture" in runs:
            bundle = dev_bundle(scratch / "bundle")
            for dark in (False, True):
                data = scratch / ("fixture-dark" if dark else "fixture")
                subprocess.run([sys.executable, str(FIXTURE / "make_fixture.py"), str(data / APP)], check=True)
                global FIXTURE_JAIL
                FIXTURE_JAIL = data / APP
                out = args.output / "fixture"
                out.mkdir(parents=True, exist_ok=True)
                log = out / ("card-host-dark.log" if dark else "card-host.log")
                app = CardHost(binary, bundle, data, args.port, "macos-dark" if dark else "macos", log)
                try:
                    fixture_journey(app, out, dark)
                    fixture_states(app, out, dark)
                except BaseException:
                    keep_failure(app, out, "dark" if dark else "light")
                    raise
                finally:
                    app.quit()
                check_log(log)
                compare(out, [(design, ("d" if dark else "") + grab) for design, grab in DESIGNED])


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--shell", type=Path, help="the desktop shell binary (target/debug/octosense)")
    parser.add_argument("--card-host", type=Path, help="App Hub's card-host, for the runs without host services")
    parser.add_argument("--lock", type=Path, default=Path(tempfile.gettempdir()) / "octosense-shell.lock",
                        help="the folder that marks a hidden shell run on this machine")
    parser.add_argument("--port", type=int, default=0, help="the remote bridge's port (8915 for a shell, 8911 for card-host)")
    parser.add_argument("--output", type=Path, default=ROOT / "target/pdftools-ui")
    parser.add_argument("--work", type=Path, help="where the runs' OctoSense homes go (default: the system's temporary folder)")
    parser.add_argument("--only", choices=["shell", "restart", "full", "empty", "missing", "fixture"], action="append")
    parser.add_argument("--budget-ms", type=int, help="MAKEPAD_SPLASH_BUDGET_MS for the hosts (default: the runtime's 64 ms); for a loaded machine")
    parser.add_argument("--grab-scale", type=float, default=0.5, help="the grabs' scale (the bridge's /g scale): 0.5 is one pixel per point on a 2x screen")
    args = parser.parse_args()
    Driver.scale = args.grab_scale
    Driver.budget_ms = args.budget_ms
    card_host_only = ("missing", "fixture")
    runs = args.only or (["shell", "restart", "full", "empty"] if args.shell else []) + (list(card_host_only) if args.card_host else [])
    assert runs, "give --shell, --card-host or both"
    if any(run not in card_host_only for run in runs):
        assert args.shell, "these runs need --shell"
        args.port = args.port or 8915
        shell_runs(args, runs)
    if any(run in card_host_only for run in runs):
        assert args.card_host, "the missing and fixture runs need --card-host"
        args.port = 8911 if args.port in (0, 8915) else args.port
        card_host_runs(args, runs)
    print("all journeys passed; grabs in", args.output)


if __name__ == "__main__":
    main()

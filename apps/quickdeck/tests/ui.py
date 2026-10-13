#!/usr/bin/env python3
"""Quick Deck v2 end to end, in App Hub's card-host and in a hidden OctoSense desktop shell.

card-host runs serve no host services:

  missing  the shipped bundle on the dev fixture's library: no deck engine
           and no files service (Home, a deck's tab, New from an outline and
           its Draft with AI), light and dark
  fixture  a scratch copy of the bundle with dev-fixture/engine.splash in
           place of engine(), and make_fixture.py's sample decks (the approved
           designs' own sample text): every designed screen at the designs'
           1536 x 1024, light and dark, each grab put beside its design in
           compare/, then a table's and the footer's controls; the inspector's
           switches and marks are checked against what the stand-in reports,
           and a change and its undo against what it reports after them
  states   the same stand-in: a damaged deck, a protected, a held and an
           unsupported presentation, the unsaved-changes bar, the storage-full
           bar, and the empty library of a first run, light and dark

Shell runs use the shell this checkout builds, with the real deck engine
(`cargo build --locked -p octosense --no-default-features --features
app-hub,craft-engines`). For each run the driver makes a fresh OctoSense home,
starts the shell hidden: on macOS from a Terminal.app tab (never as a child of
this process); on Linux, where MAKEPAD_HIDE_WINDOWS does nothing, on a private
headless weston (the pixman renderer, never idle), which stops with it. It
opens the app with MAKEPAD_WM_TEST_APP and walks it over Makepad's remote
bridge, saving original /g grabs. Every engine call is real.

  shell    a deck made from an outline, then edited: text in place, a format,
           a chart inserted and its data changed, a layout, a theme, the sorter
           (duplicate, hide, a section), notes, undo and redo, save, export
           (.pptx, PDF and pictures), Present and its notes, light, then dark
           through the shell's own style menu; in the light pass the
           inspector's states follow real changes and their undo: a box's
           anchor and autofit, a chart element, a table option, the colour set
           and the footer
  restart  the same home again: the library, the deck's slides and covers,
           the place it was left at, and its exports

Shell runs hold a lock folder, so one hidden shell runs on this machine at a
time, and on macOS refuse to start while another OctoSense runs (on Linux each
run has a compositor of its own).

  python3 apps/quickdeck/tests/ui.py --card-host <App Hub>/target/release/card-host --output target/quickdeck-ui
  python3 apps/quickdeck/tests/ui.py --shell target/debug/octosense --lock <dir> --output target/quickdeck-ui
  (Linux) python3 apps/quickdeck/tests/ui.py --shell target/debug/octosense --lock <dir> --work <dir> --grab-scale 1 --output target/quickdeck-ui
"""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[3]
APP_DIR = ROOT / "apps/quickdeck"
BUNDLE = APP_DIR / "bundle"
FIXTURE = APP_DIR / "dev-fixture"
DESIGNS = APP_DIR / "design/source"
APP = "os.quickdeck"
QUOTA = 64 * 1024 * 1024
DECK = "Q3 2026 Board Update"
# The slide's size in points (the engine's default 16:9).
SLIDE_W = 960
LINUX = sys.platform.startswith("linux")


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

    scale = 0.5
    # MAKEPAD_SPLASH_BUDGET_MS for the hosts these runs start, or None for the
    # runtime's 64 ms: the budget is wall-clock time, so a loaded machine can
    # overrun it in any handler (--budget-ms).
    budget_ms = None
    # Whether the journeys check the inspector's states (--grabs-only turns it
    # off, to take another bundle's frames for a before/after comparison).
    check_states = True

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
                roots = [w["r"] for w in snap if w["ty"] in ("Splash", "Window")]
                if roots:
                    return max(roots, key=lambda r: r[2] * r[3])
            except (OSError, URLError, AssertionError):
                pass
            if time.monotonic() > deadline:
                raise AssertionError("the app's surface never appeared in /snap")
            time.sleep(0.5)

    def widgets(self):
        return [w for w in self.remote("snap")["s"] if w["ty"] != "Splash" and w["r"][2] > 0 and w["r"][3] > 0]

    def find(self, text, kind=None, below=0.0, timeout=30.0, exact=True, right=None):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                found = [w for w in self.widgets()
                         if (w.get("t") == text if exact else text in (w.get("t") or ""))
                         and (kind is None or w["ty"] == kind) and w["r"][1] >= below
                         and (right is None or w["r"][0] >= right)]
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
        """The app's own pictures (the engine's renders): inside its surface."""
        ax, ay, aw, ah = self.area()
        return [w for w in self.widgets() if w["ty"] == "Image" and w["r"][2] < aw - 1
                and ax - 1 <= w["r"][0] <= ax + aw and ay - 1 <= w["r"][1] <= ay + ah]

    def images(self, count, timeout=60.0):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                if len(self.pictures()) >= count:
                    time.sleep(0.4)
                    return
            except (OSError, ValueError, AssertionError):
                pass
            time.sleep(0.3)
        raise AssertionError(f"fewer than {count} slide pictures after {timeout} s")

    def click(self, x, y):
        ax, ay, aw, ah = self.area()
        assert ax <= x <= ax + aw and ay <= y <= ay + ah, f"({x}, {y}) is outside the app's window {[ax, ay, aw, ah]}"
        self.remote("click", x=x, y=y, wait=1)
        time.sleep(0.45)

    def tap(self, text, kind=None, below=0.0, exact=True, right=None):
        x, y, w, h = self.find(text, kind, below, exact=exact, right=right)["r"]
        self.click(x + w / 2, y + h / 2)

    def scroll(self, dy, at=None):
        x, y, w, h = self.area()
        cx, cy = at if at else (x + w / 2, y + h / 2)
        self.remote("m", k="scroll", x=cx, y=cy, dy=dy, wait=1)
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
        path.with_suffix(".json").write_text(json.dumps({"area": self.area(), "window": self.size()}))
        print("  ", path)


class Shell(Driver):
    """The desktop shell, started hidden: from a Terminal.app tab on macOS, on
    a private headless weston on Linux (as PDF Tools' ui.py does)."""

    def __init__(self, binary, home, port, log):
        super().__init__(port)
        self.log = log
        self.window = self.process = self.weston = None
        env = {
            "OCTOSENSE_HOME": home,
            "OCTOS_APP_CORE_DIR": home / "octos-home" / ".octos",
            "MAKEPAD_HOME": home / "makepad",
            "MAKEPAD_HIDE_WINDOWS": "1",
            "MAKEPAD_REMOTE": f"127.0.0.1:{port}",
            "MAKEPAD_WM_TEST_APP": "quickdeck",
        }
        if Driver.budget_ms:
            env["MAKEPAD_SPLASH_BUDGET_MS"] = str(Driver.budget_ms)
        if LINUX:
            self.start_on_weston(binary, home, env, log)
        else:
            self.start_in_terminal(binary, env, log)
        self.pid = None
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            text = log.read_text(errors="replace") if log.exists() else ""
            listening = re.search(r"listening on 127\.0\.0\.1:(\d+) pid=(\d+)", text)
            if listening:
                self.pid = int(listening[2])
                break
            if self.process is not None and self.process.poll() is not None:
                break
            time.sleep(0.5)
        if not self.pid:
            self.stop_compositor()
        assert self.pid, f"the shell did not start its remote bridge; see {log}"
        try:
            self.ready()
        except BaseException:
            self.stop_compositor()  # a session of its own: nothing else would stop it
            raise

    def start_in_terminal(self, binary, env, log):
        command = "cd {} && env {} {} > {} 2>&1; exit".format(
            shlex.quote(str(ROOT)), " ".join(f"{k}={shlex.quote(str(v))}" for k, v in env.items()),
            shlex.quote(str(binary)), shlex.quote(str(log)))
        script = 'tell application "Terminal" to do script "{}"'.format(command.replace("\\", "\\\\").replace('"', '\\"'))
        answer = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, check=True).stdout
        found = re.search(r"window id (\d+)", answer)
        self.window = found[1] if found else None

    def start_on_weston(self, binary, home, env, log):
        """Linux: a headless weston of the run's own (pixman renders without a
        GPU; an idle weston stops presenting frames, so it never idles), in a
        private runtime folder, and the shell on it with Makepad's Wayland
        backend, each in a session of its own. The XDG folders are the run
        home's, so nothing lands in the account's own."""
        # A short folder: a socket's path must fit in 108 bytes.
        self.runtime = Path(tempfile.mkdtemp(prefix="quickdeck-wl-"))
        socket_name = "wayland-quickdeck"
        weston_log = log.with_name("weston.log")
        self.weston = subprocess.Popen(
            ["weston", "--backend=headless", "--renderer=pixman", "--idle-time=0", f"--socket={socket_name}",
             "--no-config", f"--log={weston_log}"],
            env=dict(os.environ, XDG_RUNTIME_DIR=str(self.runtime)), stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        deadline = time.monotonic() + 20
        while not (self.runtime / socket_name).exists():
            if self.weston.poll() is not None or time.monotonic() > deadline:
                self.stop_compositor()
                raise AssertionError(f"weston did not open its socket; see {weston_log}")
            time.sleep(0.1)
        env.update(XDG_RUNTIME_DIR=self.runtime, WAYLAND_DISPLAY=socket_name)
        for kind in ("CONFIG", "DATA", "CACHE", "STATE"):
            env[f"XDG_{kind}_HOME"] = home / "xdg" / kind.lower()
            env[f"XDG_{kind}_HOME"].mkdir(parents=True, exist_ok=True)
        with log.open("wb") as handle:
            self.process = subprocess.Popen([str(binary)], cwd=ROOT, env=dict(os.environ, **{k: str(v) for k, v in env.items()}),
                                            stdin=subprocess.DEVNULL, stdout=handle, stderr=subprocess.STDOUT,
                                            start_new_session=True)

    def stop_compositor(self):
        """Linux: the shell if it still runs, then its weston and the runtime folder."""
        for process in (self.process, self.weston):
            if process is None or process.poll() is not None:
                continue
            for signal_number in (signal.SIGTERM, signal.SIGKILL):
                try:
                    os.killpg(process.pid, signal_number)
                except ProcessLookupError:
                    break
                try:
                    process.wait(timeout=10)
                    break
                except subprocess.TimeoutExpired:
                    pass
        if self.weston is not None:
            shutil.rmtree(self.runtime, ignore_errors=True)

    def alive(self):
        if self.process is not None:
            return self.process.poll() is None
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
            subprocess.run(["osascript", "-e", f'tell application "Terminal" to close window id {self.window}'], capture_output=True)
        if self.weston is not None:
            self.stop_compositor()
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


def slide_rect(app):
    """The canvas's slide picture: [x, y, w, h] in points (the largest picture
    right of the navigator and left of the inspector)."""
    ax, ay, aw, ah = app.area()
    imgs = [w["r"] for w in app.pictures() if w["r"][2] > 300 and w["r"][1] > ay + 100]
    assert imgs, "no slide on the canvas"
    return max(imgs, key=lambda r: r[2] * r[3])


def click_slide(app, px, py):
    """Click a point of the canvas's slide, given in slide points."""
    x, y, w, h = slide_rect(app)
    k = w / SLIDE_W
    app.click(x + px * k, y + py * k)


def pill_label(app):
    return next(w for w in app.widgets() if re.fullmatch(r"\d+ / \d+", w.get("t") or ""))


def go_to(app, n):
    """The canvas pill's ‹ and › until slide `n` shows (the navigator keeps no
    scroll position a script could set; the pill always works)."""
    for _ in range(40):
        label = pill_label(app)
        at, total = map(int, label["t"].split(" / "))
        if at == n:
            return
        x, y, w, h = label["r"]
        buttons = sorted((b["r"] for b in app.widgets() if b["ty"] in ("Button", "ButtonFlat") and not b.get("t")
                          and abs(b["r"][1] + b["r"][3] / 2 - (y + h / 2)) < 12), key=lambda r: r[0])
        if at < n:
            bx, by, bw, bh = [r for r in buttons if r[0] >= x + w - 1][0]
        else:
            bx, by, bw, bh = [r for r in buttons if r[0] + r[2] <= x + 1][-1]
        app.click(bx + bw / 2, by + bh / 2)
        time.sleep(0.6)
    raise AssertionError(f"never reached slide {n}")


def seg(app, name):
    """One of the inspector's segments: Format, Layout, Theme, Notes."""
    ax, ay, aw, ah = app.area()
    app.tap(name, "Button", right=ax + aw - 360)


def labels(app, text):
    return len([w for w in app.widgets() if w["ty"] == "Label" and w.get("t") == text])


def open_card(app, title):
    """Home: open a deck from its card. A click the bridge couldn't deliver
    while the app was busy (drawing covers) is tried again, until the deck's
    tab shows."""
    for _ in range(4):
        app.tap(title, "Label", below=380)
        try:
            wait_until(lambda: any(w["ty"] == "Label" and w.get("t") in (title, title + " •") and w["r"][1] < app.area()[1] + 70
                                   for w in app.widgets()), "no tab", timeout=8)
            return
        except AssertionError:
            pass
    raise AssertionError(f"{title!r} never opened from its card")


def home_tab(app):
    app.tap("Home", "Label")


# ---------------------------------------------------------------- fixture journey (card-host)

def fixture_journey(app, out, dark):
    """The approved designs' screens on the dev fixture, one grab per screen."""
    p = "d" if dark else ""
    app.find(DECK, "Label")
    app.images(6, timeout=120)
    app.grab(out, f"{p}01-home")

    # The editor screens show the six decks open: open them in the designs'
    # order, then come back to the first.
    for title in (DECK, "Northern Lights Field Trip", "Riverside Community Garden", "Welcome to the Team", "Spring Fundraiser Pitch", "Coastal Survey Findings"):
        open_card(app, title)
        app.find("Slide 1 of ", "Label", exact=False, timeout=60)
        home_tab(app)
        app.find("Start a new deck", "Label")
    app.tap(DECK, "Label", below=0, exact=False)
    app.find(" of 12", "Label", exact=False, timeout=60)
    app.images(4)

    # 02: slide 5's bullet box, edited in place.
    go_to(app, 5)
    app.images(4)
    click_slide(app, 250, 280)  # the bullet box: chosen
    app.find("Applies to", "Label")
    # As the stand-in reports the box (design 02): its text shrinks to fit, top.
    expect("the box shrinking its text, top", lambda: switch_on(app, "Shrink text when it overflows") and segment(app, "Vertical alignment") == 0)
    click_slide(app, 250, 280)  # tapped again: its text in place
    wait_until(lambda: any(w["ty"] == "TextInput" and (w.get("t") or "").startswith("Northwest led") for w in app.widgets()), "the box's text field never came")
    field = [w for w in app.widgets() if w["ty"] == "TextInput" and (w.get("t") or "").startswith("Northwest led")][0]["r"]
    app.click(field[0] + 250, field[1] + 76)  # after "rose 18%"
    app.grab(out, f"{p}02-edit-text")
    click_slide(app, 700, 520)  # the slide's empty corner: the text goes in, the box whole again
    app.gone("Applies to")

    # 03: the chart strip and slide 4's chart.
    go_to(app, 4)
    app.images(4)
    app.tap("Chart", "Label", below=70)
    app.find("Insert chart", "Button")
    click_slide(app, 480, 300)
    app.find("Elements", "Label")
    # Design 03: title, data labels and gridlines on, no legend, colourful.
    expect("the chart's elements", lambda: switch_on(app, "Title") and not switch_on(app, "Legend") and switch_on(app, "Data labels")
           and switch_on(app, "Gridlines") and segment(app, "Colours", 50) == 0)
    app.grab(out, f"{p}03-chart")
    app.tap("Chart", "Label", below=70)  # the strip closes again
    app.gone("Insert chart")

    # 04: the Layout segment on slide 7.
    seg(app, "Layout")
    go_to(app, 7)
    app.find("Slide layout", "Label")
    app.images(4)
    app.grab(out, f"{p}04-layout")

    # 05: the Theme segment on the title slide.
    seg(app, "Theme")
    go_to(app, 1)
    app.find("Themes", "Label")
    # The deck's colour set and font pair (deck.open): Harbor's own, whose
    # fonts are Classic's.
    expect("Harbor's colours and Classic's fonts", lambda: {"Harbor", "Classic"} <= selected_texts(app))
    app.grab(out, f"{p}05-theme")

    # 06: the sorter, slides 5 and 6 chosen.
    app.tap("Sorter", "Button")
    app.find("Drag slides to reorder", "Label")
    app.images(10)
    tile_number(app, 1)  # the current slide starts chosen: unchoose it
    tile_number(app, 5)
    tile_number(app, 6)
    app.find("2 selected", "Label")
    app.grab(out, f"{p}06-sorter")

    # 07: New from an outline, its cards typed as the design shows them.
    app.tap("New", "Button")
    app.tap("New from an outline", "Button")
    app.find("Draft with AI", "Label")
    outline_cards(app)
    app.grab(out, f"{p}07-outline")
    tab = app.find("New deck", "Label")["r"]
    app.click(tab[0] + tab[2] + 22, tab[1] + tab[3] / 2)  # its close cross: the six decks' tabs again, as 09 and 10 draw them
    app.gone("Draft with AI")

    # 08: Present on slide 4.
    app.tap(DECK, "Label", exact=False)
    app.tap("Slides", "Button")
    go_to(app, 4)
    app.tap("Present", "Button")
    app.find("Exit", "Button", timeout=30)
    app.images(1)
    app.grab(out, f"{p}08-present")
    app.tap("Exit", "Button")
    app.find("Present", "Button")

    # 09: Export, after a presentation and a PDF.
    app.tap("Export", "Button")
    app.find("Saved in Quick Deck's storage", "Label")
    app.tap("Presentation", "Label")
    app.tap("Export presentation", "Button")
    app.find(DECK + ".pptx", "Label", exact=False)
    app.tap("PDF", "Label")
    app.tap("Export PDF", "Button")
    app.find(DECK + ".pdf", "Label", exact=False)
    app.gone("OK", timeout=12)
    app.grab(out, f"{p}09-export")
    app.tap("Export", "Button")  # back to the slides

    # 10: the Notes segment on slide 6 (the design draws it dark).
    seg(app, "Notes")
    go_to(app, 6)
    app.find("Speaker notes", "Label")
    app.find("28 words  ·  about 10 seconds", "Label")
    app.images(4)
    app.grab(out, f"{p}10-notes")

    # 11: slide 6's table and its options (design 10's table: a header row,
    # banded rows and a total row).
    seg(app, "Format")
    click_slide(app, 496, 284)
    app.find("Header row", "Label", timeout=30)
    expect("the table's options", lambda: switch_on(app, "Header row") and switch_on(app, "Total row") and switch_on(app, "Banded rows")
           and not switch_on(app, "First column") and not switch_on(app, "Last column") and not switch_on(app, "Banded columns"))
    app.grab(out, f"{p}11-table")
    # A table option and its undo: the switch follows the table.
    app.click(*switch_point(app, "First column"))
    expect("the first column on", lambda: switch_on(app, "First column"))
    app.tap("Undo", "Label")
    expect("the first column off again after Undo", lambda: not switch_on(app, "First column"))

    # 12: the footer (design 05: slide numbers on, no date).
    seg(app, "Theme")
    x, y, w, h = app.area()
    seek(app, "Slide numbers", "Label", at=(x + w - 170, y + h / 2))
    expect("the deck's footer", lambda: switch_on(app, "Slide numbers") and not switch_on(app, "Date")
           and any(f["ty"] == "TextInput" and f.get("val") == "Northwind Cooperative" for f in app.widgets()))
    app.grab(out, f"{p}12-footer")
    app.click(*switch_point(app, "Date"))
    expect("the date on", lambda: switch_on(app, "Date"))
    app.tap("Undo", "Label")
    expect("the date off again after Undo", lambda: not switch_on(app, "Date"))


def seek(app, text, kind="Button", exact=True, at=None, timeout=60.0):
    """Scroll the page under `at` (the middle of the window by default) until
    `text` is in view: a short window scrolls what 1024 points show whole."""
    x, y, w, h = app.area()
    point = at or (x + w / 2, y + h / 2)
    deadline = time.monotonic() + timeout
    while True:
        found = [f for f in app.widgets() if (f.get("t") == text if exact else text in (f.get("t") or ""))
                 and f["ty"] == kind and f["r"][1] + f["r"][3] <= y + h - 10]
        if found:
            return found[0]["r"]
        if time.monotonic() > deadline:
            raise AssertionError(f"{text!r} never came into view")
        app.scroll(300, at=point)
        time.sleep(1)


def reach(app, text, kind="Button", at=None):
    """seek(), then tap it."""
    fx, fy, fw, fh = seek(app, text, kind, at=at)
    app.click(fx + fw / 2, fy + fh / 2)


def switch_on(app, label):
    """The switch on `label`'s row (a Toggle reports its state as `c`)."""
    lx, ly, lw, lh = app.find(label, "Label")["r"]
    cy = ly + lh / 2
    found = [w for w in app.widgets() if "c" in w and w["r"][0] > lx and w["r"][1] <= cy <= w["r"][1] + w["r"][3]]
    assert found, f"no switch beside {label!r}"
    return found[0]["c"] == 1


def switch_point(app, label):
    """Where to click `label`'s switch."""
    lx, ly, lw, lh = app.find(label, "Label")["r"]
    cy = ly + lh / 2
    found = [w["r"] for w in app.widgets() if "c" in w and w["r"][0] > lx and w["r"][1] <= cy <= w["r"][1] + w["r"][3]]
    assert found, f"no switch beside {label!r}"
    x, y, w, h = found[0]
    return x + w / 2, y + h / 2


def selected_texts(app):
    """The words inside every view that says it is selected (View.selected)."""
    widgets = app.widgets()
    views = [w["r"] for w in widgets if w.get("selected") == "true"]
    out = set()
    for w in widgets:
        if w["ty"] != "Label" or not w.get("t"):
            continue
        x, y, ww, hh = w["r"]
        for vx, vy, vw, vh in views:
            if vx - 1 <= x and x + ww <= vx + vw + 1 and vy - 1 <= y and y + hh <= vy + vh + 1:
                out.add(w["t"])
    return out


def segment(app, label, below=60):
    """Which of the segments under `label` is selected (0-based), or -1."""
    lx, ly, lw, lh = app.find(label, "Label")["r"]
    row = sorted((w for w in app.widgets() if "selected" in w and ly + lh <= w["r"][1] <= ly + lh + below), key=lambda w: w["r"][0])
    assert row, f"no segments under {label!r}"
    on = [i for i, w in enumerate(row) if w["selected"] == "true"]
    return on[0] if on else -1


def expect(what, check, timeout=30.0):
    """Wait until the inspector shows `what` (skipped with --grabs-only)."""
    if not Driver.check_states:
        return
    wait_until(check, f"the inspector never showed {what}", timeout=timeout)


def tile_number(app, n):
    """The sorter: tap slide n's tile, above its number, scrolling the sorter
    until both are in view (a short window shows one row and a half)."""
    ax, ay, aw, ah = app.area()
    for _ in range(8):
        found = [f["r"] for f in app.widgets() if f["ty"] == "Label" and f.get("t") == str(n)
                 and f["r"][1] > ay + 170 and f["r"][1] + f["r"][3] <= ay + ah - 50]
        if found and found[0][1] - 70 > ay + 170:
            x, y, w, h = found[0]
            app.click(x + 60, y - 70)
            return
        app.scroll(150 if not found else -100, at=(ax + aw / 2, ay + ah / 2))
    raise AssertionError(f"slide {n}'s tile never came into view")


def slide_count(app):
    """The sorter's status line: "12 slides  ·  2 selected"."""
    for w in app.widgets():
        found = re.match(r"(\d+) slides?  ·  \d+ selected", w.get("t") or "")
        if found:
            return int(found[1])
    raise AssertionError("no slide count on the status line")


OUTLINE = [
    ("Q3 2026 Board Update", "Northwind Cooperative · 21 October 2026"),
    ("Agenda", "Highlights\nRevenue\nMembership\nOperations\nOutlook"),
    ("Quarter at a glance", "$28.7M revenue, up 12%\n18,400 members, up 6%\n96% on-time delivery"),
    ("Revenue by region", "Northwest led with $12.4 million"),
]


TITLE_HINTS = ("The deck's title", "Slide title")
POINTS_HINTS = ("A subtitle (optional)", "Points, one per line")


def empty_field(app, hints):
    """The first empty outline field of a kind (an empty TextInput reports its
    hint), scrolling the outline down until it is in view."""
    for _ in range(6):
        x, y, w, h = app.area()
        found = sorted((f for f in app.widgets() if f["ty"] == "TextInput" and f.get("t") in hints and f["r"][1] > y + 300),
                       key=lambda f: f["r"][1])
        if found and found[0]["r"][1] + 30 < y + h - 70:
            return found[0]["r"]
        app.scroll(240, at=outline_point(app))
    raise AssertionError(f"no empty {hints[1]!r} field in view")


def outline_point(app):
    """A point over the outline's list, low in the window (any window size)."""
    x, y, w, h = app.area()
    return (x + w * 0.3, y + h - 150)


def outline_cards(app):
    app.tap("What is the deck about? A topic, the audience, the points to cover…", "TextInput")
    app.type("Quarterly update for the board: revenue by region, members, operations and our Q4 plans")
    if len([w for w in app.widgets() if w["ty"] == "TextInput" and w.get("t") in TITLE_HINTS]) < len(OUTLINE):
        app.tap("Add slide", "Button")
    for title, points in OUTLINE:
        tx, ty, tw, th = empty_field(app, TITLE_HINTS)
        app.click(tx + tw / 2, ty + th / 2)
        app.type(title)
        px, py, pw, ph = empty_field(app, POINTS_HINTS)
        app.click(px + pw / 2, py + min(ph / 2, 16))
        for k, line in enumerate(points.split("\n")):
            if k:
                app.key("ReturnKey")
            app.type(line)
    app.scroll(-3000, at=outline_point(app))  # back to the top, as design 07 shows it
    app.find("4 slides", "Label", exact=False)


def fixture_states(app, out, dark):
    """The states the brief lists, on the fixture: damaged, protected, held,
    unsupported, unsaved changes."""
    p = "d" if dark else ""
    app.find("Start a new deck", "Label")
    for title, head, name in (("Damaged deck", "Couldn't open this deck", "s1-damaged"),
                              ("Payroll 2025", "This presentation is protected with a password", "s2-protected"),
                              ("Town hall recording", "Quick Deck can't open this presentation yet", "s3-held"),
                              ("Minutes 2019", "Quick Deck opens .pptx presentations", "s4-unsupported")):
        app.scroll(1200)
        open_card(app, title)
        app.find(head, "Label", timeout=30)
        app.grab(out, f"{p}{name}")
        app.tap("Close this tab", "Button")
        app.scroll(-3000)  # Home keeps its scroll: back to the top
        app.find("Start a new deck", "Label")
    app.scroll(-3000)
    open_card(app, DECK)
    app.find(" of 12", "Label", exact=False, timeout=60)
    seg(app, "Notes")
    note = [w for w in app.widgets() if w["ty"] == "TextInput" and w["r"][0] > app.area()[2] - 360]
    x, y, w, h = note[0]["r"]
    app.click(x + w / 2, y + 30)
    app.type("Thank the board.")
    app.find("Edited", "Label", timeout=20)
    tab = app.find(DECK + " •", "Label")["r"]
    app.click(tab[0] + tab[2] + 22, tab[1] + tab[3] / 2)  # its close cross
    app.find("Save changes to “" + DECK + "”?", "Label")
    app.grab(out, f"{p}s5-unsaved")
    app.tap("Cancel", "Button")
    app.gone("Save changes to “" + DECK + "”?")


def full(app, out, dark):
    """Storage full: a save is refused, the bar says so, the status line goes red."""
    p = "d" if dark else ""
    open_card(app, DECK)
    app.find(" of 12", "Label", exact=False, timeout=60)
    app.tap("Save", "Button")
    app.find("Quick Deck's storage is full: remove a deck or an export to make room. Your edits stay in the open deck.", "Label", timeout=20)
    app.grab(out, f"{p}s6-storage-full")


def empty(app, out, dark):
    p = "d" if dark else ""
    app.find("Start a new deck", "Label")
    app.find("Harbor", "Label")
    assert labels(app, "Recent") == 0, "an empty library shows Recent"
    app.grab(out, f"{p}s7-empty-library")


def missing(app, out, dark):
    p = "d" if dark else ""
    app.find("The deck engine isn't available here", "Label", timeout=60)
    app.find("Not available here", "Label")
    app.grab(out, f"{p}m01-missing-home")
    open_card(app, DECK)
    app.find("The deck engine isn't available here", "Label")
    app.grab(out, f"{p}m02-missing-deck")
    app.tap("Close this tab", "Button")
    app.tap("New from an outline", "Button")
    app.tap("What is the deck about? A topic, the audience, the points to cover…", "TextInput")
    app.type("Quarterly update for the board")
    app.tap("Draft outline", "Button")
    app.find("Drafting needs OctoSense's AI service, which isn't available here. You can still type the outline.", "Label", timeout=20)
    app.grab(out, f"{p}m03-missing-outline")


# ---------------------------------------------------------------- shell journey (the real engine)

def shell_journey(app, out, dark):
    """A deck made from an outline and edited with the real deck engine, in
    the shell's own window. The dark pass works on what the light pass left."""
    p = "d" if dark else ""
    if not dark:
        app.find("Start a new deck", "Label", timeout=90)
        app.find("Harbor", "Label", timeout=90)
        app.grab(out, "01-home-empty")
        app.tap("New from an outline", "Button")
        app.find("Draft with AI", "Label")
        outline_cards(app)
        # Draft with AI in a home with no provider: the app says how to set one
        # up, and the typed outline stays.
        app.tap("Draft outline", "Button")
        said = ("to draft outlines", "couldn't draft", "isn't available here", "allowance", "Too many requests", "taking too long")
        wait_until(lambda: any(any(x in (w.get("t") or "") for x in said) for w in app.widgets()), "Draft with AI never answered", timeout=90)
        shown = [w.get("t") for w in app.widgets() if any(x in (w.get("t") or "") for x in said)]
        (out / "draft.json").write_text(json.dumps({"shown": shown}, indent=1))
        app.grab(out, "01b-draft")
        app.find("4 slides", "Label", exact=False)
        app.tap("Make deck", "Button")
        app.find("Slide 1 of 4", "Label", exact=False, timeout=120)
    else:
        # The style switch keeps the screen: the deck is still open.
        app.find(DECK, "Label", exact=False, timeout=60)
        if not any((w.get("t") or "").startswith("Slide ") and " of " in (w.get("t") or "") for w in app.widgets()):
            open_card(app, DECK)
        app.find(" of ", "Label", exact=False, timeout=120)
    app.images(2, timeout=120)
    app.grab(out, f"{p}02-deck")
    # text in place on slide 2
    go_to(app, 2)
    click_slide(app, 300, 200)
    click_slide(app, 300, 200)
    wait_until(lambda: any(w["ty"] == "TextInput" and "Highlights" in (w.get("t") or "") for w in app.widgets()), "no text field on slide 2's body")
    field = [w for w in app.widgets() if w["ty"] == "TextInput" and "Highlights" in (w.get("t") or "")][0]["r"]
    app.click(field[0] + field[2] - 8, field[1] + field[3] - 8)
    app.key("ReturnKey")
    app.type("Questions")
    app.find("Edited", "Label", timeout=30)
    app.grab(out, f"{p}03-text")
    click_slide(app, 900, 520)
    # bold the box
    click_slide(app, 300, 200)
    app.find("Applies to", "Label", timeout=30)
    app.tap("B", "Label")
    app.grab(out, f"{p}04-bold")
    # a chart on slide 4
    go_to(app, 4)
    app.tap("Chart", "Label", below=70)
    app.tap("Insert chart", "Button")
    app.find("Elements", "Label", timeout=60)
    app.images(2)
    app.grab(out, f"{p}05-chart")
    # undo and redo
    app.tap("Undo", "Label")
    app.find("Redo", "Label")
    app.tap("Redo", "Label")
    # the layout and the theme
    seg(app, "Layout")
    app.find("Slide layout", "Label", timeout=60)
    app.tap("Title Only", "Label")
    seg(app, "Theme")
    app.tap("Ember", "Label")
    app.images(2, timeout=120)
    app.grab(out, f"{p}06-theme")
    if not dark:
        states_follow(app, out)
    # notes
    seg(app, "Notes")
    note = [w for w in app.widgets() if w["ty"] == "TextInput" and w["r"][0] > app.area()[2] - 360]
    x, y, w, h = note[0]["r"]
    app.click(x + w / 2, y + 30)
    app.type("Thank the members.")
    app.find("3 words  ·  about 1 second", "Label", timeout=20)
    # the sorter
    app.tap("Sorter", "Button")
    app.images(4, timeout=120)
    count = slide_count(app)
    tile_number(app, 4)  # the current slide starts chosen: unchoose it
    tile_number(app, 2)
    app.find("1 selected", "Label")
    app.tap("Duplicate", "Button")
    app.find(f"{count + 1} slides", "Label", exact=False, timeout=60)
    app.tap("Hide", "Button")
    app.tap("Section", "Button")
    app.grab(out, f"{p}07-sorter")
    app.tap("Slides", "Button")
    # save
    app.tap("Save", "Button")
    app.find("Saved", "Label", timeout=60)
    # export (the dark pass names its exports apart: an export never
    # replaces another)
    app.tap("Export", "Button")
    name = DECK + (" (dark)" if dark else "")
    if dark:
        fx, fy, fw, fh = seek(app, DECK, "TextInput")
        app.click(fx + fw - 6, fy + fh / 2)
        app.type(" (dark)")
        app.scroll(-3000)
    app.tap("Presentation", "Label")
    reach(app, "Export presentation")
    seek(app, name + ".pptx  ·  ", "Label", exact=False, timeout=120)
    app.scroll(-3000)
    app.tap("PDF", "Label")
    reach(app, "Export PDF")
    seek(app, name + ".pdf  ·  ", "Label", exact=False, timeout=180)
    app.grab(out, f"{p}08-export")
    app.scroll(-3000)
    app.tap("Export", "Button")
    # present
    go_to(app, 1)
    app.tap("Present", "Button")
    app.find("Exit", "Button", timeout=60)
    app.images(1, timeout=60)
    app.grab(out, f"{p}09-present")
    app.click(app.area()[2] / 2, app.area()[3] / 2)
    app.tap("Exit", "Button")


def states_follow(app, out):
    """The inspector shows what the engine reports, and follows a real change
    and its undo: a box's anchor and autofit, a chart element, a table option,
    the colour set and the footer (SERVICE.md "Inspector states (#507)").
    After each change the shape is chosen again, or the design read again, so
    the check is of what the engine reports, not of what the app showed at
    once; after each Undo, the same."""
    x, y, w, h = app.area()
    inspector = (x + w - 170, y + h / 2)
    seen = {}

    def settle():
        time.sleep(2.5)

    def choose_again(px, py, ready):
        click_slide(app, 930, 525)  # the slide's empty corner: nothing chosen
        app.gone(ready, timeout=30)
        settle()
        click_slide(app, px, py)
        app.find(ready, "Label", timeout=30)

    # A box: its vertical alignment and autofit (deck.format_state).
    seg(app, "Format")
    go_to(app, 2)
    click_slide(app, 300, 200)
    app.find("Applies to", "Label", timeout=30)
    seek(app, "Vertical alignment", "Label", at=inspector)
    expect("the box's anchor", lambda: segment(app, "Vertical alignment") >= 0)
    anchor, shrink = segment(app, "Vertical alignment"), switch_on(app, "Shrink text when it overflows")
    target = 2 if anchor != 2 else 0
    seen["box"] = {"anchor": ["top", "middle", "bottom"][anchor], "shrink": shrink, "set_anchor": ["top", "middle", "bottom"][target], "set_shrink": not shrink}
    lx, ly, lw, lh = app.find("Vertical alignment", "Label")["r"]
    row = sorted((v for v in app.widgets() if "selected" in v and ly + lh <= v["r"][1] <= ly + lh + 60), key=lambda v: v["r"][0])
    bx, by, bw, bh = row[target]["r"]
    app.click(bx + bw / 2, by + bh / 2)
    expect("the new anchor", lambda: segment(app, "Vertical alignment") == target)
    app.click(*switch_point(app, "Shrink text when it overflows"))
    expect("the box's autofit changed", lambda: switch_on(app, "Shrink text when it overflows") != shrink)
    choose_again(300, 200, "Applies to")
    seek(app, "Vertical alignment", "Label", at=inspector)
    expect("the engine's new anchor and autofit", lambda: segment(app, "Vertical alignment") == target
           and switch_on(app, "Shrink text when it overflows") != shrink)
    app.grab(out, "10-box")
    app.tap("Undo", "Label")
    app.tap("Undo", "Label")
    settle()
    seek(app, "Vertical alignment", "Label", at=inspector)
    expect("the anchor and autofit back after Undo", lambda: segment(app, "Vertical alignment") == anchor
           and switch_on(app, "Shrink text when it overflows") == shrink)
    settle()
    expect("the anchor and autofit still back", lambda: segment(app, "Vertical alignment") == anchor
           and switch_on(app, "Shrink text when it overflows") == shrink)
    app.grab(out, "10b-box-undone")

    # A chart element: data labels (deck.slide's chart).
    go_to(app, 4)
    app.images(2, timeout=60)
    click_slide(app, 480, 300)
    app.find("Elements", "Label", timeout=30)
    seek(app, "Data labels", "Label", at=inspector)
    labels = switch_on(app, "Data labels")
    seen["chart"] = {"title": switch_on(app, "Title"), "legend": switch_on(app, "Legend"), "data_labels": labels, "gridlines": switch_on(app, "Gridlines")}
    app.click(*switch_point(app, "Data labels"))
    expect("the data labels changed", lambda: switch_on(app, "Data labels") != labels)
    choose_again(480, 300, "Elements")
    seek(app, "Data labels", "Label", at=inspector)
    expect("the engine's data labels", lambda: switch_on(app, "Data labels") != labels)
    app.images(2, timeout=60)
    app.grab(out, "11-chart")
    app.tap("Undo", "Label")
    settle()
    seek(app, "Data labels", "Label", at=inspector)
    expect("the data labels back after Undo", lambda: switch_on(app, "Data labels") == labels)
    settle()
    expect("the data labels still back", lambda: switch_on(app, "Data labels") == labels)
    app.grab(out, "11b-chart-undone")

    # A table option: the total row (deck.slide's table).
    go_to(app, 3)
    app.tap("Table", "Label", below=70)
    app.tap("Insert table", "Button")
    app.find("Header row", "Label", timeout=60)
    seek(app, "Total row", "Label", at=inspector)
    total = switch_on(app, "Total row")
    seen["table"] = {"header_row": switch_on(app, "Header row"), "total_row": total, "banded_rows": switch_on(app, "Banded rows"),
                     "first_column": switch_on(app, "First column")}
    app.click(*switch_point(app, "Total row"))
    expect("the total row changed", lambda: switch_on(app, "Total row") != total)
    # The engine centres a new table on the slide (deckcraft insert.table).
    choose_again(480, 270, "Header row")
    seek(app, "Total row", "Label", at=inspector)
    expect("the engine's total row", lambda: switch_on(app, "Total row") != total)
    app.images(2, timeout=60)
    app.grab(out, "12-table")
    app.tap("Undo", "Label")
    settle()
    seek(app, "Total row", "Label", at=inspector)
    expect("the total row back after Undo", lambda: switch_on(app, "Total row") == total)
    settle()
    expect("the total row still back", lambda: switch_on(app, "Total row") == total)
    app.grab(out, "12b-table-undone")
    app.tap("Undo", "Label")  # and the table goes
    app.gone("Header row", timeout=30)

    # The colour set (deck.state): Ocean, then Undo back to the theme's own.
    seg(app, "Theme")
    app.find("Colours", "Label", timeout=30)
    expect("the theme's colour set marked", lambda: "Ember" in selected_texts(app))
    seen["colours"] = sorted(t for t in selected_texts(app) if t != "Title")
    ox, oy, ow, oh = seek(app, "Ocean", "Label", at=inspector)
    app.click(ox + ow / 2, oy - 8)
    expect("the Ocean colour set marked", lambda: "Ocean" in selected_texts(app), timeout=60)
    settle()
    expect("the engine's colour set, Ocean", lambda: "Ocean" in selected_texts(app))
    app.images(2, timeout=60)
    app.grab(out, "13-colours")
    app.tap("Undo", "Label")
    expect("Ocean unmarked after Undo", lambda: "Ocean" not in selected_texts(app), timeout=60)
    app.scroll(-3000, at=inspector)
    expect("the theme's colour set marked again after Undo", lambda: "Ember" in selected_texts(app), timeout=30)
    app.grab(out, "13b-colours-undone")

    # The footer (deck.state): slide numbers, then Undo.
    seek(app, "Slide numbers", "Label", at=inspector)
    numbers = switch_on(app, "Slide numbers")
    seen["footer"] = {"slide_numbers": numbers, "date": switch_on(app, "Date")}
    app.click(*switch_point(app, "Slide numbers"))
    expect("the slide numbers changed", lambda: switch_on(app, "Slide numbers") != numbers, timeout=60)
    settle()
    expect("the engine's slide numbers", lambda: switch_on(app, "Slide numbers") != numbers)
    app.images(2, timeout=60)
    app.grab(out, "14-footer")
    app.tap("Undo", "Label")
    settle()
    seek(app, "Slide numbers", "Label", at=inspector)
    expect("the slide numbers back after Undo", lambda: switch_on(app, "Slide numbers") == numbers, timeout=60)
    settle()
    expect("the slide numbers still back", lambda: switch_on(app, "Slide numbers") == numbers)
    app.grab(out, "14b-footer-undone")
    (out / "states.json").write_text(json.dumps(seen, indent=1))
    app.scroll(-3000, at=inspector)
    go_to(app, 4)  # where the journey was: the rest of it starts from slide 4


def restart(app, out):
    """The same home again: the deck in the library with its cover and the
    slide count the last save wrote, opened at the slide it was left at."""
    app.find("Recent", "Label", timeout=90)
    seek(app, " slides  ·  ", "Label", exact=False, timeout=90)  # a short window shows the card's top only
    card = [w["t"] for w in app.widgets() if " slides  ·  " in (w.get("t") or "")][0]
    count = int(card.split(" ")[0])
    app.images(1, timeout=120)
    app.grab(out, "r01-home")
    open_card(app, DECK)
    app.find(f" of {count}", "Label", exact=False, timeout=120)
    app.images(2, timeout=120)
    app.grab(out, "r02-deck")
    (out / "restart.json").write_text(json.dumps({"card": card}, indent=1))


# ---------------------------------------------------------------- runs

def compare(out, names):
    """Each grab beside its design, the same height: compare/NN-name.png."""
    from PIL import Image
    folder = out / "compare"
    folder.mkdir(exist_ok=True)
    for design, grab in names:
        if not (out / f"{grab}.png").exists():
            continue
        a = Image.open(DESIGNS / f"{design}.png").convert("RGB")
        b = Image.open(out / f"{grab}.png").convert("RGB")
        b = b.resize((int(b.width * a.height / b.height), a.height))
        sheet = Image.new("RGB", (a.width + b.width + 12, a.height), "#ff00ff")
        sheet.paste(a, (0, 0))
        sheet.paste(b, (a.width + 12, 0))
        sheet.save(folder / f"{grab}.png")


def keep_failure(app, out, name):
    try:
        app.grab(out, f"failed-{name}")
        (out / f"failed-{name}.snap.json").write_text(json.dumps(app.remote("snap"), indent=1))
    except Exception as error:
        print("could not keep the failure:", error)


def check_log(log):
    # "script_result dropped: widget … not in the widget tree" is the runtime
    # saying a region's render answered after the region was made again (the
    # navigator's and the inspector's regions live inside regions that render
    # too): nothing is lost, so it doesn't fail a run.
    errors = [line for line in log.read_text(errors="replace").splitlines()
              if ("[E]" in line and ("splash" in line.lower() or "script" in line.lower() or "quickdeck" in line.lower())
                  or "on_render closure failed" in line or "callback error" in line or "script time budget exceeded" in line)
              and "not in the widget tree" not in line]
    assert not errors, f"script errors in {log}:\n" + "\n".join(errors)


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


def dev_bundle(out, source=None):
    """A scratch copy of the bundle (or `source`) with the stand-in engine in
    place of engine()."""
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(source or BUNDLE, out)
    main = out / "main.splash"
    text = main.read_text()
    start = text.index("fn engine(method, args, done){")
    end = text.index("\n}\n", start) + 3
    if 'host.request("deck." + method' not in text[start:end] or text.count("fn engine(method, args, done){") != 1:
        raise SystemExit("engine() in main.splash is not what this script replaces: update dev_bundle()")
    main.write_text(text[:start] + (FIXTURE / "engine.splash").read_text().rstrip() + "\n" + text[end:])
    return out


def make_fixture(jail, *flags):
    subprocess.run([sys.executable, str(FIXTURE / "make_fixture.py"), str(jail), *flags], check=True, stdout=subprocess.DEVNULL)


DESIGNED = [("01-home", "01-home"), ("02-edit-text", "02-edit-text"), ("03-chart", "03-chart"), ("04-layout", "04-layout"),
            ("05-theme", "05-theme"), ("06-sorter", "06-sorter"), ("07-outline", "07-outline"), ("08-present", "08-present"),
            ("09-export", "09-export"), ("10-dark-notes", "10-notes")]


def card_host_runs(args, runs):
    binary = args.card_host.resolve()
    assert binary.is_file(), f"build App Hub's card-host first: {binary}"
    assert port_free(args.port), f"port {args.port} is taken"
    with tempfile.TemporaryDirectory(prefix="quickdeck-card-host-", dir=args.work) as scratch:
        scratch = Path(scratch)
        bundle = dev_bundle(scratch / "bundle", args.bundle)

        def session(name, which, flags, steps, dark):
            data = scratch / f"{name}-{'dark' if dark else 'light'}"
            make_fixture(data / APP, *flags)
            out = args.output / name
            out.mkdir(parents=True, exist_ok=True)
            log = out / ("card-host-dark.log" if dark else "card-host.log")
            app = CardHost(binary, which, data, args.port, "macos-dark" if dark else "macos", log)
            try:
                steps(app, out, dark)
            except BaseException:
                keep_failure(app, out, ("dark-" if dark else "light-") + name)
                raise
            finally:
                app.quit()
            check_log(log)

        for dark in (False, True):
            if "missing" in runs:
                session("missing", BUNDLE, [], missing, dark)
            if "fixture" in runs:
                session("fixture", bundle, [], fixture_journey, dark)
                compare(args.output / "fixture", [(design, ("d" if dark else "") + grab) for design, grab in DESIGNED])
            if "states" in runs:
                session("states", bundle, ["--states"], fixture_states, dark)
                session("states-full", bundle, ["--full"], full, dark)
                session("states-empty", bundle, ["--empty"], empty, dark)


def shell_runs(args, runs):
    binary = args.shell.resolve()
    assert binary.is_file(), f"build the shell first: {binary}"
    scratch = Path(tempfile.mkdtemp(prefix="quickdeck-shell-", dir=args.work))
    print("homes in", scratch)

    def session(name, home, steps):
        deadline = time.monotonic() + 1800
        while not LINUX and octosense_running():
            assert time.monotonic() < deadline, "another OctoSense is still running: quit it first"
            print("waiting for another OctoSense to quit", flush=True)
            time.sleep(20)
        assert port_free(args.port), f"port {args.port} is taken"
        out = args.output / name
        out.mkdir(parents=True, exist_ok=True)
        log = out / "shell.log"
        app = Shell(binary, home, args.port, log)
        try:
            app.find("Quick Deck", "Label", timeout=90)
            (out / "snap.json").write_text(json.dumps(app.remote("snap"), indent=1))
            steps(app, out)
        except BaseException:
            keep_failure(app, out, name)
            raise
        finally:
            app.quit()
        check_log(log)

    with Lock(args.lock):
        home = scratch / "home"
        if "shell" in runs:
            def light_then_dark(app, out):
                shell_journey(app, out, dark=False)
                app.style("octosense-dark", "octosense dark")
                shell_journey(app, out, dark=True)
            session("shell", home, light_then_dark)
        if "restart" in runs:
            session("restart", home, restart)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--shell", type=Path, help="the desktop shell binary (target/debug/octosense)")
    parser.add_argument("--card-host", type=Path, help="App Hub's card-host, for the runs without host services")
    parser.add_argument("--lock", type=Path, default=Path(tempfile.gettempdir()) / "octosense-shell.lock",
                        help="the folder that marks a hidden shell run on this machine")
    parser.add_argument("--port", type=int, default=0, help="the remote bridge's port (8916 for a shell, 8912 for card-host)")
    parser.add_argument("--output", type=Path, default=ROOT / "target/quickdeck-ui")
    parser.add_argument("--work", type=Path, help="where the runs' homes go (default: the system's temporary folder)")
    parser.add_argument("--only", choices=["shell", "restart", "missing", "fixture", "states"], action="append")
    parser.add_argument("--budget-ms", type=int, help="MAKEPAD_SPLASH_BUDGET_MS for the hosts (default: the runtime's 64 ms); for a loaded machine")
    parser.add_argument("--grab-scale", type=float, default=0.5, help="the grabs' scale (the bridge's /g scale): 0.5 is one pixel per point on a 2x screen")
    parser.add_argument("--bundle", type=Path, help="another bundle for the card-host runs (default: apps/quickdeck/bundle), for a before/after comparison")
    parser.add_argument("--grabs-only", action="store_true", help="take the frames without checking the inspector's states (for another bundle)")
    args = parser.parse_args()
    Driver.scale = args.grab_scale
    Driver.budget_ms = args.budget_ms
    Driver.check_states = not args.grabs_only
    card_host_only = ("missing", "fixture", "states")
    runs = args.only or (["shell", "restart"] if args.shell else []) + (list(card_host_only) if args.card_host else [])
    assert runs, "give --shell, --card-host or both"
    if any(run not in card_host_only for run in runs):
        assert args.shell, "these runs need --shell"
        args.port = args.port or 8916
        shell_runs(args, runs)
    if any(run in card_host_only for run in runs):
        assert args.card_host, "the missing, fixture and states runs need --card-host"
        args.port = 8912 if args.port in (0, 8916) else args.port
        card_host_runs(args, runs)
    print("all journeys passed; grabs in", args.output)


if __name__ == "__main__":
    main()

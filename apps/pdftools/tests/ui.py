#!/usr/bin/env python3
"""PDF Tools in App Hub's card-host, hidden, driven over Makepad's remote bridge.

card-host serves no host services, so the app runs on its developer fixture:
the sample PDFs and the pdf service's recorded answers that
octosense-pdf-service's pdftools_fixture example writes (see
apps/pdftools/README.md). The journey walks every screen in light and dark,
then the engine-refused state (the samples without the fixture) and the empty
library, saving each original /g grab. Nothing here calls the real engine.

Run from the repository root:
  cargo run --locked -p octosense-pdf-service --example pdftools_fixture -- target/pdftools-fixture
  python3 apps/pdftools/tests/ui.py --card-host <App Hub>/target/release/card-host \\
      --fixture target/pdftools-fixture --output target/pdftools-ui
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
from urllib.parse import urlencode
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[3]
BUNDLE = ROOT / "apps/pdftools/bundle"
APP = "os.pdftools"
ICON_UP = ""


class App:
    def __init__(self, card_host, data, port, style, log_path):
        self.port = port
        env = os.environ.copy()
        env.update(MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE=f"127.0.0.1:{port}", MAKEPAD_WIDGET_STYLE=style)
        self.log = log_path.open("w")
        self.process = subprocess.Popen([str(card_host), "--bundle", str(BUNDLE), "--system", "--app-data", str(data)],
                                        cwd=ROOT, env=env, stdout=self.log, stderr=self.log)

    def remote(self, route, **query):
        url = f"http://127.0.0.1:{self.port}/{route}"
        if query:
            url += "?" + urlencode(query)
        with urlopen(url, timeout=20) as response:
            data = response.read()
        try:
            return json.loads(data)
        except ValueError:
            return data

    def widgets(self):
        return [w for w in self.remote("snap")["s"] if w["ty"] != "Splash" and w["r"][2] > 0 and w["r"][3] > 0]

    def find(self, text, kind=None, below=0.0, timeout=20.0, exact=True):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                found = [w for w in self.widgets()
                         if (w.get("t") == text if exact else text in (w.get("t") or ""))
                         and (kind is None or w["ty"] == kind) and w["r"][1] >= below]
                if found:
                    buttons = [w for w in found if w["ty"] in ("Button", "ButtonFlat")]
                    return (buttons or found)[0]
            except (OSError, ValueError):
                pass
            time.sleep(0.15)
        raise AssertionError(f"no visible {kind or 'widget'} {text!r} below y={below}")

    def tap(self, text, kind=None, below=0.0, exact=True):
        x, y, w, h = self.find(text, kind, below, exact=exact)["r"]
        self.remote("click", x=x + w / 2, y=y + h / 2, wait=1)
        time.sleep(0.35)

    def reveal(self, text, kind="Label", tries=6):
        """Scroll down until `text` is on screen, clear of the bottom bar."""
        for _ in range(tries):
            try:
                x, y, w, h = self.find(text, kind, timeout=1.0)["r"]
                if y + h < 812:
                    return
            except AssertionError:
                pass
            self.scroll(240)
        raise AssertionError(f"could not scroll {text!r} into view")

    def tap_above(self, text, rise, below=0.0):
        """Tap `rise` points above a label (a page tile above its number)."""
        x, y, w, h = self.find(text, "Label", below)["r"]
        self.remote("click", x=x + w / 2, y=y - rise, wait=1)
        time.sleep(0.35)

    def type(self, text):
        self.remote("t", t=text, wait=1)
        time.sleep(0.5)

    def scroll(self, dy):
        self.remote("m", k="scroll", x=206, y=600, dy=dy, wait=1)
        time.sleep(0.4)

    def grab(self, out, name):
        time.sleep(0.6)
        path = out / f"{name}.png"
        path.write_bytes(self.remote("g", raw=1))
        print("  ", path.relative_to(out.parent) if out.parent in path.parents else path)

    def quit(self):
        try:
            result = self.remote("gq")
            assert result.get("quit") == 1, result
        except OSError:
            pass
        try:
            self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)
            raise AssertionError("card-host did not exit after /gq")
        finally:
            self.log.close()


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def port_free(port):
    with socket.socket() as probe:
        return probe.connect_ex(("127.0.0.1", port)) != 0


def stage(fixture, data, kind):
    """A fresh app-data folder: the whole fixture, the samples alone, or nothing."""
    if data.exists():
        shutil.rmtree(data)
    data.mkdir(parents=True)
    jail = fixture / APP
    if kind == "fixture":
        shutil.copytree(jail, data / APP)
    elif kind == "samples":
        library = data / APP / "accounts/device/library"
        shutil.copytree(jail / "accounts/device/library", library)
        for leftover in library.iterdir():
            if leftover.name == "Damaged scan.pdf":
                leftover.unlink()


def journey(app, out, split_where):
    app.find("PDF Tools", "Label")
    app.find("Quarterly report", "Label")
    app.find("4 pages  ·  9 KB", "Label")
    app.grab(out, "01-library")
    app.tap("Open from this device", "Label")
    app.find("For now, PDF Tools works on the PDFs in its own storage.", "Label", exact=False)
    app.grab(out, "02-open-from-device")
    app.tap("OK")

    app.tap("Field guide", "Label")
    app.find("7 pages  ·  16 KB", "Label")
    app.find("Split into files")
    app.grab(out, "03-pages")
    app.tap_above("2", 120, below=300)
    app.find("Page 2 of 7", "Label")
    app.grab(out, "04-page")
    app.tap("Next")
    app.find("Page 3 of 7", "Label")
    app.grab(out, "05-page-next")
    app.tap("‹ Pages")

    app.tap("Text")
    app.find("PAGE 1", "Label")
    app.grab(out, "06-text")
    app.tap("Find in the text", "TextInput")
    app.type("wren")
    app.find("2 pages contain “wren”.", "Label")
    app.grab(out, "07-text-find")
    app.tap("Info")
    app.find("A4, 210 × 297 mm", "Label")
    app.grab(out, "08-info")
    app.scroll(900)
    app.find("Not protected", "Label")
    app.find("Helvetica, Helvetica-Bold, Times-Italic, Times-Roman", "Label")
    app.grab(out, "08b-info-end")

    app.tap("Pages")
    app.tap("Split into files")
    app.find("Makes 4 files", "Label")
    app.grab(out, "09-split-every")
    app.tap("Where I choose")
    app.find("Tap a page to split there")
    app.tap_above("2", 120, below=300)
    app.find("File 2 starts here", "Label")
    app.find("Split into 2 files")
    app.grab(out, "10-split-where")
    if split_where:
        app.tap("Split into 2 files")
        app.find("Split into 2 files", "Label")
        app.find("Pages 2–7", "Label")
    else:
        app.tap("Every few pages")
        app.tap("Split into 4 files")
        app.find("Split into 4 files", "Label")
        app.find("Page 7", "Label")
    app.grab(out, "11-split-done")
    app.tap("Done")

    app.find("Field guide-part2", "Label")
    app.tap("Merge PDFs")
    app.reveal("Quarterly report")
    app.tap("Quarterly report", "Label")
    app.scroll(-2000)
    app.tap("Board minutes", "Label")
    app.find("Next: order 2 files")
    app.grab(out, "12-merge-choose")
    app.tap("Next: order 2 files")
    app.find("One PDF of 6 pages, saved in your library.", "Label")
    app.grab(out, "13-merge-order")
    board = app.find("Board minutes", "Label")["r"]
    up = [w for w in app.widgets() if w.get("t") == ICON_UP and abs(w["r"][1] - board[1]) < 30]
    assert up, "no move-up button on the Board minutes row"
    x, y, w, h = up[0]["r"]
    app.remote("click", x=x + w / 2, y=y + h / 2, wait=1)
    time.sleep(0.4)
    app.tap("Merge 2 PDFs")
    app.find("Merged into one PDF", "Label")
    app.find("6 pages  ·  13 KB", "Label")
    app.grab(out, "14-merge-done")
    app.tap("Open")
    app.find("Merged", "Label")
    app.find("6 pages  ·  13 KB", "Label")
    app.grab(out, "15-merged-pages")
    app.tap("‹ Library")

    app.tap("Damaged scan", "Label")
    app.find("Couldn't open this PDF", "Label")
    app.grab(out, "16-damaged")
    app.tap("‹ Library")


def restart(app, out):
    """The same storage after a restart: what merge and split wrote is still there."""
    app.find("Merged", "Label")
    app.find("6 pages  ·  13 KB", "Label")
    app.grab(out, "17-restart-library")
    app.tap("Merged", "Label")
    app.find("Split into files")
    app.find("1", "Label", below=300)
    app.grab(out, "18-restart-merged")
    app.tap("‹ Library")


def engine_missing(app, out):
    app.find("PDF Tools can't use the PDF engine yet", "Label")
    app.grab(out, "19-engine-missing-library")
    app.tap("Field guide", "Label")
    app.find("PDF Tools can't use the PDF engine yet", "Label")
    app.grab(out, "20-engine-missing-document")
    app.tap("‹ Library")
    app.tap("Merge PDFs")
    app.find("PDF Tools can't use the PDF engine yet", "Label", below=600)
    app.grab(out, "21-engine-missing-merge")


def empty(app, out):
    app.find("No PDFs here yet", "Label")
    app.grab(out, "22-empty")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--card-host", type=Path, default=os.environ.get("OCTO_CARD_HOST"), required=os.environ.get("OCTO_CARD_HOST") is None)
    parser.add_argument("--fixture", type=Path, required=True, help="the app-data folder pdftools_fixture wrote")
    parser.add_argument("--output", type=Path, default=ROOT / "target/pdftools-ui")
    parser.add_argument("--port", type=int, default=0, help="the remote bridge's port (default: a free one)")
    parser.add_argument("--only", choices=["light", "dark", "missing", "empty"], action="append")
    args = parser.parse_args()
    card_host = args.card_host.resolve()
    assert card_host.is_file(), f"build App Hub's card-host first: {card_host}"
    assert (args.fixture / APP / "dev/engine-replay.json").is_file(), f"run the pdftools_fixture example first: {args.fixture}"
    port = args.port or free_port()
    assert port_free(port), f"port {port} is taken: quit what holds it first"
    runs = args.only or ["light", "dark", "missing", "empty"]
    with tempfile.TemporaryDirectory(prefix="pdftools-ui-") as scratch:
        data = Path(scratch) / "app-data"
        for run in runs:
            out = args.output / run
            out.mkdir(parents=True, exist_ok=True)
            style = "macos-dark" if run == "dark" else "macos"
            stage(args.fixture, data, {"light": "fixture", "dark": "fixture", "missing": "samples", "empty": "none"}[run])
            print(f"{run} ({style}):")
            logs = [out / "card-host.log"]
            app = App(card_host, data, port, style, logs[0])
            try:
                if run in ("light", "dark"):
                    journey(app, out, split_where=run == "light")
                elif run == "missing":
                    engine_missing(app, out)
                else:
                    empty(app, out)
            finally:
                app.quit()
            if run == "light":
                # Restart on the same storage.
                logs.append(out / "card-host-restart.log")
                app = App(card_host, data, port, style, logs[1])
                try:
                    restart(app, out)
                finally:
                    app.quit()
            for log in logs:
                errors = [line for line in log.read_text(errors="replace").splitlines()
                          if "[E]" in line or "on_render closure failed" in line or "callback error" in line]
                assert not errors, f"script errors in {log.name}:\n" + "\n".join(errors)
    print("all journeys passed; grabs in", args.output)


if __name__ == "__main__":
    main()

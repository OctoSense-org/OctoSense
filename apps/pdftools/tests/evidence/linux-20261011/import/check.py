#!/usr/bin/env python3
"""One-off check, kept as run (Linux only): "Open a PDF from this device"
through makepad's Linux file dialog, which runs the desktop's own helper
(zenity, qarma, matedialog or kdialog). The build host has none, so a
stand-in for zenity goes first on the shell's PATH. It records how it was
asked and answers with one real PDF. Everything after it is shipped code:
the helper lookup, its command line, the parsing of its reply, the byte
loader, the files service's import into the app's storage, and PDF Tools'
title for the new file. ui.py's own Linux launcher (a private headless
weston) and driver run the shell.

  python3 apps/pdftools/tests/evidence/linux-20261011/import/check.py \
      --shell target/debug/octosense --output <dir>
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / "apps/pdftools/tests"))
import ui  # noqa: E402

parser = argparse.ArgumentParser()
parser.add_argument("--shell", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--port", type=int, default=8915)
args = parser.parse_args()
assert ui.LINUX, "the stand-in replaces a Linux dialog helper"
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
home = out / "home"
jail = home / "apps" / ui.APP
ui.fixture(home / "apps")
chosen = out / "chosen" / "Lease 2026.pdf"
chosen.parent.mkdir(exist_ok=True)
shutil.copy(jail / ui.LIBRARY / "Field guide.pdf", chosen)
stub = out / "bin" / "zenity"
stub.parent.mkdir(exist_ok=True)
stub.write_text("#!/bin/sh\n"
                f"printf '%s\\n' \"$@\" > '{out}/zenity-args.txt'\n"
                f"printf '%s\\n' '{chosen}'\n")
stub.chmod(0o755)
os.environ["PATH"] = f"{stub.parent}:{os.environ['PATH']}"

app = ui.Shell(args.shell.resolve(), home, args.port, out / "shell.log")
seen = {}
try:
    app.find("PDF Tools", "Label", timeout=90)
    app.see("Quarterly report.pdf")
    app.images(3)
    app.scroll(-3000)  # back to the top: the button row
    app.find("Up to 64 MB", "Label")  # files.status: import_supported, max_import_bytes
    app.grab(out, "i01-home-import-offered")
    app.tap("Open a PDF from this device", "Button")
    # The new tab: the title is the chosen file's name less ".pdf", and a
    # title shows as title + ".pdf" (main.splash shown_name).
    app.find("Lease 2026.pdf", "Label", timeout=60)
    app.find(" / ", "Label", exact=False, timeout=60)
    app.images(1)
    app.grab(out, "i02-imported-and-opened")
    seen["page_label"] = next(w["t"] for w in app.widgets() if " / " in (w.get("t") or ""))
    app.tap("Open", "Button")
    app.see("Lease 2026.pdf")
    app.scroll(-3000)
    app.grab(out, "i03-home-with-the-import")
except BaseException:
    ui.keep_failure(app, out, "import")
    raise
finally:
    app.quit()
ui.check_log(out / "shell.log")
index = json.loads((jail / "accounts/device/library.json").read_text())["docs"]
imported = jail / ui.LIBRARY / "Imported PDF.pdf"
seen.update({
    "zenity_args": (out / "zenity-args.txt").read_text().splitlines(),
    "library": sorted(p.name for p in (jail / ui.LIBRARY).glob("*.pdf")),
    "imported_file": {"name": imported.name, "bytes": imported.stat().st_size,
                      "same_bytes_as_chosen": imported.read_bytes() == chosen.read_bytes()},
    "library_json_entry": index.get("Imported PDF.pdf"),
})
(out / "import.json").write_text(json.dumps(seen, indent=1))
print(json.dumps(seen, indent=1))

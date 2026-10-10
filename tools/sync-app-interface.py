#!/usr/bin/env python3
"""Keep shipped Splash bundles self-contained with one shared interface prelude."""
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
START = "// BEGIN shared app interface\n"
END = "// END shared app interface\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    prelude = START + (ROOT / "apps/interface.splash").read_text() + END
    stale = []
    for name in ("news", "photos", "mail", "calendar", "maps", "ai-providers", "youtube", "wasmlab", "quickdeck", "pdftools"):
        path = ROOT / "apps" / name / "bundle/main.splash"
        text = path.read_text()
        body = text.split(END, 1)[1] if text.startswith(START) else text
        expected = prelude + body
        if text != expected:
            stale.append(str(path.relative_to(ROOT)))
            if not args.check:
                path.write_text(expected)
    if args.check and stale:
        parser.exit(1, "Run tools/sync-app-interface.py: " + ", ".join(stale) + "\n")


if __name__ == "__main__":
    main()

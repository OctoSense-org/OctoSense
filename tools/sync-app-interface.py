#!/usr/bin/env python3
"""Keep shipped Splash bundles self-contained with one shared interface prelude and icon family."""
import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
START = "// BEGIN shared app interface\n"
END = "// END shared app interface\n"
APPS = ("news", "photos", "mail", "calendar", "maps", "ai-providers", "youtube", "wasmlab", "quickdeck", "pdftools", "writer", "settings")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    prelude = START + (ROOT / "apps/interface.splash").read_text() + END
    icons = {p.name: p.read_bytes() for p in (ROOT / "apps/interface-icons").glob("*.svg")}
    stale = []
    for name in APPS:
        bundle = ROOT / "apps" / name / "bundle"
        path = bundle / "main.splash"
        text = path.read_text()
        body = text.split(END, 1)[1] if text.startswith(START) else text
        expected = prelude + body
        if text != expected:
            stale.append(str(path.relative_to(ROOT)))
            if not args.check:
                path.write_text(expected)
        directory = bundle / "icons"
        wanted = {"ui-" + icon: data for icon, data in icons.items()}
        have = {p.name: p.read_bytes() for p in directory.glob("ui-*.svg")} if directory.is_dir() else {}
        if have != wanted:
            stale.append(str(directory.relative_to(ROOT)))
            if not args.check:
                directory.mkdir(exist_ok=True)
                for icon in have.keys() - wanted.keys():
                    (directory / icon).unlink()
                for icon, data in wanted.items():
                    (directory / icon).write_bytes(data)
    if args.check and stale:
        parser.exit(1, "Run tools/sync-app-interface.py: " + ", ".join(stale) + "\n")


if __name__ == "__main__":
    main()

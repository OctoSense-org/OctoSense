#!/usr/bin/env python3
"""Render C4's night dial with C3's four-spoke compass rose."""
from pathlib import Path
import importlib.util
import subprocess

OUT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("compass", OUT / "build.py")
compass = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compass)
studies = compass.studies
family = studies.family
circle, line, path = compass.circle, compass.line, compass.path
IVORY, MINT = compass.IVORY, compass.MINT
INK = "#233d40"

art = (
    compass.SHADOW
    + circle(32, 31.5, 21, MINT)
    + circle(32, 31.5, 19, INK)
    + line("M32 15.5 V17 M32 46 V47.5 M16 31.5 H17.5 M46.5 31.5 H48", "#8abcb5", 1.8)
    + compass.group(
        path("M32 17 L36 28 L47 32 L36 36 L32 47 L28 36 L17 32 L28 28 Z", "#a8d1c8")
        + path("M32 17 L36 28 L32 32 L28 28 Z", IVORY)
        + path("M32 32 L36 36 L32 47 Z", "#74afa7")
        + circle(32, 32, 2, INK),
        "translate(0 -0.5) rotate(30 32 32)",
    )
)


def build():
    svg = studies.master("C5 / Night rose", art)
    bounds = studies.verify(svg)
    (OUT / "c5-night-rose.svg").write_text(svg)
    text, image, rect = family.text, family.image, family.rect
    body = rect(0, 0, 960, 770, "#f6f4ed")
    body += text("Browser / Night rose", 40, 54, 30, INK, 'font-weight="600"')
    body += text("C4 night dial + C3 four-spoke rose", 40, 84, 16, "#63736e")
    left = (OUT / "c3-compass-rose.svg").read_text()
    right = (OUT / "c4-night-dial.svg").read_text()
    for item, x, y, size in ((left, 60, 170, 160), (svg, 352, 130, 256), (right, 740, 170, 160)):
        body += image(family.framed(item, "macos"), x, y, size)
    for label, x, y in (("C3 / Four spokes", 140, 365), ("C5 / Night rose", 480, 420), ("C4 / Night dial", 820, 365)):
        body += text(label, x, y, 18, INK, 'text-anchor="middle" font-weight="600"')
    body += text("Ivory north point · mint rose · ink dial", 480, 449, 14, "#63736e", 'text-anchor="middle"')
    body += text("Small sizes", 40, 506, 15, INK, 'font-weight="600"')
    for x, size in ((50, 64), (145, 40), (215, 24)):
        body += image(family.framed(svg, "macos"), x, 526 + (64-size)/2, size)
        body += text(str(size), x+size/2, 611, 12, "#63736e", 'text-anchor="middle"')
    body += rect(320, 478, 280, 150, INK, 12)
    body += text("Dark ground", 340, 506, 13, "#cde9e4")
    for x, size in ((343, 64), (435, 40), (507, 24)):
        body += image(family.framed(svg, "macos"), x, 526 + (64-size)/2, size)
    body += text("Platform shapes", 645, 506, 15, INK, 'font-weight="600"')
    for x, style, label in ((650, "macos", "macOS"), (743, "android", "Android"), (836, "ios", "iOS")):
        body += image(family.framed(svg, style), x, 530, 56)
        body += text(label, x+28, 611, 12, "#63736e", 'text-anchor="middle"')
    body += text("Family context", 40, 671, 15, INK, 'font-weight="600"')
    neighbors = ["mail", None, "calendar", "sheets", "clock"]
    for i, name in enumerate(neighbors):
        item = svg if name is None else (studies.ROOT / f"docs/design/icon-family/assets/{name}.svg").read_text()
        body += image(family.framed(item, "macos"), 210+i*108, 657, 64)
    body += text("Static SVG comparison · platform frame approximations · C5 is the approved Browser replacement", 40, 750, 11, "#63736e")
    sheet = OUT / "night-rose-preview.svg"
    sheet.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="960" height="770" viewBox="0 0 960 770">{body}</svg>')
    subprocess.run(["rsvg-convert", "-o", str(OUT / "night-rose-preview.png"), str(sheet)], check=True)
    print(f"PASS: C5 Night rose SVG; central 66% foreground bounds {bounds}")


if __name__ == "__main__":
    build()

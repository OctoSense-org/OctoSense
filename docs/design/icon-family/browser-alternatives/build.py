#!/usr/bin/env python3
"""Build Browser design studies; does not update shipping artwork."""
from pathlib import Path
import importlib.util
import io
import subprocess
import xml.etree.ElementTree as ET

from PIL import Image

ROOT = Path(__file__).resolve().parents[4]
OUT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("family", ROOT / "tools/build_icon_family.py")
family = importlib.util.module_from_spec(spec)
spec.loader.exec_module(family)
rect, circle, line, path = family.rect, family.circle, family.line, family.path
IVORY = family.IVORY
TEAL = "#398b94"
MINT = "#cde9e4"
CORAL = "#ef956a"


def globe(cx, cy, radius, stroke=1.8, outline=False):
    edge = f'stroke="{TEAL}" stroke-width="{stroke}"' if outline else ""
    return (circle(cx, cy, radius, IVORY, edge)
            + line(f"M{cx-radius} {cy} H{cx+radius}", TEAL, stroke)
            + f'<ellipse cx="{cx}" cy="{cy}" rx="{radius*0.43}" ry="{radius}" fill="none" stroke="{TEAL}" stroke-width="{stroke}"/>')


STUDIES = [
    ("a-open-globe", "A / Open globe", "Simplest silhouette", globe(32, 32, 20, 2.2)),
    ("b-browser-window", "B / Browser window", "Most explicit browser cue",
     rect(13, 17, 40, 34, "#194f61", 5, 'opacity="0.18"')
     + rect(12, 15, 40, 34, IVORY, 5)
     + path("M17 15 H47 Q52 15 52 20 V25 H12 V20 Q12 15 17 15 Z", MINT)
     + rect(16, 18, 12, 4, IVORY, 2)
     + circle(46, 20, 1.6, CORAL)
     + globe(32, 36, 10, 1.6, outline=True)),
    ("c-orbit", "C / Orbit", "Distinctive exploration motif",
     family.group('<ellipse cx="32" cy="32" rx="23" ry="9" fill="none" stroke="#e5b28d" stroke-width="3"/>', 'rotate(-28 32 32)')
     + circle(32, 32, 16, IVORY)
     + path("M23 19 Q29 15 33 17 L33 23 L28 28 L22 27 L19 32 Q17 25 23 19 Z", MINT)
     + path("M36 31 L42 29 L47 32 Q46 41 39 46 L35 41 L36 37 L32 34 Z", MINT)
     + family.group(line("M9 32 C9 44 55 44 55 32", CORAL, 3), 'rotate(-28 32 32)')),
    ("d-linked-tabs", "D / Linked tabs", "A tactile, layered object",
     rect(13, 16, 32, 27, MINT, 4)
     + rect(17, 19, 12, 4, "#75b7b7", 2)
     + rect(22, 27, 31, 25, "#194f61", 4, 'opacity="0.18"')
     + rect(20, 25, 31, 25, IVORY, 4)
     + path("M24 25 H47 Q51 25 51 29 V33 H20 V29 Q20 25 24 25 Z", "#f4c9ae")
     + rect(24, 28, 11, 2, IVORY, 1)
     + globe(35.5, 41, 6.5, 1.3, outline=True)),
]


def master(label, art):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
            f'<title>Browser alternative — {label}</title>'
            '<defs><linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">'
            '<stop stop-color="#45aec2"/><stop offset="1" stop-color="#227a94"/>'
            '</linearGradient></defs>' + rect(0, 0, 64, 64, "url(#tile)")
            + f'<g id="artwork" transform="translate(32 32) scale(0.9) translate(-32 -32)">{art}</g></svg>\n')


def verify(svg):
    root = ET.fromstring(svg)
    for child in list(root):
        if child.tag.endswith("rect"):
            root.remove(child)
    png = subprocess.check_output(["rsvg-convert", "-w", "1024", "-h", "1024"], input=ET.tostring(root))
    bounds = Image.open(io.BytesIO(png)).convert("RGBA").getchannel("A").getbbox()
    bounds = tuple(v / 16 for v in bounds)
    assert min(bounds[:2]) >= 10.88 and max(bounds[2:]) <= 53.12, bounds
    return bounds


def build(title="Browser / four directions", reference=None, reference_label="Current"):
    icons = []
    for name, label, note, art in STUDIES:
        svg = master(label, art)
        bounds = verify(svg)
        (OUT / f"{name}.svg").write_text(svg)
        icons.append(svg)
        print(f"PASS: {name}: foreground within central 66% {bounds}")
    text, image = family.text, family.image
    body = rect(0, 0, 1120, 930, "#f6f4ed")
    body += text(title, 40, 54, 30, "#233d40", 'font-weight="600"')
    body += text("OctoSense icon family · design studies", 40, 84, 16, "#63736e")
    current = reference or (ROOT / "docs/design/icon-family/assets/browser.svg").read_text()
    body += image(family.framed(current, "macos"), 983, 22, 60)
    body += text(reference_label, 1013, 102, 12, "#63736e", 'text-anchor="middle"')
    for i, ((name, label, note, art), svg) in enumerate(zip(STUDIES, icons)):
        x = 40 + i * 275
        body += text(label, x, 156, 20, "#233d40", 'font-weight="600"')
        body += text(note, x, 182, 14, "#63736e")
        framed = family.framed(svg, "macos")
        body += image(framed, x + 42, 208, 160)
        for dx, size in ((24, 64), (114, 40), (182, 24)):
            body += image(framed, x + dx, 400 + (64-size)/2, size)
            body += text(str(size), x + dx + size/2, 490, 12, "#63736e", 'text-anchor="middle"')
    body += rect(0, 515, 1120, 130, "#233d40")
    body += text("Dark ground", 40, 544, 12, "#b7cecb")
    for i, svg in enumerate(icons):
        for dx, size in ((24, 64), (114, 40), (182, 24)):
            body += image(family.framed(svg, "macos"), 40+i*275+dx, 563+(64-size)/2, size)
    body += text("Platform shapes", 40, 686, 16, "#233d40", 'font-weight="600"')
    for i, svg in enumerate(icons):
        for j, style in enumerate(("macos", "android", "ios")):
            x = 56 + i*275 + j*78
            body += image(family.framed(svg, style), x, 708, 56)
            body += text({"macos": "macOS", "android": "Android", "ios": "iOS"}[style], x+28, 786, 11, "#63736e", 'text-anchor="middle"')
    body += text("Family context", 40, 836, 16, "#233d40", 'font-weight="600"')
    neighbors = ["mail", "calendar", "sheets"]
    for i, svg in enumerate(icons):
        row = [(ROOT / f"docs/design/icon-family/assets/{name}.svg").read_text() for name in neighbors]
        row.insert(1, svg)
        for j, item in enumerate(row):
            body += image(family.framed(item, "macos"), 58+i*275+j*52, 856, 40)
    body += text("Static SVG studies · shell frame approximations · artwork is not installed", 40, 922, 11, "#63736e")
    target = OUT / "comparison.svg"
    target.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" width="1120" height="930" viewBox="0 0 1120 930">{body}</svg>')
    subprocess.run(["rsvg-convert", "-o", str(OUT / "comparison.png"), str(target)], check=True)


if __name__ == "__main__":
    build()

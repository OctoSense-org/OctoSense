#!/usr/bin/env python3
"""Build this shell's own app icons: News and OctosMap, one SVG per style.

The framework ships an icon per app id and style (`widgets/themes/<style>/
icons/`, built by its `build_icons.py`). These two replace its News art and
give OctosMap art of its own, in the same seven-style scheme and on the same
64x64 canvas. `src/octosense/style.rs` embeds the files and lays them over the
framework's. Windows 2000 keeps the framework's 16-pixel art.

    python3 tools/build_app_icons.py            # writes resources/icons/apps/
    python3 tools/build_app_icons.py --sheet p  # also renders a review sheet
                                                # to p.png (needs rsvg-convert)

Stdlib only. The renderer takes paths, rects, circles, ellipses, linear
gradients, group opacity and transforms; it has no clip paths, masks or
filters, so everything stays inside its tile by construction.
"""
import base64
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "resources" / "icons" / "apps"
STYLES = ("omarchy", "macos", "windows", "nextstep", "ios", "android")

# name: (tile top, tile bottom)
TILES = {
    "news": ("#ff6672", "#de2139"),
    "maps": ("#5fd58a", "#1f9e52"),
}
NEWS_RED = "#e3263d"
INK = "#d5dce8"  # Omarchy's single ink; the renderer tints it.


def rect(x, y, w, h, fill, rx=0, extra=""):
    extra = f" {extra}" if extra else ""
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}"{extra}/>'


def stroke(d, width=2.6, colour=INK):
    return (f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="{width}" '
            'stroke-linecap="round" stroke-linejoin="round"/>')


def letter_n(x, y, size, fill):
    """A bold sans N in the box at (x, y), `size` on a side."""
    points = [(0, 1), (0, 0), (0.29, 0), (0.71, 0.6), (0.71, 0), (1, 0),
              (1, 1), (0.71, 1), (0.29, 0.4), (0.29, 1)]
    d = " ".join(f"{'M' if i == 0 else 'L'}{x + px * size:.2f} {y + py * size:.2f}"
                 for i, (px, py) in enumerate(points))
    return f'<path d="{d} Z" fill="{fill}"/>'


def news_art():
    """The front page: the paper's N and nameplate, a picture, the columns."""
    line = "#c9ced6"
    return (
        '<g transform="rotate(-9 32 34)">' + rect(17, 14, 32, 38, "#ffffff", 4.5, 'opacity="0.5"') + "</g>"
        + rect(17.2, 15.6, 33, 39, "#000000", 4.5, 'opacity="0.18"')
        + rect(16, 13, 33, 39, "#ffffff", 4.5)
        + letter_n(20, 17, 8.6, NEWS_RED)
        + rect(31, 17, 14, 3.6, NEWS_RED, 1.8) + rect(31, 22, 9.5, 3.6, "#f4a3ad", 1.8)
        + rect(20, 29.4, 11.5, 9.6, "#ffccd2", 2)
        + rect(34.5, 29.4, 10.5, 2.8, "#3a3f4a", 1.4)
        + rect(34.5, 33.8, 10.5, 2.8, line, 1.4)
        + rect(20, 42.2, 25, 2.8, line, 1.4) + rect(20, 46.6, 18, 2.8, line, 1.4)
    )


PIN = ("M32 9 C21.5 9 13.5 17 13.5 27.5 C13.5 39 24 46.5 32 57 "
       "C40 46.5 50.5 39 50.5 27.5 C50.5 17 42.5 9 32 9 Z")


def maps_art():
    """A folded map, its river, park and route, under a pin."""
    return (
        '<path d="M9 18 L24 12.5 L40 18 L55 12.5 V47 L40 52.5 L24 47 L9 52.5 Z" fill="#ffffff"/>'
        '<path d="M24 12.5 L40 18 V52.5 L24 47 Z" fill="#e6ecf3"/>'
        '<path d="M9 40 C16 36 20 44 24 41 L24 47 L9 52.5 Z" fill="#9ed0f7"/>'
        '<path d="M40 18 L55 12.5 V24 C50 27 45 24 40 27 Z" fill="#bfe6b8"/>'
        + stroke("M13 35 L25 31 L31 40 L52 33", 3.4, "#f6c343")
        + '<ellipse cx="38" cy="39.5" rx="6" ry="1.9" fill="#000000" opacity="0.2"/>'
        '<g transform="translate(19.4 3.2) scale(0.58)">'
        f'<path d="{PIN}" fill="#f04438"/><circle cx="32" cy="27" r="7.5" fill="#ffffff"/></g>'
    )


OMARCHY = {
    "news": (f'<rect x="14" y="11" width="36" height="42" rx="4" fill="none" stroke="{INK}" stroke-width="2.6"/>'
             + stroke("M20 29 V18 L28.5 29 V18 M34 19.5 H44 M34 26 H41 M20 36 H44 M20 41.5 H44 M20 47 H37")),
    "maps": stroke("M32 7 C23.5 7 17.5 13.5 17.5 21.5 C17.5 31 26 36 32 45 C38 36 46.5 31 46.5 21.5 "
                   "C46.5 13.5 40.5 7 32 7 Z M32 16.5 A5 5 0 1 0 32 26.5 A5 5 0 1 0 32 16.5 "
                   "M21 41 L9 45.5 V57 L24 51.5 L40 57 L55 51.5 V40 L43 44.5"),
}

ART = {"news": news_art, "maps": maps_art}


def defs(name, shine=False):
    top, bottom = TILES[name]
    out = f'<linearGradient id="tile" x2="0" y2="1"><stop stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>'
    if shine:
        out += ('<linearGradient id="shine" x2="0" y2="1"><stop stop-color="#ffffff" stop-opacity="0.16"/>'
                '<stop offset="0.65" stop-color="#ffffff" stop-opacity="0"/></linearGradient>')
    return f"<defs>{out}</defs>"


def icon(name, style):
    art = ART[name]()
    if style == "omarchy":
        body = OMARCHY[name]
    elif style == "macos":
        body = (defs(name, shine=True)
                + '<g opacity="0.14">' + rect(3, 4.8, 58, 57, "#101521", 13) + "</g>"
                + rect(3, 3, 58, 57, "url(#tile)", 13) + art
                + rect(3.5, 3.5, 57, 56, "url(#shine)", 12.5))
    elif style == "ios":
        outline = "M17 1 H47 C59 1 63 5 63 17 V47 C63 59 59 63 47 63 H17 C5 63 1 59 1 47 V17 C1 5 5 1 17 1 Z"
        body = defs(name) + f'<path d="{outline}" fill="url(#tile)"/>' + art
    elif style == "android":
        body = (defs(name) + '<circle cx="32" cy="32" r="31" fill="url(#tile)"/>'
                + f'<g transform="translate(5.1 5.1) scale(0.84)">{art}</g>')
    elif style == "windows":
        body = (defs(name) + rect(6, 6, 52, 52, "url(#tile)", 6)
                + f'<g transform="translate(3.8 3.8) scale(0.88)">{art}</g>')
    elif style == "nextstep":
        body = (rect(0, 0, 64, 64, "#000000") + rect(1, 1, 62, 62, "#aaaaaa")
                + stroke("M1 62 V1 H62", 2, "#ffffff") + stroke("M2 62 H62 V2", 2, "#555555")
                + f'<g transform="translate(2.6 2.6) scale(0.92)">{art}</g>')
    else:
        raise ValueError(style)
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">{body}</svg>\n'


def write_all():
    for style in STYLES:
        directory = OUT / style
        directory.mkdir(parents=True, exist_ok=True)
        for name in ART:
            (directory / f"{name}.svg").write_text(icon(name, style))


def sheet(target):
    """Every style at two sizes, on the grounds the shell draws them on."""
    cell, small, pad = 128, 48, 20
    grounds = {"omarchy": "#1f2430"}
    width = pad + len(STYLES) * (cell + pad)
    row = 28 + cell + pad + small + pad
    height = pad + len(ART) * row
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{width}" height="{height}">',
             f'<rect width="{width}" height="{height}" fill="#5d6577"/>']
    for r, name in enumerate(ART):
        y0 = pad + r * row
        for c, style in enumerate(STYLES):
            x0 = pad + c * (cell + pad)
            data = base64.b64encode(icon(name, style).encode()).decode()
            parts.append(f'<text x="{x0 + cell / 2}" y="{y0 + 14}" font-family="Helvetica" font-size="13" fill="#ffffff" text-anchor="middle">{name} / {style}</text>')
            if style in grounds:
                parts.append(rect(x0, y0 + 22, cell, cell + pad + small + 6, grounds[style], 10))
            for size, y in ((cell, y0 + 28), (small, y0 + 28 + cell + pad)):
                parts.append(f'<image x="{x0 + (cell - size) / 2}" y="{y}" width="{size}" height="{size}" xlink:href="data:image/svg+xml;base64,{data}"/>')
    parts.append("</svg>")
    svg = pathlib.Path(target).with_suffix(".svg")
    svg.write_text("".join(parts))
    subprocess.run(["rsvg-convert", "-o", str(pathlib.Path(target).with_suffix(".png")), str(svg)], check=True)
    svg.unlink()


if __name__ == "__main__":
    write_all()
    if len(sys.argv) == 3 and sys.argv[1] == "--sheet":
        sheet(sys.argv[2])

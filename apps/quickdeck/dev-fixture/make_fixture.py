#!/usr/bin/env python3
"""Quick Deck v2's dev fixture: sample decks for UI tests in App Hub's card-host.

It never ships. `apps/quickdeck/tests/ui.py` runs it to fill a card-host app
storage folder (Quick Deck's jail) with:

- library.json and decks/*.deckcraft: the six decks of design 01 (stand-in
  files: the stand-in engine never reads them);
- dev/fixture.json: what the stand-in engine (engine.splash) answers: the
  engine's themes, colour schemes and font pairs (deckcraft d0e57d7e
  model/src/theme.rs), its shape gallery (geom/src/preset.rs CATALOG), a font
  list, and each deck's slides with their shapes, boxes, text, notes and
  transitions;
- dev/slides/<deck>/<n>@big.png, @small.png and @thumb.png: each slide drawn
  here with PIL at 2 pixels per point (and smaller), standing in for the
  engine's renders; dev/covers/<deck>.png; dev/layouts/<id>.png.

Every word on the slides is the approved designs' sample text (design/source/),
the Northwind Cooperative's Q3 2026 board update, so a grab can be put beside
its design. Usage: make_fixture.py <jail folder> [--states] [--empty] [--full] [--media]

--states adds the refusals the brief lists: a damaged deck, a protected, a held
and an unsupported presentation brought in from this device. --empty leaves
the library empty (the first run). --full makes the stand-in refuse saves and
exports for room (the storage-full state). --media adds Launch Film, a deck
with clips (playback): a video that plays by itself in a show, with the sample
clip media/clip.mp4 copied to dev/media/; a sound linked to a file outside
the deck (deck.media's unsupported:); a video whose file isn't there, which
the player can't open; and one too big for the storage (storage_full:).
"""
import json
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

S = 2  # pixels per point
W, H = 960, 540
FONTS = {
    "serif": "/System/Library/Fonts/Supplemental/Georgia.ttf",
    "serif-bold": "/System/Library/Fonts/Supplemental/Georgia Bold.ttf",
    "sans": "/System/Library/Fonts/Supplemental/Arial.ttf",
    "sans-bold": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
}
_fonts = {}


def font(kind, size):
    key = (kind, size)
    if key not in _fonts:
        path = Path(FONTS[kind])
        _fonts[key] = ImageFont.truetype(str(path), max(4, int(round(size * S)))) if path.exists() else ImageFont.load_default()
    return _fonts[key]


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


# The engine's built-in themes, colour schemes and font pairs, as deckcraft
# d0e57d7e model/src/theme.rs defines them (dk1, lt1, dk2, lt2, accent 1-6,
# hyperlink, followed hyperlink).
THEMES = [
    ("DeckCraft", ["000000", "FFFFFF", "1E2B3C", "EDEBE7", "2E6FD8", "F26B1D", "2EA36F", "8A5CD8", "D83F6B", "E3B21C", "2E6FD8", "7A5BA6"], "Inter", "Inter"),
    ("Harbor", ["0B1F2A", "FFFFFF", "12384D", "E4EEF2", "1B7FA6", "36B3A8", "F2C14E", "F78154", "5D576B", "8AA29E", "1B7FA6", "5D576B"], "Merriweather", "Open Sans"),
    ("Ember", ["1A1A1A", "FFFCF7", "3D2B1F", "F6EDE3", "D9480F", "F59F00", "A61E4D", "5C940D", "1971C2", "862E9C", "D9480F", "862E9C"], "Montserrat", "Lato"),
    ("Meadow", ["14281D", "FFFFFF", "2D4A37", "EEF5EE", "4C9A2A", "A4C639", "2F6F4E", "E9B44C", "9B2915", "50A2A7", "2F6F4E", "9B2915"], "Merriweather", "Open Sans"),
    ("Nocturne", ["FFFFFF", "121826", "F2F4F8", "1E2638", "7C5CFF", "00C2A8", "FF6B9A", "FFC53D", "4DA3FF", "B2F35F", "7C5CFF", "FF6B9A"], "Inter", "Inter"),
    ("Paper", ["262626", "FFFFFF", "404040", "F2F2F2", "595959", "8C8C8C", "C0504D", "4F81BD", "9BBB59", "F79646", "4F81BD", "8064A2"], "Playfair Display", "Lato"),
    ("Coral Reef", ["0F2A33", "FFFFFF", "0E4D64", "FDF1EC", "FF6F59", "254441", "43AA8B", "B2B09B", "EF3054", "3F88C5", "3F88C5", "EF3054"], "Poppins", "Nunito Sans"),
    ("Slate", ["111827", "FFFFFF", "374151", "F3F4F6", "6366F1", "0EA5E9", "10B981", "F59E0B", "EF4444", "8B5CF6", "6366F1", "8B5CF6"], "Roboto", "Roboto"),
]
# The engine's other colour schemes (deckcraft's builtin_color_schemes).
EXTRA_SCHEMES = [
    ("Grayscale", ["000000", "FFFFFF", "000000", "F8F8F8", "DDDDDD", "B2B2B2", "969696", "808080", "5F5F5F", "4D4D4D", "5F5F5F", "919191"]),
    ("Ocean", ["000000", "FFFFFF", "13293D", "E8F1F2", "006494", "247BA0", "1B98E0", "6CC1E8", "13293D", "0B3954", "1B98E0", "6CC1E8"]),
    ("Sunset", ["2B2118", "FFFFFF", "6F1A07", "FFF3E6", "FF9F1C", "FFBF69", "CB997E", "E76F51", "F4A261", "2A9D8F", "E76F51", "2A9D8F"]),
    ("Forest", ["1B1B1B", "FFFFFF", "283618", "FEFAE0", "606C38", "DDA15E", "BC6C25", "283618", "8A9A5B", "4F772D", "606C38", "BC6C25"]),
    ("Berry", ["1F0A1D", "FFFFFF", "4A1942", "F7E8F3", "893168", "C6426E", "EB6A8F", "4A1942", "7D7ABC", "56A3A6", "893168", "56A3A6"]),
]
FONT_SCHEMES = [("Inter", "Inter", "Inter"), ("Classic", "Merriweather", "Open Sans"), ("Geometric", "Montserrat", "Lato"),
                ("Editorial", "Playfair Display", "Lato"), ("Humanist", "Merriweather", "Open Sans"), ("Friendly", "Poppins", "Nunito Sans"),
                ("Neutral", "Roboto", "Roboto"), ("Office Compatible", "Carlito", "Carlito"), ("Times / Arial", "Liberation Serif", "Liberation Sans")]
# What FontDb finds on a Mac with no craft fonts (the stand-in's deck.fonts).
FAMILIES = ["Arial", "Avenir Next", "Courier New", "Georgia", "Gill Sans", "Helvetica", "Helvetica Neue", "Inter", "Menlo",
            "Open Sans", "Optima", "Palatino", "Times New Roman", "Trebuchet MS", "Verdana"]
# The slide master's eleven layouts (deckcraft's default master).
LAYOUTS = [(1, "Title Slide"), (2, "Title and Content"), (3, "Section Header"), (4, "Two Content"), (5, "Comparison"), (6, "Title Only"),
           (7, "Blank"), (8, "Content with Caption"), (9, "Picture with Caption"), (10, "Title and Vertical Text"), (11, "Vertical Title and Text")]

HARBOR = dict(zip(["dk1", "lt1", "dk2", "lt2", "a1", "a2", "a3", "a4", "a5", "a6"], ["#" + c for c in THEMES[1][1][:10]]))
NAVY = "#12384D"
TEAL = "#1B7FA6"
TEAL2 = "#36B3A8"
INK = "#2B3542"
GREY = "#6B7280"


class Slide:
    def __init__(self, scale=S, ground="#ffffff"):
        self.k = scale
        self.img = Image.new("RGB", (int(W * scale), int(H * scale)), ground)
        self.d = ImageDraw.Draw(self.img)

    def p(self, v):
        return v * self.k

    def text(self, x, y, s, kind="sans", size=18, color=INK, anchor="la"):
        f = ImageFont.truetype(FONTS[kind], max(4, int(round(size * self.k)))) if Path(FONTS[kind]).exists() else ImageFont.load_default()
        self.d.text((self.p(x), self.p(y)), s, font=f, fill=rgb(color), anchor=anchor)
        return f.getlength(s) / self.k

    def rect(self, x, y, w, h, fill=None, outline=None, width=1, radius=0):
        box = [self.p(x), self.p(y), self.p(x + w), self.p(y + h)]
        if radius:
            self.d.rounded_rectangle(box, radius=self.p(radius), fill=rgb(fill) if fill else None, outline=rgb(outline) if outline else None, width=max(1, int(width * self.k)))
        else:
            self.d.rectangle(box, fill=rgb(fill) if fill else None, outline=rgb(outline) if outline else None, width=max(1, int(width * self.k)))

    def line(self, pts, color, width=2):
        self.d.line([(self.p(x), self.p(y)) for x, y in pts], fill=rgb(color), width=max(1, int(width * self.k)))

    def dot(self, x, y, r, color):
        self.d.ellipse([self.p(x - r), self.p(y - r), self.p(x + r), self.p(y + r)], fill=rgb(color))

    def waves(self, top=440):
        # Harbor's title-slide waves (05-theme), as a few filled curves.
        import math
        for i, color in enumerate([TEAL2, TEAL]):
            pts = []
            for x in range(0, W + 10, 10):
                y = top + 30 * i + 22 * math.sin((x / W) * math.pi * (1.4 + i * 0.5) + i * 1.3)
                pts.append((self.p(x), self.p(y)))
            pts += [(self.p(W), self.p(H)), (0, self.p(H))]
            self.d.polygon(pts, fill=rgb(color))

    def title(self, s, y=36):
        self.text(60, y, s, "serif-bold", 34, NAVY)
        self.line([(60, y + 58), (900, y + 58)], TEAL, 1.5)

    def footer(self, n):
        self.text(60, 494, "Northwind Cooperative", "sans", 13, TEAL)
        self.text(900, 494, str(n), "sans", 13, TEAL, anchor="ra")

    def bullets(self, x, y, lines, size=20, gap=1.7, width=None):
        for i, s in enumerate(lines):
            self.dot(x + 6, y + i * size * gap + size * 0.62, 3.2, TEAL)
            self.text(x + 22, y + i * size * gap, s, "sans", size, INK)

    def save(self, path, size=None):
        img = self.img if size is None else self.img.resize(size, Image.LANCZOS)
        path.parent.mkdir(parents=True, exist_ok=True)
        img.save(path)


# ---- the Q3 2026 board update (designs 02-10) ----------------------------------

def tbox(i, name, box, text, kind="placeholder", ph="body", size=18, bold=False, align="l", bullets=False, paragraphs=None, color=INK, font="Open Sans",
         anchor="top", autofit="none"):
    paras = paragraphs if paragraphs is not None else [{"text": t, "level": 0} for t in text.split("\n")]
    return {"id": i, "name": name, "kind": kind, "box": box, "rotation": 0, "placeholder": ph if kind == "placeholder" else None,
            "text": text, "paragraphs": paras,
            # deck.format_state's answer, with the box's anchor and autofit (#507).
            "fmt": {"font": font, "size": size, "bold": bold, "italic": False, "underline": False, "strike": False, "color": color,
                    "align": align, "bullets": bullets, "numbering": False, "line_spacing": 1.2, "anchor": anchor, "autofit": autofit}}


def content_shapes(n, title, extra):
    return [tbox(n * 100 + 1, "Title 1", [60, 36, 840, 58], title, ph="title", size=34, color=NAVY, font="Merriweather")] + extra + [
        tbox(n * 100 + 8, "Footer 8", [60, 490, 300, 22], "Northwind Cooperative", ph="ftr", size=13, color=TEAL),
        tbox(n * 100 + 9, "Slide Number 9", [840, 490, 60, 22], str(n), ph="sldNum", size=13, align="r", color=TEAL)]


REGIONS = [("Northwest", 12.4), ("Coast", 7.1), ("Valley", 5.6), ("Metro", 3.6)]
QUARTERS = [("Q4 2025", 24.1), ("Q1 2026", 25.0), ("Q2 2026", 26.9), ("Q3 2026", 28.7)]
MEMBERS = [("Region", "Members", "Change"), ("Northwest", "6,200", "+8%"), ("Coast", "4,900", "+5%"), ("Valley", "4,100", "+4%"),
           ("Metro", "3,200", "+7%"), ("Total", "18,400", "+6%")]


def column_chart(sl, box, data, title, top=16):
    x, y, w, h = box
    sl.text(x + w / 2, y, title, "sans-bold", 15, "#303642", anchor="ma")
    px, py, pw, ph = x + 40, y + 40, w - 60, h - 80
    for i in range(5):
        gy = py + ph - i * ph / 4
        sl.line([(px, gy), (px + pw, gy)], "#E3E5E8", 1)
        sl.text(px - 10, gy - 8, str(i * top // 4), "sans", 12, GREY, anchor="ra")
    bw = pw / len(data) * 0.5
    for i, (name, v) in enumerate(data):
        cx = px + (i + 0.5) * pw / len(data)
        bh = ph * v / top
        sl.rect(cx - bw / 2, py + ph - bh, bw, bh, fill="#1083B2")
        sl.text(cx, py + ph - bh - 20, str(v), "sans", 13, "#303642", anchor="ma")
        sl.text(cx, py + ph + 8, name, "sans", 14, "#3C424D", anchor="ma")


def line_chart(sl, box, data, title, lo=20, hi=30):
    x, y, w, h = box
    sl.text(x + w / 2, y, title, "sans-bold", 14, "#3F444C", anchor="ma")
    px, py, pw, ph = x + 40, y + 34, w - 56, h - 72
    for i in range(6):
        gy = py + ph - i * ph / 5
        sl.line([(px, gy), (px + pw, gy)], "#E3E5E8", 1)
        sl.text(px - 10, gy - 8, str(lo + i * 2), "sans", 12, GREY, anchor="ra")
    pts = []
    for i, (name, v) in enumerate(data):
        cx = px + (i + 0.5) * pw / len(data)
        cy = py + ph - ph * (v - lo) / (hi - lo)
        pts.append((cx, cy))
        sl.text(cx, py + ph + 8, name, "sans", 12, GREY, anchor="ma")
        sl.text(cx, cy - 24, str(v), "sans-bold", 12, "#383F4B", anchor="ma")
    sl.line(pts, "#1B7FA6", 2.5)
    for cx, cy in pts:
        sl.dot(cx, cy, 4.5, "#1B7FA6")


def q3_slides():
    """(shapes, draw, notes, transition, hidden, layout, title) per slide."""
    out = []

    def add(title, layout, shapes, draw, transition="fade", notes="", hidden=False, editing_draw=None):
        out.append({"title": title, "layout": layout, "shapes": shapes, "draw": draw, "transition": transition, "notes": notes,
                    "hidden": hidden, "editing_draw": editing_draw})

    def s1(sl):
        sl.text(60, 130, "Q3 2026", "serif-bold", 72, NAVY)
        sl.text(60, 210, "Board Update", "serif-bold", 72, NAVY)
        sl.line([(64, 318), (590, 318)], TEAL, 2)
        sl.text(64, 340, "Northwind Cooperative · 21 October 2026", "sans", 24, INK)
        sl.waves()
    add("Q3 2026 Board Update", "Title Slide",
        [tbox(101, "Title 1", [60, 120, 640, 180], "Q3 2026\nBoard Update", ph="ctrTitle", size=66, color=NAVY, font="Merriweather"),
         tbox(102, "Subtitle 2", [60, 330, 620, 44], "Northwind Cooperative · 21 October 2026", ph="subTitle", size=24)], s1, "fade")

    agenda = ["Highlights", "Revenue", "Membership", "Operations", "Outlook", "Questions"]

    def s2(sl):
        sl.title("Agenda")
        sl.bullets(70, 128, agenda, 22)
        sl.footer(2)
    add("Agenda", "Title and Content", content_shapes(2, "Agenda", [tbox(203, "Content 2", [60, 120, 840, 340], "\n".join(agenda), bullets=True, size=22)]), s2, "push")

    stats = [("$28.7M", "revenue", "+12%"), ("18,400", "members", "+6%"), ("96%", "on-time", "delivery")]

    def s3(sl):
        sl.title("Quarter at a glance")
        for i, (big, a, b) in enumerate(stats):
            x = 96 + i * 264
            sl.rect(x, 150, 240, 170, fill="#FFFFFF", outline="#9FCBDB", width=1.5, radius=10)
            sl.text(x + 120, 182, big, "serif-bold", 40, TEAL, anchor="ma")
            sl.text(x + 120, 246, a, "sans", 18, GREY, anchor="ma")
            sl.text(x + 120, 274, b, "sans", 18, TEAL if b.startswith("+") else GREY, anchor="ma")
        sl.footer(3)
    add("Quarter at a glance", "Title Only", content_shapes(3, "Quarter at a glance", [
        tbox(303 + i, "Rounded Rectangle " + str(i + 1), [96 + i * 264, 150, 240, 170], f"{a}\n{b}\n{c}", kind="shape", size=18, align="ctr")
        for i, (a, b, c) in enumerate(stats)]), s3, "fade")

    def s4(sl):
        sl.title("Revenue by region")
        column_chart(sl, [96, 120, 768, 360], REGIONS, "Revenue by region, Q3 2026 (USD millions)")
        sl.footer(4)
    chart4 = {"id": 403, "name": "Chart 3", "kind": "chart", "box": [96, 120, 768, 360], "rotation": 0,
              "chart": {"type": "Clustered Column", "title": "Revenue by region, Q3 2026 (USD millions)", "categories": [r[0] for r in REGIONS],
                        "series": [{"name": "Q3 2026", "values": [r[1] for r in REGIONS]}], "legend": "none", "data_labels": True, "gridlines": True, "palette": "colorful1"}}
    add("Revenue by region", "Title and Content", content_shapes(4, "Revenue by region", [chart4]), s4, "push")

    rev_lines = ["Northwest led", "with $12.4 million", "Prices held steady", "in every region", "Online orders", "rose 18%"]
    rev_paras = ["Northwest led with $12.4 million", "Prices held steady in every region", "Online orders rose 18%"]

    def s5_text(sl):
        y = 132
        for i in range(3):
            sl.dot(80, y + 16, 3.6, TEAL)
            sl.text(98, y, rev_lines[i * 2], "sans", 23, INK)
            sl.text(98, y + 32, rev_lines[i * 2 + 1], "sans", 23, INK)
            y += 84

    def s5(sl, text=True):
        sl.title("Revenue grew 12% year over year")
        if text:
            s5_text(sl)
        line_chart(sl, [480, 120, 420, 330], QUARTERS, "Quarterly revenue (USD millions)")
        sl.footer(5)
    add("Revenue grew 12% year over year", "Two Content", content_shapes(5, "Revenue grew 12% year over year", [
        tbox(503, "Content 2", [60, 132, 400, 300], "\n".join(rev_paras), bullets=True, size=20,
             paragraphs=[{"text": t, "level": 0} for t in rev_paras], autofit="shrink"),
        {"id": 504, "name": "Chart 3", "kind": "chart", "box": [480, 120, 420, 330], "rotation": 0,
         "chart": {"type": "Line with Markers", "title": "Quarterly revenue (USD millions)", "categories": [q[0] for q in QUARTERS],
                   "series": [{"name": "Revenue", "values": [q[1] for q in QUARTERS]}], "legend": "none", "data_labels": True, "gridlines": True, "palette": "colorful1"}}]),
        lambda sl: s5(sl, True), "fade", "", editing_draw=lambda sl: s5(sl, False))

    def s6(sl):
        sl.title("Membership by region")
        x0, y0, cw, rh = 112, 128, [256, 256, 256], 52
        for r, row in enumerate(MEMBERS):
            y = y0 + r * rh
            if r == 0:
                sl.rect(x0, y, sum(cw), rh, fill="#1B7FA6")
            elif r % 2 == 0:
                sl.rect(x0, y, sum(cw), rh, fill="#EEF5F8")
            x = x0
            for c, cell in enumerate(row):
                kind = "sans-bold" if r == 0 or r == len(MEMBERS) - 1 else "sans"
                color = "#FFFFFF" if r == 0 else INK
                if c == 0:
                    sl.text(x + 48, y + 14, cell, kind, 21, color)
                else:
                    sl.text(x + cw[c] / 2, y + 14, cell, kind, 21, color, anchor="ma")
                x += cw[c]
        sl.rect(x0, y0, sum(cw), rh * len(MEMBERS), outline="#D6DCE0", width=1)
        sl.footer(6)
    add("Membership by region", "Title and Content", content_shapes(6, "Membership by region", [
        {"id": 603, "name": "Table 3", "kind": "table", "box": [112, 128, 768, 312], "rotation": 0,
         "table": {"rows": len(MEMBERS), "cols": 3, "style": "medium2-accent1", "cells": [list(r) for r in MEMBERS],
                   # design 10's table: a header row, banded rows and a total row (#507's options)
                   "options": {"header_row": True, "total_row": True, "banded_rows": True, "first_column": False, "last_column": False,
                               "banded_columns": False}}}]), s6, "fade",
        "Total membership passed 18,000 for the first time. Northwest added 460 members, most through the referral program. "
        "Ask Jun Park to say a word about the Metro waitlist.")

    def s7(sl):
        # The design's photo is sample art: a plain stand-in block here.
        sl.rect(0, 0, 480, 540, fill="#9DB4C8")
        sl.rect(0, 300, 480, 240, fill="#7F96A8")
        sl.text(520, 150, "Operations", "serif-bold", 44, NAVY)
        sl.line([(520, 214), (880, 214)], TEAL, 1.5)
        sl.text(522, 236, "The Valley cold-storage", "sans", 22, INK)
        sl.text(522, 266, "hub opened on 2 September.", "sans", 22, INK)
        sl.text(520, 494, "Northwind Cooperative", "sans", 13, TEAL)
        sl.text(900, 494, "7", "sans", 13, TEAL, anchor="ra")
    add("Operations", "Picture with Caption", [
        {"id": 701, "name": "Picture 1", "kind": "picture", "box": [0, 0, 480, 540], "rotation": 0, "alt_text": "", "picture": {}},
        tbox(702, "Title 2", [520, 140, 380, 70], "Operations", ph="title", size=44, color=NAVY, font="Merriweather"),
        tbox(703, "Text 3", [520, 228, 380, 80], "The Valley cold-storage hub opened on 2 September.", size=22)], s7, "push")

    def s8(sl):
        sl.rect(0, 0, W, H, fill="#0B3042")
        sl.text(60, 210, "Outlook", "serif-bold", 54, "#DBE0E4")
        sl.line([(64, 286), (300, 286)], TEAL2, 2)
        sl.waves(470)
    add("Outlook", "Section Header", [tbox(801, "Title 1", [60, 200, 840, 80], "Outlook", ph="title", size=54, color="#DBE0E4", font="Merriweather")], s8, "fade")

    pri = ["Open the Metro pickup point", "Cut delivery emissions by 10%", "Launch the member app"]

    def s9(sl):
        sl.title("Q4 priorities")
        for i, s in enumerate(pri):
            sl.rect(80, 140 + i * 80, 40, 40, outline=TEAL, width=2, radius=20)
            sl.text(150, 148 + i * 80, s, "sans", 22, INK)
        sl.footer(9)
    add("Q4 priorities", "Title and Content", content_shapes(9, "Q4 priorities", [tbox(903, "Content 2", [140, 136, 700, 240], "\n".join(pri), size=22)]), s9, "push")

    risks = ["Rising fuel costs", "Supply chain delays", "Severe weather"]
    mits = ["Lock in rates early", "Diversify suppliers", "Strengthen inventory"]

    def s10(sl):
        sl.title("Risks and mitigations")
        sl.text(80, 130, "Risks", "sans-bold", 22, TEAL)
        sl.text(500, 130, "Mitigations", "sans-bold", 22, "#E05A47")
        sl.bullets(80, 176, risks, 19)
        sl.bullets(500, 176, mits, 19)
        sl.line([(470, 130), (470, 330)], "#D6DCE0", 1)
        sl.footer(10)
    add("Risks and mitigations", "Comparison", content_shapes(10, "Risks and mitigations", [
        tbox(1003, "Text 3", [80, 120, 360, 220], "Risks\n" + "\n".join(risks), size=19),
        tbox(1004, "Text 4", [500, 120, 360, 220], "Mitigations\n" + "\n".join(mits), size=19)]), s10, "push")

    budget = [("Operations", 45, "#1B7FA6"), ("Technology", 25, "#36B3A8"), ("Marketing", 20, "#F78154"), ("Reserve", 10, "#F2C14E")]

    def s11(sl):
        sl.title("Budget request")
        cx, cy, r = 340, 290, 130
        start = -90
        for name, v, color in budget:
            sl.d.pieslice([sl.p(cx - r), sl.p(cy - r), sl.p(cx + r), sl.p(cy + r)], start, start + v * 3.6, fill=rgb(color))
            start += v * 3.6
        sl.dot(cx, cy, 52, "#FFFFFF")
        for i, (name, v, color) in enumerate(budget):
            sl.rect(560, 210 + i * 40, 16, 16, fill=color)
            sl.text(590, 208 + i * 40, f"{name}  {v}%", "sans", 18, INK)
        sl.footer(11)
    add("Budget request", "Title and Content", content_shapes(11, "Budget request", [
        {"id": 1103, "name": "Chart 3", "kind": "chart", "box": [180, 140, 600, 320], "rotation": 0,
         "chart": {"type": "Doughnut", "title": "", "categories": [b[0] for b in budget], "series": [{"name": "Share", "values": [b[1] for b in budget]}],
                   "legend": "r", "data_labels": True, "gridlines": False, "palette": "colorful1"}}]), s11, "none", hidden=True)

    def s12(sl):
        sl.title("Questions")
        sl.d.ellipse([sl.p(400), sl.p(170), sl.p(560), sl.p(320)], outline=rgb(NAVY), width=int(4 * sl.k))
        sl.text(480, 200, "?", "serif-bold", 80, NAVY, anchor="ma")
        sl.footer(12)
    add("Questions", "Title Only", content_shapes(12, "Questions", [
        {"id": 1203, "name": "Speech Bubble 3", "kind": "shape", "box": [400, 170, 160, 150], "rotation": 0, "preset": "wedgeEllipseCallout", "fill": "none", "text": ""}]), s12, "fade")
    return out


# ---- the other decks of design 01 (one cover each) -------------------------------

def cover_title_slide(title_lines, sub, ground, ink, accent, dark=False):
    sl = Slide(S, ground)
    y = 150
    for t in title_lines:
        sl.text(60, y, t, "serif-bold", 60, ink)
        y += 72
    sl.line([(64, y + 10), (420, y + 10)], accent, 2)
    sl.text(64, y + 30, sub, "sans", 22, ink)
    return sl


LIBRARY = [
    # file, title, slides, bytes, theme, days ago
    ("Q3 2026 Board Update", 12, 3565158, "Harbor", 0),
    ("Northern Lights Field Trip", 8, 6501171, "Nocturne", 1),
    ("Riverside Community Garden", 10, 2831155, "Meadow", 5),
    ("Welcome to the Team", 9, 1992294, "Coral Reef", 8),
    ("Spring Fundraiser Pitch", 6, 860160, "Ember", 13),
    ("Coastal Survey Findings", 14, 9227468, "Slate", 19),
]
COVERS = {
    "Northern Lights Field Trip": (["Northern Lights", "Field Trip"], "10–15 February 2026", "#121826", "#E1E3E6", "#7C5CFF"),
    "Riverside Community Garden": (["Riverside", "Community Garden"], "Growing together", "#FFFFFF", "#2D4A37", "#4C9A2A"),
    "Welcome to the Team": (["Welcome", "to the Team"], "We're glad you're here.", "#FFFFFF", "#F0604E", "#FF6F59"),
    "Spring Fundraiser Pitch": (["Spring", "Fundraiser Pitch"], "Support. Grow. Thrive.", "#FCF2E3", "#EA6B1D", "#F59F00"),
    "Coastal Survey Findings": (["Coastal Survey", "Findings"], "September 2026", "#FFFFFF", "#20318A", "#0EA5E9"),
}


# ---- a deck with clips (--media; playback, SERVICE.md "Playback") ------------------

MEDIA_DECK = "Launch Film"
# The sample clip: media/clip.mp4, the repository's synthetic five-second
# H.264 clip with no sound (crates/video-api-smoke/resources/playback.mp4,
# made by tools/generate-video-fixture.swift): red, green and blue thirds
# with a white bar that crosses them.
CLIP = Path(__file__).parent / "media" / "clip.mp4"


def poster(sl, box):
    """The clip's first frame, as the engine draws a clip's poster."""
    x, y, w, h = box
    sl.rect(x, y, w / 3, h, fill="#DC232D")
    sl.rect(x + w / 3, y, w / 3, h, fill="#19B42D")
    sl.rect(x + 2 * w / 3, y, w / 3, h, fill="#1923D7")
    sl.rect(x, y, w * 16 / 320, h, fill="#F5F5F5")


def film(sl, box):
    """A clip without a poster: the engine's film glyph on dark."""
    x, y, w, h = box
    sl.rect(x, y, w, h, fill="#20252B")
    cx, cy = x + w / 2, y + h / 2
    sl.rect(cx - 34, cy - 24, 68, 48, outline="#9AA3AD", width=3, radius=6)
    sl.d.polygon([(sl.p(cx - 9), sl.p(cy - 13)), (sl.p(cx - 9), sl.p(cy + 13)), (sl.p(cx + 14), sl.p(cy))], fill=rgb("#9AA3AD"))


def speaker(sl, box):
    """An audio object: the engine's speaker glyph."""
    x, y, w, h = box
    sl.rect(x, y, w, h, fill="#E8F1F5", radius=10)
    cx, cy = x + w / 2, y + h / 2
    s = w / 80
    sl.d.polygon([(sl.p(cx - 22 * s), sl.p(cy - 9 * s)), (sl.p(cx - 9 * s), sl.p(cy - 9 * s)), (sl.p(cx + 6 * s), sl.p(cy - 22 * s)),
                  (sl.p(cx + 6 * s), sl.p(cy + 22 * s)), (sl.p(cx - 9 * s), sl.p(cy + 9 * s)), (sl.p(cx - 22 * s), sl.p(cy + 9 * s))], fill=rgb(TEAL))
    sl.d.arc([sl.p(cx + 2 * s), sl.p(cy - 14 * s), sl.p(cx + 22 * s), sl.p(cy + 14 * s)], -50, 50, fill=rgb(TEAL), width=max(1, int(3 * s * sl.k)))


def clip_shape(i, name, box, video, autoplay=False, loop=False, **fixture):
    """A video or audio shape as deck.slide reports it (media: {id, video,
    autoplay, loop}); fixture_media is what the stand-in answers deck.media
    with (engine.splash), never read by the app."""
    sh = {"id": i, "name": name, "kind": "video" if video else "audio", "box": box, "rotation": 0,
          "media": {"id": 9000 + i, "video": video, "autoplay": autoplay, "loop": loop}}
    if fixture:
        sh["fixture_media"] = fixture
    return sh


def media_slides():
    """(title, layout, shapes, draw) per slide: a video that plays by itself,
    a sound whose clip is linked, a video the player can't open and one too
    big for the storage."""
    out = []

    def add(n, title, shapes, draw, notes=""):
        out.append({"title": title, "layout": "Title Only", "shapes": [tbox(n * 100 + 1, "Title 1", [60, 36, 840, 58], title, ph="title", size=34,
                                                                            color=NAVY, font="Merriweather")] + shapes,
                    "draw": draw, "notes": notes})

    film_box = [200, 120, 560, 315]

    def s1(sl):
        sl.title("Launch film")
        poster(sl, film_box)
        sl.text(480, 460, "Plays by itself in a show", "sans", 16, GREY, anchor="ma")
    add(1, "Launch film", [clip_shape(102, "Launch film.mp4", film_box, True, autoplay=True)], s1,
        "Let the film run, then take questions.")

    sound_box = [440, 200, 80, 80]

    def s2(sl):
        sl.title("Soundtrack")
        speaker(sl, sound_box)
        sl.text(480, 310, "Theme music for the launch (linked to a file outside the deck)", "sans", 16, GREY, anchor="ma")
    add(2, "Soundtrack", [clip_shape(202, "Theme.mp3", sound_box, False, answer="unsupported")], s2)

    small_box = [260, 130, 440, 248]

    def s3(sl):
        sl.title("Behind the scenes")
        film(sl, small_box)
    add(3, "Behind the scenes", [clip_shape(302, "Behind the scenes.mov", small_box, True, path="dev/media/missing.mov", ext="mov")], s3)

    def s4(sl):
        sl.title("Full cut")
        film(sl, small_box)
    add(4, "Full cut", [clip_shape(402, "Full cut.mp4", small_box, True, answer="storage_full")], s4)
    return out


def layout_picture(lid):
    sl = Slide(LAYOUT_SCALE, "#FFFFFF")
    g, d = "#C9CBCF", "#E4E4E5"
    boxes = {
        1: [(160, 160, 640, 110, g), (240, 300, 480, 40, d)],
        2: [(60, 40, 840, 70, g), (60, 140, 840, 340, d)],
        3: [(60, 230, 840, 90, g), (60, 340, 840, 40, d)],
        4: [(60, 40, 840, 70, g), (60, 140, 400, 340, d), (500, 140, 400, 340, d)],
        5: [(60, 40, 840, 70, g), (60, 130, 400, 40, g), (500, 130, 400, 40, g), (60, 190, 400, 290, d), (500, 190, 400, 290, d)],
        6: [(60, 40, 840, 70, g)],
        7: [],
        8: [(60, 60, 300, 80, g), (420, 60, 480, 420, d), (60, 170, 300, 300, d)],
        9: [(60, 40, 520, 460, d), (640, 200, 260, 70, g), (640, 290, 260, 120, d)],
        10: [(60, 40, 840, 70, g), (420, 140, 120, 340, d)],
        11: [(760, 40, 140, 460, g), (60, 40, 660, 460, d)],
    }[lid]
    for x, y, w, h, c in boxes:
        sl.rect(x, y, w, h, fill=c)
    return sl


LAYOUT_SCALE = 0.2


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    jail = Path(sys.argv[1])
    states = "--states" in sys.argv
    empty = "--empty" in sys.argv
    dev = jail / "dev"
    (jail / "decks").mkdir(parents=True, exist_ok=True)
    now = time.time()
    presets = json.loads((Path(__file__).parent / "presets.json").read_text())

    fixture = {
        "themes": {
            "themes": [{"name": n, "label": "Default" if n == "DeckCraft" else n, "colors": ["#" + c for c in cols], "major": mj, "minor": mn}
                       for n, cols, mj, mn in THEMES],
            # As the service answers (#507): every colour set with its twelve colours.
            "color_schemes": [{"name": n, "colors": ["#" + c for c in cols]} for n, cols, _, _ in THEMES] +
                             [{"name": n, "colors": ["#" + c for c in cols]} for n, cols in EXTRA_SCHEMES],
            "font_schemes": [{"name": n, "major": a, "minor": b} for n, a, b in FONT_SCHEMES],
        },
        "fonts": FAMILIES,
        "shapes": [{"name": n, "label": l, "category": c} for n, l, c in presets],
        "layouts": [{"id": i, "name": n, "kind": n.lower().replace(" ", "_"), "path": f"dev/layouts/{i}.png"} for i, n in LAYOUTS],
        "decks": {},
        "refusals": {},
    }
    for i, n in LAYOUTS:
        layout_picture(i).save(dev / "layouts" / f"{i}.png")
    # A slide the fixture has no picture for (a new or an outline's slide):
    # the editor's empty title placeholder.
    blank = Slide()
    blank.rect(60, 36, 840, 58, outline="#B8BCC2", width=1)
    blank.text(72, 50, "Click to add title", "sans", 26, "#9AA0A6")
    blank.save(dev / "blank@big.png")
    blank.save(dev / "blank@small.png", (536, 302))
    blank.save(dev / "blank@thumb.png", (400, 225))

    index = {}
    for title, count, size, theme, days in ([] if empty else LIBRARY):
        path = f"decks/{title}.deckcraft"
        (jail / path).write_text("Quick Deck dev fixture: a stand-in deck\n")
        t = now - days * 86400 - 3600
        index[path] = {"title": title, "slides": count, "bytes": size, "theme": theme, "created": t - 86400, "edited": t, "opened": t}
        key = title.lower().replace(" ", "-")
        if title == "Q3 2026 Board Update":
            slides = q3_slides()
            deck_slides = []
            for n, s in enumerate(slides, start=1):
                sl = Slide()
                s["draw"](sl)
                sl.save(dev / "slides" / key / f"{n}@big.png")
                sl.save(dev / "slides" / key / f"{n}@small.png", (536, 302))
                sl.save(dev / "slides" / key / f"{n}@thumb.png", (400, 225))
                if s["editing_draw"]:
                    e = Slide()
                    s["editing_draw"](e)
                    e.save(dev / "slides" / key / f"{n}-editing@big.png")
                if n == 1:
                    sl.save(dev / "covers" / f"{key}.png", (480, 270))
                shapes = []
                for sh in s["shapes"]:
                    sh = {k: v for k, v in sh.items() if v is not None}
                    shapes.append(sh)
                deck_slides.append({"id": 1000 + n, "title": s["title"], "layout": s["layout"], "hidden": s["hidden"], "notes": s["notes"],
                                    "transition": s["transition"], "builds": 1 if n == 2 else 0, "shapes": shapes, "pic": n})
            fixture["decks"][path] = {"key": key, "title": title, "theme": theme, "size": [960, 540], "slides": deck_slides,
                                      "sections": [{"name": "Highlights", "first": 0, "count": 7}, {"name": "Outlook", "first": 7, "count": 5}],
                                      "missing_fonts": ["Merriweather", "Open Sans"],
                                      # design 05: slide numbers on, no date; the slides' footer text
                                      "footer": {"slide_numbers": True, "footer": True, "footer_text": "Northwind Cooperative", "date": False,
                                                 "hide_on_title": False}}
        else:
            lines, sub, ground, ink, accent = COVERS[title]
            sl = cover_title_slide(lines, sub, ground, ink, accent)
            sl.save(dev / "covers" / f"{key}.png", (480, 270))
            sl.save(dev / "slides" / key / "1@big.png")
            sl.save(dev / "slides" / key / "1@small.png", (536, 302))
            sl.save(dev / "slides" / key / "1@thumb.png", (400, 225))
            fixture["decks"][path] = {"key": key, "title": title, "theme": theme, "size": [960, 540], "sections": [], "missing_fonts": [],
                                      "slides": [{"id": 1001, "title": lines[0], "layout": "Title Slide", "hidden": False, "notes": "", "transition": "fade",
                                                  "builds": 0, "pic": 1, "shapes": [tbox(101, "Title 1", [60, 140, 700, 170], "\n".join(lines), ph="ctrTitle", size=60, color=ink),
                                                                                    tbox(102, "Subtitle 2", [60, 330, 600, 40], sub, ph="subTitle", size=22, color=ink)]}]}
    if "--media" in sys.argv:
        path = f"decks/{MEDIA_DECK}.deckcraft"
        (jail / path).write_text("Quick Deck dev fixture: a stand-in deck\n")
        key = MEDIA_DECK.lower().replace(" ", "-")
        slides = media_slides()
        index[path] = {"title": MEDIA_DECK, "slides": len(slides), "bytes": 1258291, "theme": "Harbor", "created": now - 7200, "edited": now - 600,
                       "opened": now - 600}
        deck_slides = []
        for n, s in enumerate(slides, start=1):
            sl = Slide()
            s["draw"](sl)
            sl.save(dev / "slides" / key / f"{n}@big.png")
            sl.save(dev / "slides" / key / f"{n}@small.png", (536, 302))
            sl.save(dev / "slides" / key / f"{n}@thumb.png", (400, 225))
            if n == 1:
                sl.save(dev / "covers" / f"{key}.png", (480, 270))
            deck_slides.append({"id": 1000 + n, "title": s["title"], "layout": s["layout"], "hidden": False, "notes": s["notes"], "transition": "fade",
                                "builds": 0, "shapes": [{k: v for k, v in sh.items() if v is not None} for sh in s["shapes"]], "pic": n})
        fixture["decks"][path] = {"key": key, "title": MEDIA_DECK, "theme": "Harbor", "size": [960, 540], "slides": deck_slides, "sections": [],
                                  "missing_fonts": []}
        (dev / "media").mkdir(parents=True, exist_ok=True)
        (dev / "media" / "clip.mp4").write_bytes(CLIP.read_bytes())
    if states:
        (jail / "imports").mkdir(parents=True, exist_ok=True)
        refusals = {
            "decks/Damaged deck.deckcraft": "damaged: the engine couldn't read it: the archive's central directory is cut short",
            "imports/Payroll 2025.pptx": "protected: this presentation is encrypted and needs a password",
            "imports/Town hall recording.pptx": "held: the presentation holds a zip inside its zip (`ppt/media/recording.zip`), which the deck service does not open (#448)",
            "imports/Minutes 2019.pptx": "unsupported: this is a legacy PowerPoint file (.ppt), not a .pptx presentation",
        }
        for path, error in refusals.items():
            (jail / path).write_text("Quick Deck dev fixture: a stand-in file\n")
            stem = Path(path).stem
            index[path] = {"title": stem, "created": now - 40 * 86400, "edited": now - 40 * 86400}
        fixture["refusals"] = refusals
    (jail / "library.json").write_text(json.dumps({"version": 2, "decks": index, "pictures": {}, "show_notes": True, "v1_done": True}))
    dev.mkdir(parents=True, exist_ok=True)
    (dev / "fixture.json").write_text(json.dumps(fixture))
    if "--full" in sys.argv:
        (dev / "full").write_text("the stand-in refuses saves and exports for room\n")
    print("fixture written to", jail)


if __name__ == "__main__":
    main()

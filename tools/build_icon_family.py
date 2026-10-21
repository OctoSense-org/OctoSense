#!/usr/bin/env python3
"""Build the approved OctoSense icon family and optional shipping copies.

Default: rebuild review sheets (rsvg-convert/Pillow). --install: also update
shell/bundle resources. --check: read-only synchronization and provenance check.
"""
from pathlib import Path
import argparse
import base64
import hashlib
import io
import json
import subprocess
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "docs/design/icon-family"
IVORY = "#fffdf7"
INK = "#233d40"


def rect(x, y, w, h, color, r=0, extra=""):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{color}" {extra}/>'


def path(d, color, extra=""):
    return f'<path d="{d}" fill="{color}" {extra}/>'


def line(d, color, width=2.5, extra=""):
    return path(d, "none", f'stroke="{color}" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round" {extra}')


def circle(x, y, r, color, extra=""):
    return f'<circle cx="{x}" cy="{y}" r="{r}" fill="{color}" {extra}/>'


def group(art, transform):
    return f'<g transform="{transform}">{art}</g>'


def book(color, accent):
    return (rect(16, 12, 33, 42, "#102c36", 4, 'opacity="0.15"')
            + rect(15, 11, 33, 41, color, 4)
            + rect(19, 12, 29, 38, IVORY, 3)
            + rect(15, 11, 5, 41, accent, 2)
            + line("M24 45 H43", "#c8d3cf", 1.5))


def document(x=18, y=12, color=IVORY):
    return path(f"M{x+3} {y} H{x+21} L{x+29} {y+8} V{y+38} Q{x+29} {y+41} {x+26} {y+41} H{x+3} Q{x} {y+41} {x} {y+38} V{y+3} Q{x} {y} {x+3} {y} Z", color)


# Every drawing uses a 64-point canvas. Critical foreground is in [11, 53].
# Backgrounds and simple secondary cues may reach beyond that safe square.
ART = {}

ART["mail"] = (
    rect(12, 25, 40, 26, "#082d41", 5, 'opacity="0.16"')
    + rect(12, 23, 40, 26, "#b8e6e0", 4)
    + group(rect(19, 13, 27, 29, IVORY, 3) + line("M24 20 H39 M24 26 H36", "#82b9b7", 2.5), "rotate(6 32 30)")
    + path("M12 27 L32 40 L52 27 V47 Q52 51 48 51 H16 Q12 51 12 47 Z", "#e7f5ec")
    + path("M12 48 L25 37 L32 41 L39 37 L52 48 Q51 51 47 51 H17 Q13 51 12 48 Z", IVORY)
    + circle(43, 41, 5, "#f47761") + line("M41 41 H45", IVORY, 1.8)
)
ART["calendar"] = (
    rect(15, 16, 36, 38, "#7e292b", 5, 'opacity="0.18"')
    + rect(13, 14, 37, 38, IVORY, 5)
    + path("M18 14 H45 Q50 14 50 19 V26 H13 V19 Q13 14 18 14 Z", "#ffb399")
    + line("M23 12 V19 M40 12 V19", "#843c3f", 3)
    + line("M23 31 V44 M33 31 H41 L33 37 H38 Q43 37 41 42 Q39 46 33 43", "#c34846", 3)
    + path("M40 52 L50 42 V47 Q50 52 45 52 Z", "#f3d7c4")
)
ART["calculator"] = (
    rect(19, 12, 30, 42, "#153e39", 5, 'opacity="0.2"')
    + rect(17, 10, 30, 43, IVORY, 5)
    + rect(21, 15, 22, 11, "#b8ded0", 2)
    + line("M34 19 H39 M36 22 H39", "#38695e", 1.8)
    + ''.join(rect(x, y, 5, 5, "#879f91", 1.5) for y in (31, 40) for x in (21, 29))
    + rect(37, 31, 6, 14, "#ef8263", 2)
    + line("M39 36 H41 M39 39 H41", IVORY, 1.2)
)
ART["clock"] = (
    circle(32, 32, 21, IVORY)
    + line("M32 18 V20 M32 44 V46 M18 32 H20 M44 32 H46", "#304b55", 2)
    + line("M32 22 V32 L40 36", "#304b55", 3.5)
    + circle(32, 32, 2.5, "#304b55")
)
ART["notes"] = (
    group(book("#e9b948", "#bd7e27")
          + rect(21, 13, 26, 8, "#ffe6a0", 2)
          + line("M25 28 H40 M25 34 H36", "#a5a994", 2.3), "rotate(-4 32 32)")
    + group(rect(43, 27, 6, 21, "#287e73", 1)
            + rect(43, 25, 6, 5, "#d2ece0", 1)
            + path("M43 48 L46 54 L49 48 Z", "#f3d2a3")
            + path("M45 52 L46 54 L47 52 Z", INK), "rotate(24 46 40)")
)
ART["reminders"] = (
    rect(18, 15, 31, 39, "#155d53", 5, 'opacity="0.18"')
    + rect(16, 13, 32, 39, IVORY, 5)
    + rect(25, 10, 14, 7, "#b9e9d4", 3)
    + circle(25, 27, 5, "#f17b60") + line("M22.5 27 L24.5 29 L28 25", IVORY, 1.8)
    + line("M35 26 H41 M35 30 H39", "#3c8475", 2)
    + rect(21, 37, 7, 7, "#c5ddd0", 2) + line("M35 39 H41 M35 43 H38", "#9cb9aa", 2)
)
ART["weather"] = (
    circle(25, 25, 11, "#ffe496")
    + line("M25 11 V13 M11 25 H13 M15 15 L17 17 M35 15 L33 17 M25 37 V39", "#ffe496", 2.5)
    + path("M17 47 C10 47 10 36 17 35 C17 22 37 21 39 33 C52 29 57 47 45 47 Z", "#c7e8f3")
    + path("M18 44 C11 44 12 34 20 34 C20 22 38 23 39 34 C51 30 55 44 45 44 Z", IVORY)
    + line("M25 50 L23 53 M36 50 L34 53", "#b1eef2", 2.5)
)
ART["sheets"] = (
    rect(20, 13, 31, 40, "#0a4d3a", 4, 'opacity="0.2"')
    + rect(18, 11, 32, 40, IVORY, 4)
    + rect(18, 11, 32, 10, "#b5e4c6", 4) + rect(18, 17, 32, 5, "#b5e4c6")
    + ''.join(rect(x, y, 6, 5, "#b9d9c7", 1) for x in (24, 34, 42) for y in (27, 36, 43))
    + rect(12, 29, 20, 19, "#267d57", 3)
    + line("M17 34 H26 M17 38 H24 M17 42 H26", IVORY, 2)
)
ART["settings"] = (
    rect(13, 14, 39, 39, "#193842", 8, 'opacity="0.2"')
    + rect(12, 12, 39, 39, IVORY, 7)
    + line("M21 20 V43 M32 20 V43 M43 20 V43", "#c1d5d1", 3)
    + rect(17, 25, 8, 7, "#347e79", 2)
    + rect(28, 36, 8, 7, "#e69963", 2)
    + rect(39, 21, 8, 7, "#347e79", 2)
)
ART["updater"] = (
    path("M24 13 C14 17 10 29 14 39", "none", 'stroke="#b6e7c9" stroke-width="5" stroke-linecap="round"')
    + path("M40 51 C50 47 54 35 50 25", "none", 'stroke="#b6e7c9" stroke-width="5" stroke-linecap="round"')
    + path("M10 37 L15 45 L20 36 Z M44 28 L49 19 L54 27 Z", "#b6e7c9")
    + rect(23, 35, 19, 13, "#d5e9d6", 3)
    + path("M32 13 L21 25 H28 V37 H36 V25 H43 Z", IVORY)
    + line("M27 43 H37", "#3c7d64", 2)
)
ART["terminal"] = (
    rect(11, 16, 42, 35, "#101e27", 6, 'opacity="0.35"')
    + rect(11, 13, 42, 35, "#152d35", 6)
    + path("M17 13 H47 Q53 13 53 19 V22 H11 V19 Q11 13 17 13 Z", "#40575b")
    + circle(17, 18, 1.5, "#e9a379") + circle(22, 18, 1.5, "#d9d3a4")
    + line("M20 29 L26 34 L20 39", "#b9f2d5", 3)
    + rect(32, 37, 11, 3, "#eaa774", 1)
)
ART["octoscode"] = (
    rect(12, 15, 41, 35, "#362751", 6, 'opacity="0.25"')
    + rect(11, 12, 42, 36, "#f3edf8", 6)
    + path("M17 12 H47 Q53 12 53 18 V22 H11 V18 Q11 12 17 12 Z", "#d1bce9")
    + circle(17, 17, 1.5, "#8a64af") + circle(22, 17, 1.5, "#8a64af")
    + line("M23 28 L17 34 L23 40 M41 28 L47 34 L41 40 M35 27 L29 42", "#715199", 2.5)
    + rect(42, 42, 9, 9, "#efb867", 2)
)
ART["reference"] = (
    book("#a59bc5", "#696286")
    + path("M35 11 H42 V31 L38.5 28 L35 31 Z", "#e9a46d")
    + line("M25 23 H30 M25 29 H30 M25 36 H41", "#9b9bb0", 2.4)
)
ART["appcard"] = (
    group(rect(15, 15, 31, 38, "#c5b9e5", 5), "rotate(-10 32 34)")
    + rect(19, 12, 31, 38, IVORY, 5)
    + path("M26 22 H44 V34 H35 L29 39 V34 H26 Z", "#8b6fbd")
    + circle(31, 28, 1.3, IVORY) + circle(35, 28, 1.3, IVORY) + circle(39, 28, 1.3, IVORY)
    + line("M26 44 H41", "#c5b9d7", 2)
)
ART["browser"] = (
    circle(32, 33.5, 21, "#194f61", 'opacity="0.16"')
    + circle(32, 31.5, 21, "#cde9e4")
    + circle(32, 31.5, 19, INK)
    + line("M32 15.5 V17 M32 46 V47.5 M16 31.5 H17.5 M46.5 31.5 H48", "#8abcb5", 1.8)
    + group(path("M32 17 L36 28 L47 32 L36 36 L32 47 L28 36 L17 32 L28 28 Z", "#a8d1c8")
            + path("M32 17 L36 28 L32 32 L28 28 Z", IVORY)
            + path("M32 32 L36 36 L32 47 Z", "#74afa7")
            + circle(32, 32, 2, INK), "translate(0 -0.5) rotate(30 32 32)")
)
ART["files"] = (
    rect(14, 16, 23, 28, "#b3d8d2", 3)
    + group(rect(22, 11, 24, 31, IVORY, 3) + line("M28 19 H39 M28 24 H37", "#b3c9bf", 2), "rotate(7 32 30)")
    + path("M12 29 V25 Q12 22 16 22 H27 L32 28 H48 Q52 28 52 32 V48 Q52 52 48 52 H16 Q12 52 12 48 Z", "#5ca6b1")
    + path("M12 33 H52 L49 49 Q48 52 45 52 H16 Q12 52 12 48 Z", "#8acbd1")
    + rect(18, 39, 13, 4, IVORY, 2)
)
ART["task"] = (
    rect(12, 15, 40, 33, "#d9ede2", 5)
    + rect(15, 18, 34, 27, "#1e4e4b", 3)
    + line("M19 34 H24 L28 25 L33 39 L37 31 H45", "#b8edd1", 2.5)
    + line("M27 52 H37 M32 48 V52", "#c5e5d3", 3)
    + circle(45, 22, 2, "#eeac6d")
)
ART["finance"] = (
    document(16, 11)
    + line("M23 21 H37 M23 27 H32", "#a9bcb1", 2.5)
    + rect(23, 37, 4, 9, "#9ac2a6", 1) + rect(30, 32, 4, 14, "#59967c", 1)
    + circle(43, 43, 10, "#f2bd5e") + circle(43, 43, 7, "#ffe5a0")
    + line("M44 38 H41 Q38 41 43 43 Q48 45 42 48 M43 37 V49", "#b88436", 1.5)
)
ART["route"] = (
    path("M13 22 L25 18 L39 22 L51 18 V46 L39 50 L25 46 L13 50 Z", IVORY)
    + path("M25 18 L39 22 V50 L25 46 Z", "#d5e7d4")
    + line("M19 42 L27 33 L36 37 L46 27", "#d7a24d", 3.5)
    + circle(19, 42, 3, "#cc704e")
    + path("M46 15 C37 15 36 25 46 35 C56 25 55 15 46 15 Z", "#d96b50")
    + circle(46, 22, 3, IVORY)
)
ART["image"] = (
    group(rect(13, 15, 35, 33, "#c5d9d9", 3), "rotate(-8 32 32)")
    + rect(16, 13, 35, 34, IVORY, 3)
    + rect(20, 17, 27, 25, "#a9d9e3", 1)
    + circle(39, 24, 4, "#ffe39b")
    + path("M20 42 V35 L28 26 L39 42 Z", "#508d83")
    + path("M29 42 L39 31 L47 38 V42 Z", "#85b6a1")
    + line("M25 51 H43", "#c4e4de", 2)
)
ART["pdf"] = (
    document(17, 11)
    + path("M38 11 V19 H46 Z", "#ecc7c0")
    + line("M23 43 C30 32 34 22 32 22 C27 24 33 39 43 39 C48 37 32 34 23 43 Z", "#cc655c", 2.3)
    + rect(22, 17, 11, 5, "#df8c7d", 1.5)
)
ART["mixer"] = (
    path("M19 13 H45 L50 50 H14 Z", "#f4e5e5")
    + line("M23 23 L21 43 M32 23 V43 M41 23 L43 43", "#bd9caa", 2.5)
    + rect(18, 30, 8, 6, "#8b5879", 2)
    + rect(28, 37, 8, 6, "#df967a", 2)
    + rect(38, 26, 8, 6, "#8b5879", 2)
    + circle(23, 18, 2, "#d8adbd") + circle(32, 18, 2, "#d8adbd") + circle(41, 18, 2, "#d8adbd")
)
ART["fabric"] = (
    path("M23 14 L15 18 L10 31 L19 35 L22 29 V51 H43 V29 L46 35 L54 31 L49 18 L41 14 Q32 22 23 14 Z", "#c5e3dd")
    + path("M24 13 L16 17 L12 29 L20 32 L23 26 V49 H41 V26 L44 32 L52 29 L48 17 L40 13 Q32 21 24 13 Z", IVORY)
    + line("M25 14 Q32 24 39 14", "#86b5ad", 2)
    + path("M29 29 H39 V39 H29 Z", "#b1d8cd")
    + line("M29 29 H39 V39", "#75a99b", 1.5)
)
ART["score"] = (
    path("M24 20 V45 C17 42 12 47 15 51 C19 55 29 51 29 46 V27 L43 23 V39 C36 36 31 42 34 46 C38 50 48 45 48 40 V13 Z", IVORY)
    + path("M29 20 L43 16 V21 L29 25 Z", "#f2c78f")
    + circle(17, 28, 2, "#edbd82") + circle(47, 52, 1.5, "#edbd82")
)
ART["video"] = (
    rect(13, 20, 39, 30, "#302c54", 5, 'opacity="0.2"')
    + rect(12, 18, 39, 30, IVORY, 5)
    + group(rect(12, 13, 39, 10, "#c7b9de", 2)
            + path("M18 13 H24 L18 23 H12 Z M32 13 H38 L32 23 H26 Z M46 13 H51 V16 L47 23 H40 Z", "#66547f"), "rotate(-9 12 23)")
    + path("M27 28 L39 35 L27 42 Z", "#9470b5")
)
ART["vj"] = (
    circle(30, 33, 21, "#342840") + circle(30, 33, 17, "#574467")
    + circle(30, 33, 13, "#342840") + circle(30, 33, 9, "#675276")
    + circle(30, 33, 6, "#df97b5") + circle(30, 33, 1.8, IVORY)
    + line("M50 14 L46 19 L42 40 L38 43", "#e7d2cc", 3)
    + rect(36, 40, 6, 5, "#f3b774", 1)
)
ART["fab"] = (
    path("M32 12 L51 23 V44 L32 54 L13 44 V23 Z", "#e3b580")
    + path("M32 10 L51 21 L32 32 L13 21 Z", "#fff3d7")
    + path("M13 21 L32 32 V52 L13 42 Z", "#f4d5a1")
    + path("M32 32 L51 21 V42 L32 52 Z", "#e8b775")
    + path("M26 14 L45 25 L39 28 L20 18 Z", "#b48353")
    + path("M39 28 L45 25 V34 L39 37 Z", "#9f704a")
)
ART["studio"] = (
    rect(13, 17, 37, 29, "#efdfc6", 4)
    + rect(17, 21, 29, 20, "#3c5b60", 2)
    + line("M22 26 V36 M22 26 H27 M36 26 H41 V36", "#8dafab", 1.5)
    + path("M28 25 L37 31 L28 37 Z", "#f0b978")
    + line("M27 51 H38 M32 46 V51", "#efdfc6", 3)
    + circle(18, 12, 4, "#f0b978") + circle(29, 12, 4, "#f0b978")
)

# id, label, scope, top, bottom, metaphor. Retain custom app identities.
SPECS = [
    ("mail", "Mail", "primary", "#30a8b2", "#007080", "Ivory envelope, mint letter, coral seal"),
    ("calendar", "Calendar", "primary", "#ff7e6f", "#d94d4d", "Coral flip calendar with two binding rings"),
    ("calculator", "Calculator", "primary", "#54b49c", "#208673", "Ivory calculator, sage display, coral equals key"),
    ("clock", "Clock", "primary", "#304b55", "#304b55", "Two-color ivory dial and dark hands; no rim, feet or second hand"),
    ("notes", "Notes", "primary", "#f1c654", "#d99a2a", "Pocket notebook with a mint pencil"),
    ("reminders", "Reminders", "primary", "#47bea0", "#00866f", "Ivory clipboard with a coral completed task"),
    ("weather", "Weather", "primary", "#4bc4e7", "#167ebf", "Butter sun behind an ivory rain cloud"),
    ("sheets", "Sheets", "primary", "#42ba79", "#0a854c", "Ivory workbook with a green formula tab and uniformly colored cells"),
    ("settings", "Settings", "primary", "#6da7a8", "#3a717b", "Ivory control plate with three tactile sliders"),
    ("updater", "OctoSense Updates", "primary", "#6cad83", "#367651", "Upward install arrow between two mint cycle arcs"),
    ("terminal", "Terminal", "primary", "#506769", "#2b4148", "Ink command window with mint prompt and apricot cursor"),
    ("octoscode", "OctosCode", "primary", "#9c75dc", "#6943b0", "Ivory code window with violet brackets and amber tab"),
    ("rinx", "Rinx", "primary", "", "", "Original Rinx cube icon, preserved byte-for-byte from the pinned dependency"),
    ("reference", "Reference", "primary", "#ac9ec7", "#74648e", "Ivory reference book with an apricot bookmark"),
    ("appcard", "AppCard", "primary", "#b08ada", "#7c50b0", "Layered ivory cards with an inset chat panel"),
    ("browser", "Browser", "optional", "#45aec2", "#227a94", "C5 Night rose: ink compass dial, mint rim and four-spoke rose with an ivory northeast point"),
    ("files", "Files", "optional", "#89cfc8", "#4d999c", "Sea-glass folder holding an angled ivory sheet"),
    ("task", "Task Manager", "optional", "#3ba78e", "#1b715e", "Ivory monitor with a mint performance trace"),
    ("finance", "Finance", "optional", "#93b95e", "#608438", "Ivory ledger and a warm gold coin"),
    ("route", "Route", "optional", "#afc77d", "#7d9b4b", "Folded route map with a small terracotta destination pin"),
    ("image", "Image Viewer", "optional", "#60bdc5", "#37879b", "Stacked landscape prints with a pale yellow sun"),
    ("pdf", "PDF Viewer", "optional", "#e08e78", "#b4584e", "Ivory page with an inked coral ribbon curve"),
    ("mixer", "Mixer", "optional", "#c489b1", "#935381", "Ivory perspective mixing desk with apricot fader"),
    ("fabric", "Fabric", "optional", "#70bea6", "#3f907c", "Ivory garment with a sea-glass stitched pocket"),
    ("score", "Score", "optional", "#e7a25e", "#c07138", "Ivory connected notes with a warm score band"),
    ("video", "Video Player", "optional", "#a27bda", "#7548b0", "Ivory clapperboard with a lifted striped slate"),
    ("vj", "VJ", "optional", "#b575ba", "#80498e", "Plum record deck, rose center and ivory pickup arm"),
    ("fab", "Fab", "optional", "#dfb36a", "#b17e3e", "Folded kraft cube with a diagonal packing band"),
    ("studio", "Director", "optional", "#8d9fa2", "#596f7b", "Ivory director monitor with two amber film reels"),
]


# Outlined versions of the same objects for Omarchy's single-ink renderer.
# Filled nested objects would merge into one silhouette when tinted.
MONO = {
    "mail": 'M13 25 V49 H51 V25 L32 39 Z M13 25 L25 34 M51 25 L39 34 M20 29 V14 H44 V29 M25 21 H39',
    "calendar": 'M18 15 H46 Q50 15 50 19 V49 Q50 52 46 52 H18 Q14 52 14 48 V19 Q14 15 18 15 Z M14 25 H50 M23 11 V19 M41 11 V19 M23 32 V44 M33 32 H41 L34 37 H38 Q44 39 40 44 H33',
    "calculator": 'M22 11 H42 Q47 11 47 16 V48 Q47 53 42 53 H22 Q17 53 17 48 V16 Q17 11 22 11 Z M22 17 H42 V26 H22 Z M23 33 H25 M31 33 H33 M23 42 H25 M31 42 H33 M40 33 V43 M38 36 H42 M38 40 H42',
    "clock": 'M32 11 A21 21 0 1 0 32 53 A21 21 0 1 0 32 11 M32 18 V20 M32 44 V46 M18 32 H20 M44 32 H46 M32 22 V32 L40 36',
    "notes": 'M39 12 H20 Q16 12 16 16 V48 Q16 52 20 52 H40 M22 12 V52 M27 22 H40 M27 29 H37 M44 29 L49 32 L40 50 L35 53 L35 47 Z M44 29 L46 25 L51 28 L49 32',
    "reminders": 'M24 14 H20 Q16 14 16 18 V48 Q16 52 20 52 H44 Q48 52 48 48 V18 Q48 14 44 14 H40 M25 11 H39 V18 H25 Z M21 28 L24 31 L29 25 M35 27 H42 M22 38 H28 V44 H22 Z M35 39 H42 M35 44 H39',
    "weather": 'M17 33 C12 19 28 12 35 24 M25 11 V13 M11 25 H13 M14 14 L17 17 M36 14 L34 17 M19 46 C9 46 10 34 20 34 C20 22 38 23 39 34 C52 30 55 46 45 46 Z M25 51 L23 54 M36 51 L34 54',
    "sheets": 'M20 12 H47 Q50 12 50 16 V48 Q50 51 47 51 H20 Q18 51 18 49 M18 28 V16 Q18 12 20 12 M18 22 H50 M24 29 H28 M35 29 H39 M43 29 H46 M35 37 H39 M43 37 H46 M35 45 H39 M43 45 H46 M13 30 H31 V48 H13 Z M17 35 H26 M17 40 H24 M17 44 H26',
    "settings": 'M19 12 H45 Q51 12 51 18 V45 Q51 51 45 51 H19 Q13 51 13 45 V18 Q13 12 19 12 Z M22 20 V26 M22 33 V43 M32 20 V36 M32 43 V45 M42 20 V22 M42 29 V43 M18 26 H26 V33 H18 Z M28 36 H36 V43 H28 Z M38 22 H46 V29 H38 Z',
    "updater": 'M24 13 C14 17 10 29 14 39 L11 36 M14 39 L19 36 M40 51 C50 47 54 35 50 25 L53 28 M50 25 L45 28 M24 24 L32 15 L40 24 M32 15 V38 M24 39 V47 H40 V39',
    "terminal": 'M17 13 H47 Q53 13 53 19 V43 Q53 49 47 49 H17 Q11 49 11 43 V19 Q11 13 17 13 Z M11 23 H53 M17 18 H18 M22 18 H23 M20 30 L26 35 L20 40 M33 40 H43',
    "octoscode": 'M17 13 H47 Q53 13 53 19 V43 Q53 49 47 49 H17 Q11 49 11 43 V19 Q11 13 17 13 Z M11 23 H53 M17 18 H18 M22 18 H23 M23 29 L17 35 L23 41 M41 29 L47 35 L41 41 M35 28 L29 42',
    "reference": 'M43 12 H20 Q16 12 16 16 V48 Q16 52 20 52 H47 V12 H43 M22 12 V52 M35 12 V31 L39 28 L43 31 V12 M27 23 H30 M27 30 H30 M27 38 H42 M27 45 H42',
    "appcard": 'M17 18 L12 19 L18 53 L42 49 M24 12 H45 Q50 12 50 17 V46 Q50 51 45 51 H24 Q19 51 19 46 V17 Q19 12 24 12 Z M26 22 H44 V34 H35 L29 39 V34 H26 Z M30 28 H31 M35 28 H36 M40 28 H41 M26 45 H42',
    "browser": 'M32 10.5 A21 21 0 1 0 32 52.5 A21 21 0 1 0 32 10.5 M32 15.5 V17 M32 46 V47.5 M16 31.5 H17.5 M46.5 31.5 H48 M39.5 18.51 L37.46 30.04 L44.99 39 L33.46 36.96 L24.5 44.49 L26.54 32.96 L19.01 24 L30.54 26.04 Z',
    "files": 'M15 23 V16 H34 M23 23 V12 H44 V29 M29 18 H39 M14 28 H28 L33 33 H51 L48 49 Q48 52 44 52 H16 Q12 52 12 48 V28 Z M18 40 H30',
    "task": 'M16 14 H48 Q52 14 52 18 V44 Q52 48 48 48 H16 Q12 48 12 44 V18 Q12 14 16 14 Z M18 34 H24 L28 25 L33 40 L38 31 H46 M32 48 V53 M26 53 H38',
    "finance": 'M36 52 H17 V12 H37 L45 20 V31 M37 12 V20 H45 M23 22 H33 M23 28 H32 M23 44 V37 M29 44 V33 M43 33 A10 10 0 1 0 43 53 A10 10 0 1 0 43 33 M46 38 H42 Q38 41 43 43 Q49 46 41 48 M43 37 V49',
    "route": 'M39 22 L25 18 L13 22 V50 L25 46 L39 50 L51 46 V35 M25 18 V46 M39 38 V50 M19 42 L27 33 L36 37 L43 30 M46 15 C37 15 36 25 46 35 C56 25 55 15 46 15 Z M46 20 A3 3 0 1 0 46 26 A3 3 0 1 0 46 20',
    "image": 'M17 16 L11 17 L16 51 L43 47 M18 13 H50 V46 H18 Z M22 40 V19 H46 V40 Z M22 36 L29 28 L37 40 M32 35 L39 29 L46 36 M39 22 H40',
    "pdf": 'M20 12 H37 L46 21 V53 H17 V15 Q17 12 20 12 Z M37 12 V21 H46 M23 44 C30 33 34 23 32 23 C27 25 33 40 42 40 C48 38 32 35 23 44 Z M23 19 H31',
    "mixer": 'M19 13 H45 L50 51 H14 Z M23 22 L22 30 M21 36 L20 44 M32 22 V36 M32 43 V45 M41 22 L42 26 M43 33 L44 44 M18 30 H26 V36 H18 Z M28 36 H36 V43 H28 Z M38 26 H46 V33 H38 Z M23 18 H24 M32 18 H33 M41 18 H42',
    "fabric": 'M24 13 L16 17 L12 29 L20 32 L23 26 V49 H41 V26 L44 32 L52 29 L48 17 L40 13 Q32 21 24 13 Z M25 14 Q32 24 39 14 M29 29 H38 V39 H29 Z',
    "score": 'M25 44 V20 L47 13 V40 M29 45 V27 L43 23 V38 M29 45 C29 52 14 55 14 49 C14 44 22 42 29 45 M47 40 C47 47 33 50 33 44 C33 39 40 37 47 40 M29 20 V27 M43 16 V23',
    "video": 'M14 23 H51 V48 H12 V23 M12 13 L50 7 L52 17 L14 23 Z M21 12 L17 22 M34 10 L30 20 M47 8 L43 18 M27 29 L39 36 L27 43 Z',
    "vj": 'M30 12 A21 21 0 1 0 30 54 A21 21 0 1 0 30 12 M30 21 A12 12 0 1 0 30 45 A12 12 0 1 0 30 21 M30 29 A4 4 0 1 0 30 37 A4 4 0 1 0 30 29 M51 14 L47 19 L43 40 L38 43',
    "fab": 'M32 11 L51 22 V43 L32 53 L13 43 V22 Z M13 22 L32 33 L51 22 M32 33 V53 M22 17 L41 28 V37 L46 34 V25 L27 14',
    "studio": 'M17 19 H47 Q51 19 51 23 V44 Q51 48 47 48 H17 Q13 48 13 44 V23 Q13 19 17 19 Z M18 24 H46 V42 H18 Z M28 27 L37 33 L28 39 Z M32 48 V53 M26 53 H38 M20 9 A4 4 0 1 0 20 17 A4 4 0 1 0 20 9 M32 9 A4 4 0 1 0 32 17 A4 4 0 1 0 32 9',
}


def monochrome(spec):
    name, label = spec[:2]
    scale = {"video": 0.78, "studio": 0.84, "weather": 0.84}.get(name, 0.86)
    art = line(MONO[name], "#d5dce8", 2.4)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
            f'<title>{label}</title>'
            + group(art, f"translate(32 32) scale({scale}) translate(-32 -32)") + '</svg>\n')


def shipping_files():
    """The approved masters are copied unchanged; only Omarchy gets line art."""
    family = ROOT / "crates/shell/resources/icons/apps/family"
    files = {}
    for spec in SPECS:
        name = spec[0]
        if name == "rinx":
            files[family / "rinx.png"] = (OUT / "assets/rinx.png").read_bytes()
            continue
        data = source(spec).encode()
        files[family / f"{name}.svg"] = data
        files[family / "omarchy" / f"{name}.svg"] = monochrome(spec).encode()
        if name in ("mail", "calendar"):
            files[ROOT / f"apps/{name}/bundle/icon.svg"] = data
    for name in ("LICENSE", "LICENSE-MIT", "NOTICE"):
        files[family / "rinx-attribution" / name] = (OUT / "references/rinx" / name).read_bytes()
    provenance = json.loads((OUT / "references/rinx/provenance.json").read_text())
    original = dict(provenance["files"][0], local="../rinx.png")
    provenance["files"] = [original]
    files[family / "rinx-attribution/provenance.json"] = (json.dumps(provenance, indent=2) + "\n").encode()
    return files


def install_shipping(check=False):
    files = shipping_files()
    provenance = json.loads((OUT / "references/rinx/provenance.json").read_text())
    for item in provenance["files"]:
        assert hashlib.sha256((OUT / item["local"]).read_bytes()).hexdigest() == item["sha256"]
    for spec in SPECS:
        if spec[0] != "rinx":
            assert (OUT / "assets" / f"{spec[0]}.svg").read_text() == source(spec), f"review master differs: {spec[0]}"
    for target, data in files.items():
        if check:
            assert target.is_file() and target.read_bytes() == data, f"out of date: {target.relative_to(ROOT)}"
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
    print(f"PASS: {len(files)} shipping resources {'match approved sources' if check else 'installed'}.")


def source(spec):
    name, label, _, top, bottom, _ = spec
    if name == "rinx":
        data = base64.b64encode((OUT / "references/rinx/icon_128.png").read_bytes()).decode()
        return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64" data-original="rinx">'
                f'<title>Rinx — original artwork</title><image width="64" height="64" href="data:image/png;base64,{data}"/></svg>')
    scale = {"video": 0.84, "studio": 0.88}.get(name, 0.9)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">'
            f'<title>{label}</title><defs><linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">'
            f'<stop stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient></defs>'
            + rect(0, 0, 64, 64, "url(#tile)")
            + f'<g id="artwork" transform="translate(32 32) scale({scale}) translate(-32 -32)">{ART[name]}</g></svg>\n')


def framed(svg, style):
    """Review approximation of the unchanged shell frame geometry."""
    frame = {
        "macos": rect(3, 3, 58, 57, "url(#tile)", 13),
        "android": circle(32, 32, 31, "url(#tile)"),
        "ios": rect(1, 1, 62, 62, "url(#tile)", 15),
    }[style]
    if 'data-original="rinx"' in svg:
        frame = frame.replace('fill="url(#tile)"', 'fill="#ffffff"')
        original = svg[svg.index('<image'):svg.index('</svg>')]
        return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">'
                f'<defs><clipPath id="frame">{frame}</clipPath></defs>'
                f'<g clip-path="url(#frame)">{rect(0, 0, 64, 64, IVORY)}{original}</g></svg>')
    return svg.replace(rect(0, 0, 64, 64, "url(#tile)"), frame, 1)


def uri(svg):
    return "data:image/svg+xml;base64," + base64.b64encode(svg.encode()).decode()


def image(svg, x, y, size):
    return f'<image x="{x}" y="{y}" width="{size}" height="{size}" href="{uri(svg)}"/>'


def text(value, x, y, size=16, color="#354a47", extra=""):
    return f'<text x="{x}" y="{y}" font-family="Helvetica,Arial,sans-serif" font-size="{size}" fill="{color}" {extra}>{value}</text>'


def save_sheet(name, body, width, height):
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{body}</svg>'
    target = OUT / f"{name}.svg"
    target.write_text(svg)
    subprocess.run(["rsvg-convert", "-o", str(OUT / f"{name}.png"), str(target)], check=True)


def reference_icons():
    paths = {
        "News": "crates/shell/resources/icons/apps/macos/news.svg",
        "Photos": ".sources/makepad/widgets/themes/macos/icons/photos.svg",
        "App Hub": "crates/shell/resources/icons/apps/apphub.svg",
        "Maps": "crates/shell/resources/icons/apps/macos/maps.svg",
        "OctoSense": "desktop/packaging/icons/icon.svg",
    }
    return {name: (ROOT / file).read_text() for name, file in paths.items()}


def current_path(name):
    if name in ("settings", "updater", "rinx", "octoscode", "reference", "appcard"):
        name = "app"
    return f".sources/makepad/widgets/themes/macos/icons/{name}.svg"


def overview():
    width, cell = 1320, 244
    body = rect(0, 0, width, 1510, "#f0f2ed")
    body += text("OCTOSENSE / ICON FAMILY", 50, 55, 15, "#52756c", 'letter-spacing="3"')
    body += text("Familiar objects. Individual character.", 50, 104, 36, INK)
    body += text("14 primary replacements + original Rinx artwork • proposal 02", 50, 138, 17)
    body += text("THE REFERENCES / EXISTING ARTWORK", 50, 190, 13, "#52756c", 'letter-spacing="2"')
    for i, (name, svg) in enumerate(reference_icons().items()):
        x = 50 + i * cell
        body += image(svg, x + 58, 211, 112)
        body += text(name, x + 114, 349, 16, extra='text-anchor="middle"')
    body += line("M50 385 H1270", "#ccd7ce", 1)
    body += text("THE NEW FAMILY / PRIMARY APPS", 50, 428, 13, "#52756c", 'letter-spacing="2"')
    for i, spec in enumerate(s for s in SPECS if s[2] == "primary"):
        x, y = 50 + i % 5 * cell, 460 + i // 5 * 285
        svg = source(spec)
        body += image(framed(svg, "macos"), x + 30, y, 168)
        body += text(spec[1], x + 114, y + 199, 18, INK, 'text-anchor="middle"')
        # Same source at actual badge and launcher sizes, on dark ground.
        body += rect(x + 57, y + 214, 114, 52, "#233d40", 9)
        body += image(framed(svg, "android"), x + 66, y + 228, 24)
        body += image(framed(svg, "ios"), x + 103, y + 220, 40)
    body += text("Square masters; the shell owns the frame. Existing custom identities remain intact.", 50, 1434, 16)
    body += text("Small previews: 24 px Android + 40 px iOS on dark ground. Static artwork review; device rendering unverified.", 50, 1463, 13, "#688078")
    save_sheet("family-primary", body, width, 1510)


def optional_sheet():
    width, height, cell = 1320, 1030, 244
    body = rect(0, 0, width, height, "#f0f2ed")
    body += text("OCTOSENSE / EXTENDED FAMILY", 50, 55, 15, "#52756c", 'letter-spacing="3"')
    body += text("14 optional catalog apps", 50, 103, 36, INK)
    body += text("Checkout tools and demos: availability depends on the build and installed binaries.", 50, 138, 17)
    for i, spec in enumerate(s for s in SPECS if s[2] == "optional"):
        x, y = 50 + i % 5 * cell, 188 + i // 5 * 260
        svg = source(spec)
        body += image(framed(svg, "macos"), x + 30, y, 168)
        body += text(spec[1], x + 114, y + 199, 18, INK, 'text-anchor="middle"')
        body += image(framed(svg, "android"), x + 76, y + 218, 24)
        body += image(framed(svg, "ios"), x + 113, y + 210, 40)
    body += text("AI pane, file-type glyphs, headless craft engines and third-party app brands are outside this scope.", 50, 1000, 15)
    save_sheet("family-optional", body, width, height)


def comparison():
    width, height = 1320, 1380
    body = rect(0, 0, width, height, "#f0f2ed")
    body += text("OCTOSENSE / BEFORE AND AFTER", 50, 55, 15, "#52756c", 'letter-spacing="3"')
    body += text("From placeholders to identities", 50, 105, 36, INK)
    body += text("Current → proposed. Six apps share the same default artwork today.", 50, 140, 17)
    primary = [s for s in SPECS if s[2] == "primary"]
    for i, spec in enumerate(primary):
        x, y = 50 + i % 3 * 420, 185 + i // 3 * 228
        current = (ROOT / current_path(spec[0])).read_text()
        body += image(current, x + 10, y, 136)
        body += text("→", x + 165, y + 75, 24, "#6e8c7c")
        body += image(framed(source(spec), "macos"), x + 205, y, 136)
        body += text(spec[1], x + 180, y + 174, 18, INK, 'text-anchor="middle"')
    save_sheet("before-after", body, width, height)


def platform_sheet():
    width, height = 1320, 162 + len(SPECS) * 94
    body = rect(0, 0, width, height, "#f0f2ed")
    body += text("OCTOSENSE / SHAPE AND SIZE REVIEW", 40, 45, 25, INK)
    body += text("Static frame approximations; shipping renderer and devices are unverified.", 40, 78, 15)
    grounds = (("Light ground", "#f0f2ed", "#233d40"), ("Dark ground", "#233d40", "#e7eddf"))
    for g, (label, ground, ink) in enumerate(grounds):
        x = 240 + 520 * g
        body += rect(x, 100, 500, height - 120, ground, 12)
        body += text(label + " / 64, 40, 24 px", x + 20, 125, 14, ink)
        for i, spec in enumerate(SPECS):
            y = 150 + i * 94
            for s, style in enumerate(("macos", "android", "ios")):
                framed_svg = framed(source(spec), style)
                start = x + 12 + s * 162
                body += image(framed_svg, start, y, 64)
                body += image(framed_svg, start + 66, y + 12, 40)
                body += image(framed_svg, start + 111, y + 20, 24)
                body += text(style, start + 3, y + 80, 11, ink)
    for i, spec in enumerate(SPECS):
        body += text(spec[1], 40, 188 + i * 94, 16, INK)
    save_sheet("platform-review", body, width, height)


def revision_sheet():
    width, height = 1080, 430
    body = rect(0, 0, width, height, "#f0f2ed")
    body += text("OCTOSENSE / REVISED ICONS", 40, 45, 15, "#52756c", 'letter-spacing="3"')
    captions = {"clock": "Two colors, simpler dial", "sheets": "Uniform cell color", "rinx": "Original Rinx artwork"}
    for i, name in enumerate(captions):
        spec = next(s for s in SPECS if s[0] == name)
        x = 40 + 340 * i
        body += image(framed(source(spec), "macos"), x + 60, 95, 192)
        body += text(spec[1], x + 156, 330, 23, INK, 'text-anchor="middle"')
        body += text(captions[name], x + 156, 362, 16, extra='text-anchor="middle"')
    save_sheet("revised-icons", body, width, height)


def validate():
    """Measure real rasterized foreground, not hand-estimated path bounds."""
    from PIL import Image

    ns = "{http://www.w3.org/2000/svg}"
    ids = {spec[0] for spec in SPECS}
    expected = {p.stem for p in (OUT / "assets").iterdir() if p.suffix in (".svg", ".png")}
    assert ids == expected, "Proposal files and inventory differ"
    assert len({hashlib.sha256(art.encode()).digest() for art in ART.values()}) == 28
    clock = ET.fromstring(source(next(s for s in SPECS if s[0] == "clock")))
    colors = {e.get(key) for e in clock.iter() for key in ("fill", "stroke", "stop-color")}
    assert colors - {None, "none", "url(#tile)"} == {IVORY, "#304b55"}
    sheets = ET.fromstring(source(next(s for s in SPECS if s[0] == "sheets")))
    cells = [e for e in sheets.iter(ns + "rect") if e.get("x") in ("24", "34", "42") and e.get("y") in ("27", "36", "43")]
    assert len(cells) == 9 and {e.get("fill") for e in cells} == {"#b9d9c7"}
    native = json.loads((ROOT / "native-apps.json").read_text())["apps"]
    provenance = json.loads((OUT / "references/rinx/provenance.json").read_text())
    assert next(a for a in native if a["id"] == "rinx")["source"]["rev"] == provenance["revision"]
    for original in provenance["files"]:
        assert hashlib.sha256((OUT / original["local"]).read_bytes()).hexdigest() == original["sha256"]
    primary = {a["id"] for a in native if any(s != "off" for s in a["shells"].values())}
    primary = primary - {"apphub"} | {"settings", "mail", "calendar"}
    catalog = set()
    for package in ("desktop", "phone"):
        catalog |= {a["id"] for a in json.loads((ROOT / package / "config/apps.json").read_text())}
    optional = catalog - primary - {"aichat"}
    assert primary == {s[0] for s in SPECS if s[2] == "primary"}
    assert optional == {s[0] for s in SPECS if s[2] == "optional"}
    results = []
    for spec in SPECS:
        if spec[0] == "rinx":
            p = OUT / "assets/rinx.png"
            assert Image.open(p).size == (1024, 1024)
            results.append(dict(id="rinx", treatment="restore-original", foreground_bounds="legacy artwork; retained unchanged",
                                sha256=hashlib.sha256(p.read_bytes()).hexdigest()))
            continue
        p = OUT / "assets" / f"{spec[0]}.svg"
        root = ET.fromstring(p.read_text())
        assert root.get("viewBox") == "0 0 64 64"
        assert root.find(ns + "text") is None, "Launcher icons must not depend on fonts"
        background = root.find(ns + "rect")
        assert [background.get(key) for key in ("x", "y", "width", "height")] == ["0", "0", "64", "64"]
        assert background.get("rx") == "0", "Source must not bake a platform mask"
        root.remove(background)
        pixels = subprocess.check_output(["rsvg-convert", "-w", "1024", "-h", "1024"], input=ET.tostring(root))
        bounds = Image.open(io.BytesIO(pixels)).getbbox()
        # 10.88–53.12 points, allowing one raster pixel of edge antialiasing.
        assert all(v >= 173 for v in bounds[:2]) and all(v <= 851 for v in bounds[2:]), (spec[0], bounds)
        results.append(dict(id=spec[0], foreground_bounds=[round(v / 16, 3) for v in bounds],
                            sha256=hashlib.sha256(p.read_bytes()).hexdigest()))
    report = dict(schema=1, status="pass", primary=15, optional=14,
                  new_svg_icons=28, restored_original_png_icons=1,
                  unique_new_foregrounds=28, new_svg_full_bleed_square_sources=True,
                  new_svg_foreground_within_central_66_percent=True,
                  rinx_original_hashes_verified=True,
                  clock_colors=2, sheets_uniform_cell_color="#b9d9c7",
                  review_styles=["macos", "android", "ios"], review_sizes=[64, 40, 24],
                  rendering="librsvg static review", shipping_renderer="unverified",
                  devices="unverified", icons=results)
    (OUT / "validation.json").write_text(json.dumps(report, indent=2) + "\n")
    print("PASS: catalog coverage, 28 distinct SVGs and safe-area bounds, original Rinx PNG hashes.")


def main():
    assets = OUT / "assets"
    assets.mkdir(parents=True, exist_ok=True)
    assert len(ART) == 28 and len(SPECS) == 29
    inventory = []
    for spec in SPECS:
        svg = source(spec)
        ET.fromstring(svg)
        if spec[0] != "rinx":
            (assets / f"{spec[0]}.svg").write_text(svg)
        inventory.append(dict(id=spec[0], name=spec[1], scope=spec[2],
                              colors=[] if spec[0] == "rinx" else list(spec[3:5]), concept=spec[5],
                              current_source=current_path(spec[0]),
                              proposed_source=f"assets/{spec[0]}.png" if spec[0] == "rinx" else f"assets/{spec[0]}.svg",
                              treatment="restore-original" if spec[0] == "rinx" else "new-artwork"))
    (OUT / "inventory.json").write_text(json.dumps(inventory, indent=2) + "\n")
    overview()
    optional_sheet()
    comparison()
    platform_sheet()
    revision_sheet()
    validate()
    print("Built 28 SVG proposals, retained original Rinx PNG, and refreshed five review sheets.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--install", action="store_true", help="rebuild reviews and install shipping resources")
    mode.add_argument("--check", action="store_true", help="check shipping resources without writing")
    args = parser.parse_args()
    if args.check:
        install_shipping(check=True)
    else:
        main()
        if args.install:
            install_shipping()

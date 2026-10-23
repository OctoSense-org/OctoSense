#!/usr/bin/env python3
"""Build the second Browser study set without installing any artwork."""
from pathlib import Path
import importlib.util

OUT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("browser_studies", OUT.parent / "build.py")
studies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(studies)
rect, circle, line, path = studies.rect, studies.circle, studies.line, studies.path
IVORY, MINT, TEAL, CORAL = studies.IVORY, studies.MINT, studies.TEAL, studies.CORAL
INK = "#233d40"

studies.STUDIES = [
    ("e-compass", "E / Compass", "Quiet, focused navigation",
     circle(32, 33, 21, "#194f61", 'opacity="0.16"')
     + circle(32, 31.5, 21, IVORY)
     + line("M32 14 V16 M32 47 V49 M14.5 31.5 H16.5 M47.5 31.5 H49.5", TEAL, 1.8)
     + path("M40 18 L35 34.5 L24 45 L29 28.5 Z", "#bbd9d4")
     + path("M40 18 L35 34.5 L29 28.5 Z", CORAL)
     + circle(32, 31.5, 2, IVORY)),
    ("f-world-pointer", "F / World + pointer", "Direct web interaction",
     circle(29.5, 29, 18, IVORY)
     + path("M18 15 Q23 11 29 11 L32 16 L28 21 L29 25 L24 29 L18 25 L13 25 Q14 18 18 15 Z", "#93c8bf")
     + path("M34 27 L43 23 Q48 30 45 37 L40 42 L35 36 L37 32 Z", "#b3d8ca")
     + path("M32 29 L51 41 L42 43 L38 52 Z", "#194f61", 'opacity="0.22"')
     + path("M31 27 L50 39 L41 41 L37 50 Z", CORAL)
     + line("M42 43 L47 50", CORAL, 4.2)),
    ("g-sail", "G / Sail", "An open-web explorer",
     path("M29 12 L29 39 H12 Z", IVORY)
     + path("M33 16 L49 39 H33 Z", MINT)
     + line("M31 13 V43", "#e0b28e", 2)
     + path("M12 43 H52 L44 51 H22 Z", IVORY)
     + path("M31 43 H52 L44 51 H31 Z", "#f2b38c")
     + line("M17 52 Q23 49 29 52 Q35 55 41 52 Q45 50 49 52", MINT, 2.4)),
    ("h-search-lens", "H / Search lens", "Discovery comes first",
     studies.family.group(studies.family.group(rect(29, 39, 7, 16, "#194f61", 3, 'opacity="0.18"')
                          + rect(27, 38, 8, 17, CORAL, 3)
                          + rect(27, 38, 8, 5, "#d67d5b", 1), "rotate(-42 31 35)")
     + circle(27, 27, 17, IVORY)
     + circle(27, 27, 12.5, MINT)
     + line("M15 27 H39", TEAL, 1.8)
     + '<ellipse cx="27" cy="27" rx="5.4" ry="12.5" fill="none" stroke="#398b94" stroke-width="1.8"/>'
     + line("M19 19 L21 17", IVORY, 2.4), "translate(3 0)")),
]
studies.OUT = OUT

if __name__ == "__main__":
    studies.build("Browser / a new exploration")

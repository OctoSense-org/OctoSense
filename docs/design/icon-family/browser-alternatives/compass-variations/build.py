#!/usr/bin/env python3
"""Render refinements of the selected Compass direction; review assets only."""
from pathlib import Path
import importlib.util

OUT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("browser_studies", OUT.parent / "build.py")
studies = importlib.util.module_from_spec(spec)
spec.loader.exec_module(studies)
circle, line, path = studies.circle, studies.line, studies.path
group = studies.family.group
IVORY, MINT, TEAL, CORAL = studies.IVORY, studies.MINT, studies.TEAL, studies.CORAL


def needle(top=CORAL, bottom="#bbd9d4", pin=IVORY):
    return (path("M41 17 L36 36 L23 47 L28 28 Z", bottom)
            + path("M41 17 L36 36 L28 28 Z", top)
            + circle(32, 32, 2, pin))


CARDINAL = "M32 15 V17 M32 47 V49 M15 32 H17 M47 32 H49"
DIAGONAL = "M20 20 L21 21 M43 43 L44 44 M20 44 L21 43 M43 21 L44 20"
SHADOW = circle(32, 33.5, 21, "#194f61", 'opacity="0.16"')

studies.STUDIES = [
    ("c1-essential", "C1 / Essential", "Only the dial and needle",
     circle(32, 32, 21, IVORY) + needle()),
    ("c2-mint-rim", "C2 / Mint rim", "A tactile, finished object",
     SHADOW + circle(32, 31.5, 21, MINT) + circle(32, 31.5, 18.5, IVORY)
     + line("M32 16 V17.5 M32 45.5 V47 M16.5 31.5 H18 M46 31.5 H47.5", TEAL, 1.8)
     + group(needle(), "translate(32 31.5) scale(0.92) translate(-32 -32)")),
    ("c3-compass-rose", "C3 / Compass rose", "A stronger navigation mark",
     SHADOW + circle(32, 31.5, 21, IVORY)
     + line(CARDINAL, TEAL, 1.6)
     + line(DIAGONAL, "#a6cac4", 1.4)
     + group(path("M32 17 L36 28 L47 32 L36 36 L32 47 L28 36 L17 32 L28 28 Z", MINT)
             + path("M32 17 L36 28 L32 32 L28 28 Z", CORAL)
             + path("M32 32 L36 36 L32 47 Z", "#8dbcb3")
             + circle(32, 32, 2, IVORY), "rotate(30 32 32)")),
    ("c4-night-dial", "C4 / Night dial", "Higher contrast, quieter color",
     SHADOW + circle(32, 31.5, 21, MINT) + circle(32, 31.5, 19, "#233d40")
     + line("M32 15.5 V17 M32 46 V47.5 M16 31.5 H17.5 M46.5 31.5 H48", "#8abcb5", 1.8)
     + group(needle(IVORY, "#74afa7", "#233d40"), "translate(0 -0.5)")),
]
studies.OUT = OUT

if __name__ == "__main__":
    selected = (OUT.parent / "set-2/e-compass.svg").read_text()
    studies.build("Browser / Compass refinements", reference=selected, reference_label="Selected base")

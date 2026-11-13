#!/usr/bin/env python3
"""Turn a reviewer's per-surface grades into the 95/100 result of macos-rubric.json.

A scorecard is JSON: {"reviewer": "...", "author": "...", "surfaces": {"news": {"structure": 0.95, ...,
"defects": ["..."], "states": ["..."], "gates_failed": []}}}. A surface passes when its weighted score reaches
the target and it fails no hard gate; the result passes when every in-scope surface does. Grades are a reviewer's
judgement over original captures: this script only does the arithmetic and refuses a card with a gap, and a
review by the author of the change is not independent acceptance (tools/app-ux/README.md).
"""
import argparse
import json
from pathlib import Path

RUBRIC = json.loads((Path(__file__).parent / "macos-rubric.json").read_text())


def required_surfaces():
    groups = RUBRIC["surfaces"]
    names = []
    for group, members in groups.items():
        if group != "out of scope":
            names.extend(members)
    return names


def score(grades):
    total = 0.0
    for dimension in RUBRIC["dimensions"]:
        grade = grades.get(dimension["id"])
        if not isinstance(grade, (int, float)) or not 0 <= grade <= 1:
            raise SystemExit(f"grade for {dimension['id']!r} must be a number from 0 to 1, got {grade!r}")
        total += grade * dimension["weight"]
    return round(total, 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("card", type=Path)
    parser.add_argument("--allow-missing", action="store_true", help="Score what is there; a partial card never passes")
    args = parser.parse_args()
    card = json.loads(args.card.read_text())
    surfaces = card["surfaces"]
    missing = [name for name in required_surfaces() if name not in surfaces]
    if missing and not args.allow_missing:
        raise SystemExit("not reviewed: " + ", ".join(missing))
    rows = []
    for name, grades in surfaces.items():
        rows.append((score(grades), name, grades.get("gates_failed", []), grades.get("defects", [])))
    rows.sort()
    target = RUBRIC["target"]
    for value, name, gates, defects in rows:
        mark = "PASS" if value >= target and not gates else "FAIL"
        print(f"{mark} {value:5.1f}  {name}" + (f"  gates: {'; '.join(gates)}" if gates else ""))
        for defect in defects:
            print(f"        - {defect}")
    average = sum(r[0] for r in rows) / len(rows) if rows else 0.0
    passed = not missing and all(r[0] >= target and not r[2] for r in rows)
    print(f"\n{len(rows)} surfaces, average {average:.1f}, lowest {rows[0][0] if rows else 0:.1f}; target {target}: {'PASS' if passed else 'NOT YET'}")
    print(f"Reviewer: {card.get('reviewer', 'unstated')}; author: {card.get('author', 'unstated')}")
    if card.get("reviewer") and card.get("reviewer") == card.get("author"):
        print("A self-review is not independent acceptance.")
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()

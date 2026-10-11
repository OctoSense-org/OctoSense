#!/usr/bin/env python3
"""Measure a generated desktop UX screen for pixel mapping.

App Flow's image library measures phone screens at a fixed 406 × 776 artboard
(flows/image-lib/observe.py, measure_surfaces.py). PDF Tools v2 is a desktop
app, so this applies the same two measurements to a screen at its own size:

- text: Apple Vision OCR through App Flow's unchanged flows/image-lib/ocr.swift,
  plus each run's ink colour (the darker or more saturated of the two colour
  clusters inside its box) and the background behind it;
- surfaces: closed edges at three Canny thresholds, kept when they are nearly
  rectangular, with their fill (median inside), border (median on the edge)
  and an estimated corner radius; plus long one-pixel dividers.

The output is evidence for the semantic map, not a widget tree: roles, widget
choices and every text that ships are reviewed by a person (App Flow step 8).

Usage: python measure_desktop.py --ocr <App Flow>/flows/image-lib/ocr.swift SCREEN.png...
Writes SCREEN.measured.json beside each screen (reused while the image's hash
and this script are unchanged).
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import cv2
import numpy as np
from PIL import Image


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def hexcolor(rgb):
    return "#%02x%02x%02x" % tuple(int(round(c)) for c in rgb)


def ocr(path, swift):
    raw = subprocess.check_output(["swift", "-O", str(swift), str(path)])
    return json.loads(raw)


def ink_and_ground(pixels, box):
    x, y, w, h = [int(round(v)) for v in box]
    x0, y0 = max(0, x - 1), max(0, y - 1)
    x1, y1 = min(pixels.shape[1], x + w + 1), min(pixels.shape[0], y + h + 1)
    patch = pixels[y0:y1, x0:x1].reshape(-1, 3).astype(np.float32)
    if len(patch) < 4:
        return None, None
    criteria = (cv2.TERM_CRITERIA_EPS + cv2.TERM_CRITERIA_MAX_ITER, 20, 0.5)
    _, labels, centers = cv2.kmeans(patch, 2, None, criteria, 3, cv2.KMEANS_PP_CENTERS)
    counts = np.bincount(labels.flatten(), minlength=2)
    ground = centers[int(np.argmax(counts))]
    ink = centers[int(np.argmin(counts))]
    return hexcolor(ink), hexcolor(ground)


def corner_radius(pixels, rect, fill):
    x, y, w, h = rect
    fill = np.array(fill, dtype=np.float32)
    limit = max(1, min(w, h) // 3)
    for r in range(0, limit):
        px = pixels[min(pixels.shape[0] - 1, y + r), min(pixels.shape[1] - 1, x + r)].astype(np.float32)
        if np.abs(px - fill).max() < 10:
            return int(round(r * 1.4142))
    return limit


def surfaces(pixels):
    gray = cv2.cvtColor(cv2.GaussianBlur(pixels, (3, 3), 0), cv2.COLOR_RGB2GRAY)
    height, width = gray.shape
    found = []
    for lo, hi in [(4, 12), (10, 30), (25, 70)]:
        edges = cv2.Canny(gray, lo, hi)
        edges = cv2.dilate(edges, np.ones((2, 2), np.uint8))
        contours, _ = cv2.findContours(edges, cv2.RETR_LIST, cv2.CHAIN_APPROX_SIMPLE)
        for contour in contours:
            x, y, w, h = cv2.boundingRect(contour)
            if w < 24 or h < 16 or w * h < 900 or (w > width - 4 and h > height - 4):
                continue
            area = cv2.contourArea(contour)
            if area < 0.80 * w * h:
                continue
            found.append((x, y, w, h))
    kept = []
    for rect in sorted(found, key=lambda r: -r[2] * r[3]):
        if all(iou(rect, other) < 0.85 for other in kept):
            kept.append(rect)
    out = []
    for x, y, w, h in kept:
        inner = pixels[y + 4:y + h - 4, x + 4:x + w - 4].reshape(-1, 3)
        if len(inner) == 0:
            continue
        fill = np.median(inner, axis=0)
        edge = np.concatenate([pixels[y, x:x + w], pixels[y + h - 1, x:x + w], pixels[y:y + h, x], pixels[y:y + h, x + w - 1]])
        border = np.median(edge, axis=0)
        out.append({"bounds": [x, y, w, h], "fill": hexcolor(fill), "border": hexcolor(border),
                    "radius": corner_radius(pixels, (x, y, w, h), fill)})
    return out


def iou(a, b):
    ax, ay, aw, ah = a
    bx, by, bw, bh = b
    ix = max(0, min(ax + aw, bx + bw) - max(ax, bx))
    iy = max(0, min(ay + ah, by + bh) - max(ay, by))
    inter = ix * iy
    return inter / float(aw * ah + bw * bh - inter or 1)


def dividers(pixels):
    lines = []
    h, w, _ = pixels.shape
    signed = pixels.astype(np.int16)
    for y in range(1, h - 1):
        row, above, below = signed[y], signed[y - 1], signed[y + 1]
        differs = (np.abs(row - above).max(axis=1) > 6) & (np.abs(row - below).max(axis=1) > 6)
        run = longest_run(differs)
        if run[1] - run[0] > w * 0.18:
            lines.append({"axis": "h", "at": y, "from": int(run[0]), "to": int(run[1]),
                          "color": hexcolor(np.median(pixels[y, run[0]:run[1]], axis=0))})
    for x in range(1, w - 1):
        col, left, right = signed[:, x], signed[:, x - 1], signed[:, x + 1]
        differs = (np.abs(col - left).max(axis=1) > 6) & (np.abs(col - right).max(axis=1) > 6)
        run = longest_run(differs)
        if run[1] - run[0] > h * 0.18:
            lines.append({"axis": "v", "at": x, "from": int(run[0]), "to": int(run[1]),
                          "color": hexcolor(np.median(pixels[run[0]:run[1], x], axis=0))})
    return lines


def longest_run(mask):
    best, start = (0, 0), None
    for i, on in enumerate(list(mask) + [False]):
        if on and start is None:
            start = i
        elif not on and start is not None:
            if i - start > best[1] - best[0]:
                best = (start, i)
            start = None
    return best


def measure(path, swift):
    path = Path(path)
    data = path.read_bytes()
    out = path.with_suffix(".measured.json")
    me = sha256(Path(__file__).read_bytes())
    if out.exists():
        old = json.loads(out.read_text())
        if old.get("image_sha256") == sha256(data) and old.get("script_sha256") == me:
            return out
    pixels = np.asarray(Image.open(path).convert("RGB"))
    text = ocr(path, swift)
    runs = []
    for o in text["observations"]:
        ink, ground = ink_and_ground(pixels, o["bounds"])
        runs.append({"text": o["text"], "bounds": [round(v, 1) for v in o["bounds"]],
                     "confidence": round(o["confidence"], 2), "ink": ink, "ground": ground})
    result = {
        "schema_version": 1,
        "image": path.name,
        "image_sha256": sha256(data),
        "script_sha256": me,
        "size": [int(pixels.shape[1]), int(pixels.shape[0])],
        "ocr_engine": "Apple Vision VNRecognizeTextRequest accurate, en-US (App Flow ocr.swift)",
        "text": runs,
        "surfaces": surfaces(pixels),
        "dividers": dividers(pixels),
        "limits": ["Evidence for review: roles and widgets are assigned in the semantic map, not here."],
    }
    out.write_text(json.dumps(result, indent=1) + "\n")
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--ocr", required=True, help="App Flow's flows/image-lib/ocr.swift")
    parser.add_argument("screens", nargs="+")
    args = parser.parse_args()
    for screen in args.screens:
        print(measure(screen, args.ocr))


if __name__ == "__main__":
    main()

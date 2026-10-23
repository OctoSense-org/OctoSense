#!/usr/bin/env python3
"""Capture/check the approved family through the production hidden renderer.

Build icon_shapes from phone/ first. --capture runs five hidden previews with
temporary, separate Home/Rinx directories and closes each through /quit.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import struct
import tempfile
import time
import urllib.request
import urllib.error

from check_icon_shape_preview import check as check_shapes

ROOT = Path(__file__).resolve().parents[1]


def capture(binary, directory):
    directory.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="octosense-icon-gpu-") as isolated:
        for name, args in [("default", [])] + [(f"family-{i}", [f"--family-page={i}"]) for i in range(4)]:
            home = Path(isolated) / name / "home"
            rinx = Path(isolated) / name / "rinx"
            home.mkdir(parents=True)
            rinx.mkdir(parents=True)
            env = dict(os.environ, OCTOSENSE_HOME=str(home), RINX_DATA_DIR=str(rinx),
                       MAKEPAD_HIDE_WINDOWS="1", MAKEPAD_REMOTE="127.0.0.1:0")
            log_path = directory / f"{name}.log"
            url = None
            with log_path.open("wb") as log:
                process = subprocess.Popen([str(binary), *args], cwd=ROOT, env=env, stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 45
                    while time.monotonic() < deadline:
                        match = re.search(r"listening on 127\.0\.0\.1:(\d+)", log_path.read_text(errors="replace"))
                        if match:
                            url = f"http://127.0.0.1:{match[1]}"
                            break
                        if process.poll() is not None:
                            raise RuntimeError(f"{name} exited; inspect {log_path}")
                        time.sleep(0.1)
                    if url is None:
                        raise RuntimeError(f"{name} control port did not start")
                    # The control socket starts before the first window exists.
                    # A grab can return 404 during that short startup interval.
                    deadline = time.monotonic() + 30
                    while True:
                        try:
                            with urllib.request.urlopen(url + "/g?scale=1&raw=1", timeout=30) as response:
                                data = response.read()
                            width, height = struct.unpack(">II", data[16:24])
                            assert width * 920 == height * 1040, (width, height)
                            if width != 1040:
                                # Remote scale multiplies native backing pixels,
                                # which are 2x logical pixels on Retina displays.
                                scale = 1040 / width
                                with urllib.request.urlopen(url + f"/g?scale={scale}&raw=1", timeout=30) as response:
                                    data = response.read()
                            (directory / f"{name}.png").write_bytes(data)
                            break
                        except urllib.error.HTTPError as error:
                            if error.code != 404 or time.monotonic() >= deadline:
                                raise RuntimeError(f"{name} grab failed: {error.read().decode(errors='replace')}") from error
                            time.sleep(0.1)
                finally:
                    if url:
                        try:
                            urllib.request.urlopen(url + "/quit", timeout=5).close()
                        except (OSError, TimeoutError):
                            pass
                    try:
                        process.wait(timeout=8)
                    except subprocess.TimeoutExpired:
                        process.terminate()
                        process.wait(timeout=5)


def check(directory, body_y):
    from PIL import Image

    check_shapes(directory / "default.png", body_y)
    pages = [Image.open(directory / f"family-{i}.png").convert("RGB") for i in range(4)]
    assert all(image.size == (1040, 920) for image in pages)
    checks = 0

    def pixel(page, x, y, expected, message, tolerance=3):
        nonlocal checks
        actual = pages[page].getpixel((x, y + body_y))
        assert max(abs(a - b) for a, b in zip(actual, expected)) <= tolerance, (message, actual, expected)
        checks += 1

    for page in range(4):
        for row in range(4):  # Android, macOS, iOS, Windows rounded frames.
            for col in range(8):
                pixel(page, 135 + col * 110 + 3, 32 + row * 120 + 4,
                      (36, 48, 68), f"page {page} row {row} col {col}: masked corner")
        for row in (5, 6):  # Freeform themes must reach the family variants.
            for col in range(8 if page < 3 else 5):
                pixel(page, 135 + col * 110 + 1, 32 + row * 120 + 1,
                      (36, 48, 68), f"page {page} row {row} col {col}: no full-bleed family tile")
    for row in range(4):
        y = 32 + row * 120
        x = 135 + 3 * 110  # Clock.
        pixel(0, x + 32, y + 8, (48, 75, 85), "Clock: uniform dark ground")
        pixel(0, x + 26, y + 24, (255, 253, 247), "Clock: ivory dial")
        pixel(0, x + 32, y + 32, (48, 75, 85), "Clock: dark hands")
        x = 135 + 7 * 110  # Sheets, all six unobscured cells.
        for dx in (36, 44):
            for dy in (30, 38, 44):
                pixel(0, x + dx, y + dy, (185, 217, 199), "Sheets: every visible cell has the same fill")
        original = Image.open(ROOT / "docs/design/icon-family/assets/rinx.png").convert("RGB")
        pixel(1, 135 + 4 * 110 + 32, y + 20, original.getpixel((512, 320)), "Rinx: original teal face", tolerance=5)
        x = 135 + 7 * 110  # Browser: approved C5 Night rose.
        pixel(1, x + 32, y + 32, (35, 61, 64), "Browser: ink dial and center pin")
        pixel(1, x + 37, y + 23, (255, 253, 247), "Browser: ivory northeast point", tolerance=6)
        pixel(1, x + 23, y + 26, (168, 209, 200), "Browser: mint west spoke", tolerance=6)
    report = dict(status="pass", host="macOS", devices="unverified", family_icons=29,
                  browser_design="C5 Night rose",
                  styles=["android", "macos", "ios", "windows", "nextstep", "omarchy", "windows-2000"],
                  sizes=[64, 24], opacities=[1.0, 0.5], family_pixel_checks=checks,
                  captures=["default.png"] + [f"family-{i}.png" for i in range(4)])
    (directory / "validation.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"PASS: {checks} production family pixels, including C5 Browser, Clock, Sheets and original Rinx.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture", action="store_true")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/debug/examples/icon_shapes")
    parser.add_argument("--directory", type=Path, default=ROOT / "target/icon-family/gpu")
    parser.add_argument("--body-y", type=int, default=32, help="macOS remote caption height in logical pixels")
    args = parser.parse_args()
    if args.capture:
        capture(args.binary.resolve(), args.directory)
    check(args.directory, args.body_y)

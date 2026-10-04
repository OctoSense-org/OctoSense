#!/usr/bin/env python3
"""Check a scale=1 remote PNG of the icon_shapes example (optional: Pillow).

This checks real GPU pixels, including SVG clipping and group opacity, which
cannot be established by testing Rust geometry or shader registration alone.
The standard macOS preview has a 32-point caption; use --body-y for another
backend's body position, as reported by the remote /snap endpoint.
"""
import argparse


def check(path, body_y):
    from PIL import Image

    image = Image.open(path).convert("RGB")
    if image.size != (1040, 920):
        raise AssertionError("Capture the 1040x920 icon_shapes window at scale=1")
    checks = 0

    def pixel(x, y, expected, message, tolerance=1):
        nonlocal checks
        actual = image.getpixel((x, y + body_y))
        assert max(abs(a - b) for a, b in zip(actual, expected)) <= tolerance, (
            f"{message} at ({x}, {y + body_y}): {actual}, expected {expected}"
        )
        checks += 1

    dark = (36, 48, 68)
    red = (228, 87, 87)
    for row, style in enumerate(["Android", "macOS", "iOS", "Windows"]):
        y = 32 + row * 120
        for col, name in enumerate(["photos", "apphub", "assistant", "camera",
                                    "ai-providers", "youtube", "shape-svg", "shape-png"]):
            if name == "photos":  # Framework freeform artwork on Windows.
                continue
            x = 135 + col * 110
            pixel(x + 3, y + 4, dark, f"{style} {name}: outer corner must be masked")
        # Check a colored point inside the opaque SVG, not its white center.
        pixel(806, y + 31, red, f"{style} SVG: preserve artwork color")
    # A square SVG must fade exactly once after its neutral backing is composed.
    light = (237, 241, 248)
    half_red = tuple(round((a + b) / 2) for a, b in zip(red, light))
    pixel(834, 118, half_red, "SVG: half-opacity composition")
    pixel(921, 42, (255, 253, 247), "transparent PNG: platform-shaped backing")
    print(f"PASS: {checks} rendered icon shape/color/fade checks")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("png")
    parser.add_argument("--body-y", type=int, default=32)
    args = parser.parse_args()
    check(args.png, args.body_y)

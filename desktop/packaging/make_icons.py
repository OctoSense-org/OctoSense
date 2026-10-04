#!/usr/bin/env python3
"""OctoSense's platform app icons, rendered from icons/icon.svg's geometry.

The mark is OctoSense's eight-petal flower on its green tile (the website's
favicon). Desktop art has an inset rounded tile; mobile catalogs use an
opaque square and let the OS mask its corners. Android also has separate
adaptive layers and a monochrome layer for themed icons.

    python3 desktop/packaging/make_icons.py          # regenerate all platforms
    python3 desktop/packaging/make_icons.py --check  # compare without writing

Stdlib only (Python 3.9+), deterministic: the output is committed, so a
release build never renders anything. Edit TILE/PETAL/the geometry here and
in icon.svg together.
"""
import argparse
import json
import math
from pathlib import Path
import struct
import zlib

ROOT = Path(__file__).resolve().parents[2]
SIZES = (32, 64, 128, 256, 512, 1024)  # the sizes an .icns takes
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)

# In icon.svg's 1024 user units.
TILE = (0x24, 0x3F, 0x30)
PETAL = (0xD4, 0xED, 0xB8)
INSET, RADIUS = 100.0, 185.0          # the tile: 824 wide, macOS-like corners
CENTER, SCALE = 512.0, 824.0 / 64.0   # the favicon's 64-unit flower, scaled to the tile
PETAL_RX, PETAL_RY, PETAL_CY = 5.0 * SCALE, 10.0 * SCALE, -15.0 * SCALE
ROTATIONS = [(math.cos(-math.radians(45.0 * k)), math.sin(-math.radians(45.0 * k))) for k in range(8)]


def tile_distance(x, y):
    """Signed distance (units) to the rounded tile; negative inside."""
    lo, hi = INSET + RADIUS, 1024.0 - INSET - RADIUS
    dx = max(lo - x, 0.0, x - hi)
    dy = max(lo - y, 0.0, y - hi)
    if dx > 0 and dy > 0:
        return math.hypot(dx, dy) - RADIUS
    inner = min(x - INSET, 1024.0 - INSET - x, y - INSET, 1024.0 - INSET - y)
    return max(dx, dy) - RADIUS if (dx or dy) else -inner


def petal_distance(x, y):
    """Approximate signed distance to the nearest of the eight petals."""
    px, py = x - CENTER, y - CENTER
    best = float("inf")
    for cosine, sine in ROTATIONS:
        rx = px * cosine - py * sine
        ry = px * sine + py * cosine
        f = math.hypot(rx / PETAL_RX, (ry - PETAL_CY) / PETAL_RY)
        best = min(best, (f - 1.0) * PETAL_RX)
    return best


def render(size, style="desktop"):
    """RGBA rows, 4x4 supersampled where an edge crosses the pixel."""
    unit = 1024.0 / size
    rows = []
    offsets = [(i + 0.5) / 4.0 for i in range(4)]
    for j in range(size):
        row = bytearray()
        for i in range(size):
            cx, cy = (i + 0.5) * unit, (j + 0.5) * unit
            near = 1.5 * unit
            dt = tile_distance(cx, cy) if style == "desktop" else -1024.0
            dp = petal_distance(cx, cy)
            if abs(dt) > near and abs(dp) > near:
                tile = 1.0 if dt < 0 else 0.0
                petal = 1.0 if dp < 0 and tile else 0.0
            else:
                tile = petal = 0.0
                for oy in offsets:
                    for ox in offsets:
                        sx, sy = (i + ox) * unit, (j + oy) * unit
                        if style != "desktop" or tile_distance(sx, sy) < 0:
                            tile += 1 / 16
                            if petal_distance(sx, sy) < 0:
                                petal += 1 / 16
            if style == "foreground":
                row += bytes([*PETAL, round(255 * petal)])
                continue
            if tile == 0:
                row += b"\0\0\0\0"
                continue
            mix = petal / tile
            rgb = [round(t + (p - t) * mix) for t, p in zip(TILE, PETAL)]
            row += bytes(rgb + [round(255 * tile)])
        rows.append(bytes(row))
    return rows


def png(size, rows, alpha=True):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    if not alpha:
        rows = [bytes(value for i, value in enumerate(row) if i % 4 != 3) for row in rows]
    raw = b"".join(b"\0" + r for r in rows)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6 if alpha else 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def ico(images):
    """An .ico of PNG entries (Vista+), smallest first."""
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = len(header) + 16 * len(images)
    entries, blobs = b"", b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    return header + entries + blobs


def icns(images):
    """PNG-backed macOS icon family (including Retina representations)."""
    slots = ((b"icp5", 32), (b"icp6", 64), (b"ic07", 128), (b"ic08", 256),
             (b"ic09", 512), (b"ic10", 1024), (b"ic11", 32), (b"ic12", 64),
             (b"ic13", 256), (b"ic14", 512))
    body = b"".join(kind + struct.pack(">I", len(images[size]) + 8) + images[size] for kind, size in slots)
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def ios_slots():
    slots = [(idiom, size, scale) for idiom, scales in (("iphone", (2, 3)), ("ipad", (1, 2)))
             for size in (20, 29, 40) for scale in scales]
    return slots + [("iphone", 60, 2), ("iphone", 60, 3), ("ipad", 76, 1), ("ipad", 76, 2),
                    ("ipad", 83.5, 2), ("ios-marketing", 1024, 1)]


def android_xml():
    """The same eight ellipses within the 66dp safe circle on a 108dp layer."""
    colour = "#" + "".join(f"{c:02x}" for c in PETAL)
    background = "#" + "".join(f"{c:02x}" for c in TILE)
    xmlns = 'xmlns:android="http://schemas.android.com/apk/res/android"'
    # Ellipse rx=5, ry=10, cy=-15; rotated around the flower's centre.
    path = ("M0,-25 C2.761424,-25 5,-20.522847 5,-15 C5,-9.477153 2.761424,-5 0,-5 "
            "C-2.761424,-5 -5,-9.477153 -5,-15 C-5,-20.522847 -2.761424,-25 0,-25 Z")
    petals = "\n".join(f'    <group android:rotation="{45 * k}"><path android:fillColor="{colour}" '
                       f'android:pathData="{path}" /></group>' for k in range(8))
    foreground = (f'<vector {xmlns} android:width="108dp" android:height="108dp" '
                  'android:viewportWidth="108" android:viewportHeight="108">\n'
                  '  <group android:translateX="54" android:translateY="54" android:scaleX="1.2" android:scaleY="1.2">\n'
                  f'{petals}\n  </group>\n</vector>\n')
    layers = ('  <background android:drawable="@drawable/ic_launcher_background" />\n'
              '  <foreground android:drawable="@drawable/ic_launcher_foreground" />\n')
    return {
        "drawable/ic_launcher_foreground.xml": foreground,
        "drawable/ic_launcher_background.xml": f'<shape {xmlns} android:shape="rectangle"><solid android:color="{background}" /></shape>\n',
        "mipmap-anydpi-v26/ic_launcher.xml": f'<adaptive-icon {xmlns}>\n{layers}</adaptive-icon>\n',
        # The launcher uses this drawable's alpha, then supplies the theme colour.
        "mipmap-anydpi-v33/ic_launcher.xml": f'<adaptive-icon {xmlns}>\n{layers}  <monochrome android:drawable="@drawable/ic_launcher_foreground" />\n</adaptive-icon>\n',
    }


def assets():
    """All generated paths relative to the repository; no build-time tooling."""
    output = {}
    rendered = {size: png(size, render(size)) for size in sorted(set(SIZES) | set(ICO_SIZES) | {72, 96, 144, 192})}
    ico_data = ico([(s, rendered[s]) for s in ICO_SIZES])
    icns_data = icns(rendered)
    for size in SIZES:
        # An .icns takes 1024 only as 512 at 2x, which cargo-packager reads
        # from the `@2x` in the name.
        name = "icon_512@2x.png" if size == 1024 else f"icon_{size}.png"
        output[f"desktop/packaging/icons/{name}"] = rendered[size]
    output["desktop/packaging/icons/icon.ico"] = ico_data

    mobile = {size: png(size, render(size, "mobile"), alpha=False)
              for size in sorted({int(size * scale) for _, size, scale in ios_slots()})}
    catalog = {"images": [{"idiom": idiom, "size": f"{size}x{size}", "scale": f"{scale}x",
                           "filename": f"AppIcon{int(size * scale)}x{int(size * scale)}.png"}
                          for idiom, size, scale in ios_slots()], "info": {"version": 1, "author": "xcode"}}
    for package in ("desktop", "phone"):
        for size in SIZES:
            output[f"{package}/resources/icon_{size}.png"] = rendered[size]
        output[f"{package}/resources/icon.ico"] = ico_data
        output[f"{package}/resources/icon.icns"] = icns_data
        android = f"{package}/resources/android/res"
        for density, size in (("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)):
            output[f"{android}/mipmap-{density}/ic_launcher.png"] = rendered[size]
        for name, xml in android_xml().items():
            output[f"{android}/{name}"] = xml.encode()
        ios = f"{package}/packaging/ios/icons/Assets.xcassets"
        output[f"{ios}/Contents.json"] = (json.dumps({"info": catalog["info"]}, indent=2) + "\n").encode()
        output[f"{ios}/AppIcon.appiconset/Contents.json"] = (json.dumps(catalog, indent=2) + "\n").encode()
        for size, data in mobile.items():
            output[f"{ios}/AppIcon.appiconset/AppIcon{size}x{size}.png"] = data

    output["phone/ohos/icons/app_icon.png"] = png(512, render(512, "mobile"), alpha=False)
    output["phone/ohos/icons/foreground.png"] = png(512, render(512, "foreground"))
    output["phone/ohos/icons/background.png"] = png(512, [bytes([*TILE, 255]) * 512] * 512, alpha=False)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Fail if committed assets differ; write nothing")
    args = parser.parse_args()
    output = assets()
    stale = []
    for name, data in output.items():
        path = ROOT / name
        if args.check:
            if not path.is_file() or path.read_bytes() != data:
                stale.append(name)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    if stale:
        parser.exit(1, "Stale app icons; run desktop/packaging/make_icons.py:\n" + "\n".join(stale) + "\n")
    print(f"{'checked' if args.check else 'wrote'} {len(output)} app icon assets")


if __name__ == "__main__":
    main()

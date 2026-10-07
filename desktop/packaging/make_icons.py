#!/usr/bin/env python3
"""OctoSense's platform app icons, rendered from icons/mark.svg's geometry.

The mark is OctoSense's eight-arm flower on its green tile (the website's
favicon). Desktop art has an inset rounded tile; mobile catalogs use an
opaque square and let the OS mask its corners. Android also has separate
adaptive layers and a monochrome layer for themed icons.

    python3 desktop/packaging/make_icons.py          # regenerate all platforms
    python3 desktop/packaging/make_icons.py --check  # compare without writing

Stdlib only (Python 3.9+), deterministic: the output is committed, so a
release build never renders anything. Edit the canonical cubic paths in icons/mark.svg; the desktop SVG,
raster icons and Android vector layers are generated from that same source.
"""
import argparse
import json
import math
import re
import xml.etree.ElementTree as ET
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
MARK = ET.parse(Path(__file__).parent / "icons/mark.svg").getroot()
ARM_PATH = MARK.find(".//{http://www.w3.org/2000/svg}path").get("d")


def arm_polygon():
    """Flatten the canonical M/C/Z path to subpixel-accurate line segments."""
    tokens = re.findall(r"[MCZ]|-?\d+(?:\.\d+)?", ARM_PATH)
    if tokens.pop(0) != "M":
        raise ValueError("The mark must start with M")
    point = (float(tokens.pop(0)), float(tokens.pop(0)))
    points = [point]
    while tokens:
        command = tokens.pop(0)
        if command == "Z":
            break
        if command != "C":
            raise ValueError(f"Unsupported mark command: {command}")
        coordinates = [float(tokens.pop(0)) for _ in range(6)]
        p1, p2, p3 = [coordinates[i:i + 2] for i in (0, 2, 4)]
        for step in range(1, 65):
            t = step / 64.0
            u = 1 - t
            points.append(tuple(u**3 * point[k] + 3*u*u*t*p1[k]
                                + 3*u*t*t*p2[k] + t**3*p3[k] for k in (0, 1)))
        point = p3
    return points


def mark_edges():
    edges = []
    points = arm_polygon()
    for k in range(8):
        c, s = math.cos(math.radians(45*k)), math.sin(math.radians(45*k))
        polygon = [(CENTER + SCALE*(x*c-y*s), CENTER + SCALE*(x*s+y*c)) for x, y in points]
        for (x1, y1), (x2, y2) in zip(polygon, polygon[1:] + polygon[:1]):
            if y1 != y2:
                edges.append((min(y1, y2), max(y1, y2), x1, y1, (x2-x1)/(y2-y1), k))
    return edges


EDGES = mark_edges()


def mark_coverage(size):
    """Scan-convert all arms with 4x4 sampling and a half-open edge rule."""
    samples = size * 4
    unit = 1024.0 / samples
    coverage = [bytearray(size) for _ in range(size)]
    # Bucket edges by scanline to avoid checking every curve at every pixel.
    starts = {}
    for edge in EDGES:
        first = max(0, math.ceil(edge[0] / unit - 0.5))
        end = min(samples, math.ceil(edge[1] / unit - 0.5))
        if first < end:
            starts.setdefault(first, []).append((end, edge))
    active = []
    for row in range(samples):
        active = [(end, edge) for end, edge in active if end > row]
        active.extend(starts.get(row, []))
        y = (row + 0.5) * unit
        intersections = [[] for _ in range(8)]
        for _, (_, _, x, y0, slope, arm) in active:
            intersections[arm].append(x + (y-y0)*slope)
        intervals = []
        for crossings in intersections:
            crossings.sort()
            intervals.extend(zip(crossings[::2], crossings[1::2]))
        # Union the arms: even/odd across all paths would punch holes where
        # adjacent arms overlap, unlike eight filled SVG paths.
        merged = []
        for left, right in sorted(intervals):
            if merged and left <= merged[-1][1]:
                merged[-1] = (merged[-1][0], max(right, merged[-1][1]))
            else:
                merged.append((left, right))
        dest = coverage[row // 4]
        for left, right in merged:
            lo = max(0, math.ceil(left / unit - 0.5))
            hi = min(samples, math.ceil(right / unit - 0.5))
            for column in range(lo, hi):
                dest[column // 4] += 1
    return coverage


def tile_distance(x, y):
    """Signed distance (units) to the rounded tile; negative inside."""
    lo, hi = INSET + RADIUS, 1024.0 - INSET - RADIUS
    dx = max(lo - x, 0.0, x - hi)
    dy = max(lo - y, 0.0, y - hi)
    if dx > 0 and dy > 0:
        return math.hypot(dx, dy) - RADIUS
    inner = min(x - INSET, 1024.0 - INSET - x, y - INSET, 1024.0 - INSET - y)
    return max(dx, dy) - RADIUS if (dx or dy) else -inner


def render(size, style="desktop"):
    """RGBA rows from the SVG mark, with 4x4 supersampled edges."""
    unit = 1024.0 / size
    coverage = mark_coverage(size)
    rows = []
    offsets = [(i + 0.5) / 4.0 for i in range(4)]
    for j in range(size):
        row = bytearray()
        for i in range(size):
            petal = coverage[j][i] / 16.0
            if style == "foreground":
                row += bytes([*PETAL, round(255 * petal)])
                continue
            tile = 1.0
            if style == "desktop":
                dt = tile_distance((i+0.5)*unit, (j+0.5)*unit)
                if abs(dt) > 1.5 * unit:
                    tile = float(dt < 0)
                else:
                    tile = sum(tile_distance((i+ox)*unit, (j+oy)*unit) < 0
                               for oy in offsets for ox in offsets) / 16.0
            if tile == 0:
                row += b"\0\0\0\0"
            else:
                mix = min(petal / tile, 1.0)
                rgb = [round(t + (p-t)*mix) for t, p in zip(TILE, PETAL)]
                row += bytes(rgb + [round(255*tile)])
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
    """The same eight arms within the 66dp safe circle on a 108dp layer."""
    colour = "#" + "".join(f"{c:02x}" for c in PETAL)
    background = "#" + "".join(f"{c:02x}" for c in TILE)
    xmlns = 'xmlns:android="http://schemas.android.com/apk/res/android"'
    path = ARM_PATH
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
    arms = "".join(f'<path d="{ARM_PATH}" transform="rotate({45*k})"/>' for k in range(8))
    desktop_svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">'
                   '<rect x="100" y="100" width="824" height="824" rx="185" fill="#243f30"/>'
                   '<g fill="#d4edb8" transform="translate(512 512) scale(12.875)">'
                   + arms + '</g></svg>\n')
    output = {"desktop/packaging/icons/icon.svg": desktop_svg.encode()}
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

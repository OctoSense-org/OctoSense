#!/usr/bin/env python3
"""The desktop package's app icon, rendered from icons/icon.svg's geometry.

The mark is OctoSense's eight-petal flower on its green tile (the website's
favicon), inset on a 1024 canvas the way macOS app icons are. This writes the
PNG sizes cargo-packager turns into the macOS .icns, the Linux hicolor icons
and the Windows .ico, and the .ico itself (PNG entries):

    python3 desktop/packaging/make_icons.py          # rewrites icons/

Stdlib only (Python 3.9+), deterministic: the output is committed, so a
release build never renders anything. Edit TILE/PETAL/the geometry here and
in icon.svg together.
"""
import math
from pathlib import Path
import struct
import zlib

OUT = Path(__file__).resolve().parent / "icons"
SIZES = (32, 64, 128, 256, 512, 1024)  # the sizes an .icns takes
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)

# In icon.svg's 1024 user units.
TILE = (0x24, 0x3F, 0x30)
PETAL = (0xD4, 0xED, 0xB8)
INSET, RADIUS = 100.0, 185.0          # the tile: 824 wide, macOS-like corners
CENTER, SCALE = 512.0, 824.0 / 64.0   # the favicon's 64-unit flower, scaled to the tile
PETAL_RX, PETAL_RY, PETAL_CY = 5.0 * SCALE, 10.0 * SCALE, -15.0 * SCALE


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
    for k in range(8):
        a = -math.radians(45.0 * k)
        rx = px * math.cos(a) - py * math.sin(a)
        ry = px * math.sin(a) + py * math.cos(a)
        f = math.hypot(rx / PETAL_RX, (ry - PETAL_CY) / PETAL_RY)
        best = min(best, (f - 1.0) * PETAL_RX)
    return best


def render(size):
    """RGBA rows, 4x4 supersampled where an edge crosses the pixel."""
    unit = 1024.0 / size
    rows = []
    offsets = [(i + 0.5) / 4.0 for i in range(4)]
    for j in range(size):
        row = bytearray()
        for i in range(size):
            cx, cy = (i + 0.5) * unit, (j + 0.5) * unit
            near = 1.5 * unit
            dt, dp = tile_distance(cx, cy), petal_distance(cx, cy)
            if abs(dt) > near and abs(dp) > near:
                tile = 1.0 if dt < 0 else 0.0
                petal = 1.0 if dp < 0 and tile else 0.0
            else:
                tile = petal = 0.0
                for oy in offsets:
                    for ox in offsets:
                        sx, sy = (i + ox) * unit, (j + oy) * unit
                        if tile_distance(sx, sy) < 0:
                            tile += 1 / 16
                            if petal_distance(sx, sy) < 0:
                                petal += 1 / 16
            if tile == 0:
                row += b"\0\0\0\0"
                continue
            mix = petal / tile
            rgb = [round(t + (p - t) * mix) for t, p in zip(TILE, PETAL)]
            row += bytes(rgb + [round(255 * tile)])
        rows.append(bytes(row))
    return rows


def png(size, rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    raw = b"".join(b"\0" + r for r in rows)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
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


def main():
    OUT.mkdir(exist_ok=True)
    rendered = {}
    for size in sorted(set(SIZES) | set(ICO_SIZES)):
        rendered[size] = png(size, render(size))
    for size in SIZES:
        # An .icns takes 1024 only as 512 at 2x, which cargo-packager reads
        # from the `@2x` in the name.
        name = "icon_512@2x.png" if size == 1024 else f"icon_{size}.png"
        (OUT / name).write_bytes(rendered[size])
    (OUT / "icon.ico").write_bytes(ico([(s, rendered[s]) for s in ICO_SIZES]))
    print(f"wrote {len(SIZES)} PNGs and icon.ico to {OUT}")


if __name__ == "__main__":
    main()

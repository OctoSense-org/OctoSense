#!/usr/bin/env python3
"""Write web-installer/manifest.json for a release directory holding the
partition images. The images are served next to index.html (copy or symlink
the directory there). Usage: make-manifest.py <images dir> <name> [device]"""
import hashlib, json, os, sys, time
from pathlib import Path
src = Path(sys.argv[1]); name = sys.argv[2]; # The bootloader's `product` variable: the OnePlus 6 reports its chip, sdm845.
device = sys.argv[3] if len(sys.argv) > 3 else "sdm845"
ORDER = ["boot", "dtbo", "vbmeta", "vendor", "system"]
# Flashed without a slot suffix (the OnePlus 6 bootloader rejects vbmeta_b).
UNSLOTTED = {"vbmeta"}
images = []
for part in ORDER:
    f = src / f"{part}.img"
    if not f.exists(): continue
    h = hashlib.sha256(); size = 0
    with open(f, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""): h.update(chunk); size += len(chunk)
    entry = {"partition": part, "file": f.name, "size": size, "sha256": h.hexdigest()}
    if part in UNSLOTTED: entry["slot"] = False
    images.append(entry)
out = {"name": name, "device": device, "built": time.strftime("%Y-%m-%d %H:%M UTC", time.gmtime()), "images": images}
(src / "manifest.json").write_text(json.dumps(out, indent=2) + "\n")
print(json.dumps({k: v for k, v in out.items() if k != "images"}), len(images), "images")

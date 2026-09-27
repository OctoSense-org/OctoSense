#!/usr/bin/env python3
"""Package a complete local-development image set; does not operate a phone."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
import tempfile
from datetime import datetime, timezone

PARTITIONS = ("system", "vendor", "dtbo", "vbmeta", "boot")
MAX_IMAGE_BYTES = 16 * 1024 ** 3
MAX_RAW_BYTES = 64 * 1024 ** 2
SPARSE_MAGIC = 0xED26FF3A


def validate_sparse_chunks(image, size, block_size, blocks, chunks, name):
    offset, counted_blocks = 28, 0
    if chunks > (size - 28) // 12:
        raise ValueError(f"Invalid sparse chunk count: {name}")
    for _ in range(chunks):
        image.seek(offset)
        header = image.read(12)
        if len(header) != 12:
            raise ValueError(f"Truncated sparse chunk: {name}")
        kind, _, count, total_size = struct.unpack("<HHII", header)
        payload = count * block_size if kind == 0xCAC1 else 4 if kind in (0xCAC2, 0xCAC4) else 0 if kind == 0xCAC3 else None
        if (payload is None or total_size != 12 + payload or offset + total_size > size
                or (kind == 0xCAC4 and count != 0)):
            raise ValueError(f"Invalid sparse chunk: {name}")
        counted_blocks += count
        offset += total_size
        if counted_blocks > blocks:
            raise ValueError(f"Sparse block count overflow: {name}")
    if offset != size or counted_blocks != blocks:
        raise ValueError(f"Sparse length or block count mismatch: {name}")


def image_metadata(path, partition):
    if not path.is_file():
        raise ValueError(f"Required image missing: {path.name}")
    before = path.stat()
    if not 0 < before.st_size <= MAX_IMAGE_BYTES:
        raise ValueError(f"Invalid image size: {path.name}")
    digest = hashlib.sha256()
    with path.open("rb") as image:
        header = image.read(28)
        sparse = len(header) >= 4 and struct.unpack_from("<I", header)[0] == SPARSE_MAGIC
        expanded = before.st_size
        if sparse:
            if len(header) != 28:
                raise ValueError(f"Truncated sparse header: {path.name}")
            _, major, _, header_size, chunk_size, block_size, blocks, chunks, _ = struct.unpack("<IHHHHIIII", header)
            if (major != 1 or header_size != 28 or chunk_size != 12
                    or not block_size or block_size % 4 or not blocks or not chunks):
                raise ValueError(f"Invalid sparse header: {path.name}")
            expanded = block_size * blocks
            if expanded > MAX_IMAGE_BYTES:
                raise ValueError(f"Expanded image too large: {path.name}")
            validate_sparse_chunks(image, before.st_size, block_size, blocks, chunks, path.name)
            image.seek(28)
        elif before.st_size > MAX_RAW_BYTES:
            raise ValueError(f"Convert large raw image to sparse format first: {path.name}")
        digest.update(header)
        for chunk in iter(lambda: image.read(1024 * 1024), b""):
            digest.update(chunk)
    after = path.stat()
    if (before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_ino, after.st_size, after.st_mtime_ns):
        raise ValueError(f"Image changed during packaging: {path.name}")
    return {"partition": partition, "file": path.name, "size": before.st_size,
            "expanded_size": expanded, "sparse": sparse, "sha256": digest.hexdigest(),
            "slot": partition != "vbmeta"}


def build_manifest(source, name, device="enchilada"):
    if device != "enchilada":
        raise ValueError("Use device codename enchilada; sdm845 is not a unique phone identity")
    if not name or len(name) > 160 or any(ord(char) < 32 for char in name):
        raise ValueError("Invalid release name")
    images = [image_metadata(Path(source) / f"{partition}.img", partition) for partition in PARTITIONS]
    if sum(image["size"] for image in images) > 32 * 1024 ** 3:
        raise ValueError("Release exceeds the preview storage limit")
    return {"schema": 1, "name": name, "built": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "device": device, "recipe": "enchilada-fastboot-v1", "channel": "development", "images": images}


def write_manifest(source, name, device="enchilada"):
    source = Path(source)
    manifest = build_manifest(source, name, device)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", dir=source, prefix=".manifest-", delete=False) as output:
            temporary = Path(output.name)
            json.dump(manifest, output, indent=2)
            output.write("\n")
            output.flush()
            os.fsync(output.fileno())
        temporary.replace(source / "manifest.json")
    finally:
        if temporary and temporary.exists():
            temporary.unlink()
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("images", type=Path)
    parser.add_argument("name")
    parser.add_argument("device", nargs="?", default="enchilada", choices=["enchilada"])
    args = parser.parse_args()
    try:
        manifest = write_manifest(args.images, args.name, args.device)
    except (ValueError, OSError) as error:
        parser.exit(1, f"Cannot package release: {error}\n")
    print(json.dumps({"name": manifest["name"], "device": manifest["device"], "images": len(manifest["images"]), "channel": manifest["channel"]}))


if __name__ == "__main__":
    main()

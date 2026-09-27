#!/usr/bin/env python3
"""Split an Android sparse image into independent sparse parts of at most
--max-bytes of payload each. Every part covers the whole partition: DONT_CARE
before and after its data, so the parts can be flashed one by one, in any order,
and a failed part is retried alone (fastboot -S resends everything on a USB
error). Streams from disk; memory stays at one part.
Usage: split-sparse.py image.img out_dir [--max-bytes 67108864]"""
import struct, sys
from pathlib import Path

MAGIC, RAW, FILL, DONT_CARE, CRC = 0xED26FF3A, 0xCAC1, 0xCAC2, 0xCAC3, 0xCAC4

def index(f):
    """(block, blocks, type, file offset of data) for every RAW and FILL chunk."""
    magic, _, _, fhs, chs, blk, total_blks, total_chunks, _ = struct.unpack("<IHHHHIIII", f.read(28))
    assert magic == MAGIC and fhs == 28 and chs == 12, "not a sparse image"
    entries, block, pos = [], 0, 28
    for _ in range(total_chunks):
        f.seek(pos); typ, _, blocks, size = struct.unpack("<HHII", f.read(12))
        if typ in (RAW, FILL):
            entries.append((block, blocks, typ, pos + 12))
        if typ != CRC:
            block += blocks
        pos += size
    return blk, total_blks, entries

def main():
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    limit = int(sys.argv[sys.argv.index("--max-bytes") + 1]) if "--max-bytes" in sys.argv else 64 << 20
    f = open(src, "rb")
    blk, total, entries = index(f)
    out.mkdir(parents=True, exist_ok=True)
    # Cut the RAW runs so no part carries more than `limit` bytes of payload.
    pieces, size, part, parts = [], 0, [], []
    for block, blocks, typ, off in entries:
        if typ == FILL:
            part.append((block, blocks, FILL, off, 4)); continue
        done = 0
        while done < blocks:
            room = (limit - size) // blk
            if room <= 0:
                parts.append(part); part, size = [], 0; room = limit // blk
            take = min(room, blocks - done)
            part.append((block + done, take, RAW, off + done * blk, take * blk))
            size += take * blk; done += take
    if part: parts.append(part)
    for n, part in enumerate(parts):
        body, count, pos = [], 0, 0
        def dont_care(k):
            nonlocal count
            if k > 0: body.append(struct.pack("<HHII", DONT_CARE, 0, k, 12)); count += 1
        for block, blocks, typ, off, length in part:
            dont_care(block - pos)
            f.seek(off); body.append(struct.pack("<HHII", typ, 0, blocks, 12 + length) + f.read(length)); count += 1
            pos = block + blocks
        dont_care(total - pos)
        with open(out / ("%s.part%03d.img" % (src.stem, n)), "wb") as o:
            o.write(struct.pack("<IHHHHIIII", MAGIC, 1, 0, 28, 12, blk, total, count, 0))
            for b in body: o.write(b)
        body = None
    print("%d parts, %d blocks of %d bytes" % (len(parts), total, blk))

if __name__ == "__main__": main()

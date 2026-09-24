#!/usr/bin/env python3
"""Regenerates assets/disktree.ico (same design as src/icon.rs). Stdlib only."""
import struct, zlib, os

BLOCKS = [
    (24, 24, 140, 232, (229, 83, 83)),
    (148, 24, 232, 120, (77, 148, 235)),
    (148, 128, 232, 184, (92, 190, 108)),
    (148, 192, 188, 232, (232, 204, 72)),
    (196, 192, 232, 232, (178, 102, 224)),
]
BG = (28, 32, 42)

def rgba(size):
    radius = size * 0.18
    rows = []
    for y in range(size):
        row = bytearray()
        for x in range(size):
            fx, fy = x + 0.5, y + 0.5
            cx = min(max(fx, radius), size - radius)
            cy = min(max(fy, radius), size - radius)
            d = ((fx - cx) ** 2 + (fy - cy) ** 2) ** 0.5
            a = min(max(radius - d + 0.5, 0.0), 1.0)
            gx, gy = fx * 256 / size, fy * 256 / size
            col = next((c for (x0, y0, x1, y1, c) in BLOCKS if x0 <= gx < x1 and y0 <= gy < y1), BG)
            row += bytes(col) + bytes([int(a * 255)])
        rows.append(bytes(row))
    return rows

def png(size):
    raw = b"".join(b"\0" + r for r in rgba(size))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))

sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
images = [png(s) for s in sizes]
header = struct.pack("<HHH", 0, 1, len(sizes))
offset = 6 + 16 * len(sizes)
entries = b""
for s, img in zip(sizes, images):
    entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(img), offset)
    offset += len(img)
out = os.path.join(os.path.dirname(__file__), "..", "assets", "disktree.ico")
with open(out, "wb") as f:
    f.write(header + entries + b"".join(images))
print("wrote", os.path.normpath(out))

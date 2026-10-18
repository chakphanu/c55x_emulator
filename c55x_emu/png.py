"""Minimal grayscale PNG writer (stdlib only)."""

import struct
import zlib


def _chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def write_png(path, rows, scale=1):
    """rows: lists of 0/1 pixels (1 = dark)."""
    lines = []
    for row in rows:
        line = bytes(0x20 if p else 0xC8 for p in row for _ in range(scale))
        lines.extend([b"\0" + line] * scale)
    height, width = len(lines), len(lines[0]) - 1
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + _chunk(b"IHDR", ihdr)
                + _chunk(b"IDAT", zlib.compress(b"".join(lines))) + _chunk(b"IEND", b""))

#!/usr/bin/env python3
"""Encode raw offline Metal QA renders into PNG, without modifying pixels."""
import argparse
from pathlib import Path
import struct
import zlib


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


parser = argparse.ArgumentParser()
parser.add_argument("directory", type=Path)
args = parser.parse_args()
for path in sorted(args.directory.glob("*.rgba")):
    pixels = path.read_bytes()
    width, height = 1280, 840
    if len(pixels) != width * height * 4:
        raise ValueError(f"Wrong render size: {path}")
    rows = b"".join(b"\0" + pixels[y * width * 4:(y + 1) * width * 4] for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    path.with_suffix(".png").write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    print(path.with_suffix(".png"))

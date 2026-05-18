#!/usr/bin/env python3
"""Generate placeholder PNG icons for Tauri using stdlib only."""
import os
import zlib
from struct import pack


def make_png(size: int, path: str) -> None:
    raw = bytearray()
    for y in range(size):
        raw.append(0)  # PNG filter byte per row
        for x in range(size):
            cx, cy = size / 2, size / 2
            dx, dy = x - cx, y - cy
            d = (dx * dx + dy * dy) ** 0.5
            edge = max(0.0, 1.0 - max(0.0, d - size * 0.42) / (size * 0.05))
            t = (x + y) / (2 * size)
            r = int(70 + t * 120)
            g = int(80 + t * 40)
            b = int(180 + t * 40)
            a = int(255 * edge)
            raw.extend([r, g, b, a])
    compressed = zlib.compress(bytes(raw), 9)

    def chunk(tag: bytes, data: bytes) -> bytes:
        c = tag + data
        crc = zlib.crc32(c) & 0xFFFFFFFF
        return pack(">I", len(data)) + c + pack(">I", crc)

    sig = b"\x89PNG\r\n\x1a\n"
    ihdr = pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    with open(path, "wb") as f:
        f.write(sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", compressed) + chunk(b"IEND", b""))


def main() -> None:
    base = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src-tauri", "icons")
    os.makedirs(base, exist_ok=True)
    sizes = [
        (32, "32x32.png"),
        (128, "128x128.png"),
        (256, "128x128@2x.png"),
        (512, "icon.png"),
    ]
    for size, name in sizes:
        path = os.path.join(base, name)
        make_png(size, path)
        print(f"wrote {path}")


if __name__ == "__main__":
    main()

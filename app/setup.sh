#!/usr/bin/env bash
# Setup C2-Tracker Tauri app: generate placeholder icon, install tauri-cli if needed, run dev.
set -euo pipefail

cd "$(dirname "$0")"

ICON_DIR="src-tauri/icons"
mkdir -p "$ICON_DIR"

# Generate a 512x512 PNG via Python (stdlib only) if icon.png missing.
if [[ ! -f "$ICON_DIR/icon.png" ]]; then
  echo "→ Generating placeholder icon..."
  python3 - <<'PY'
import zlib
from struct import pack

def make_png(size, path):
    raw = bytearray()
    for y in range(size):
        raw.append(0)  # PNG row filter
        for x in range(size):
            # Diagonal indigo→violet gradient with rounded inset.
            cx, cy = size/2, size/2
            dx, dy = x - cx, y - cy
            d = (dx*dx + dy*dy) ** 0.5
            edge = max(0.0, 1.0 - max(0, d - size*0.42) / (size*0.05))
            t = (x + y) / (2*size)
            r = int(70 + t*120)
            g = int(80 + t*40)
            b = int(180 + t*40)
            a = int(255 * edge)
            raw.extend([r, g, b, a])
    compressed = zlib.compress(bytes(raw), 9)
    def chunk(tag, data):
        c = tag + data
        crc = zlib.crc32(c) & 0xffffffff
        return pack('>I', len(data)) + c + pack('>I', crc)
    sig = b'\x89PNG\r\n\x1a\n'
    ihdr = pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0)
    with open(path, 'wb') as f:
        f.write(sig + chunk(b'IHDR', ihdr) + chunk(b'IDAT', compressed) + chunk(b'IEND', b''))

import os
base = 'src-tauri/icons'
os.makedirs(base, exist_ok=True)
for sz, name in [(32,'32x32.png'),(128,'128x128.png'),(256,'128x128@2x.png'),(512,'icon.png')]:
    make_png(sz, f'{base}/{name}')
print('icons generated')
PY
fi

# Ensure tauri-cli is installed.
if ! command -v cargo-tauri >/dev/null 2>&1; then
  echo "→ Installing cargo-tauri (this may take a few minutes)..."
  cargo install tauri-cli --version "^2.0" --locked
fi

echo "→ Starting cargo tauri dev..."
exec cargo tauri dev

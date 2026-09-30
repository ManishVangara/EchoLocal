#!/usr/bin/env python3
"""Generate EchoLocal's placeholder app and menu-bar icons (no dependencies).

The mark is five rounded "sound bars". Replace the output with real artwork
any time; for a full platform icon set run `bun tauri icon <1024px.png>`.
"""

import math
import os
import struct
import zlib

ROOT = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
SS = 4  # supersampling factor for anti-aliasing

BAR_HEIGHTS = [0.34, 0.62, 0.9, 0.62, 0.34]


def write_png(path, width, height, pixels):
    """pixels: list of rows, each a list of (r, g, b, a) tuples."""
    raw = b"".join(b"\x00" + bytes(c for px in row for c in px) for row in pixels)

    def chunk(tag, data):
        body = tag + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def rounded_rect(x, y, cx, cy, w, h, r):
    """Signed-ish inside test for a rounded rectangle centred at (cx, cy)."""
    dx = max(abs(x - cx) - (w / 2 - r), 0)
    dy = max(abs(y - cy) - (h / 2 - r), 0)
    return dx * dx + dy * dy <= r * r


def render(size, shade):
    """shade(x, y) -> (r, g, b, a) in unit coordinates, supersampled."""
    rows = []
    for py in range(size):
        row = []
        for px in range(size):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SS):
                for sx in range(SS):
                    x = (px + (sx + 0.5) / SS) / size
                    y = (py + (sy + 0.5) / SS) / size
                    r, g, b, a = shade(x, y)
                    acc[0] += r * a
                    acc[1] += g * a
                    acc[2] += b * a
                    acc[3] += a
            n = SS * SS
            alpha = acc[3] / n
            if alpha > 0:
                color = [acc[i] / acc[3] for i in range(3)]
            else:
                color = [0, 0, 0]
            row.append(tuple(int(round(c)) for c in color) + (int(round(alpha * 255)),))
        rows.append(row)
    return rows


def in_bars(x, y, span=0.56, height=0.56, cy=0.5):
    n = len(BAR_HEIGHTS)
    pitch = span / n
    width = pitch * 0.58
    left = 0.5 - span / 2 + pitch / 2
    for i, h in enumerate(BAR_HEIGHTS):
        cx = left + i * pitch
        bh = height * h
        if rounded_rect(x, y, cx, cy, width, bh, width / 2):
            return True
    return False


def app_icon(x, y):
    # macOS-style squircle-ish rounded square with a vertical gradient.
    if not rounded_rect(x, y, 0.5, 0.5, 0.82, 0.82, 0.19):
        return (0, 0, 0, 0)
    if in_bars(x, y):
        return (255, 255, 255, 1)
    t = y
    top, bottom = (45, 212, 191), (15, 118, 110)
    return tuple(top[i] + (bottom[i] - top[i]) * t for i in range(3)) + (1,)


def tray_idle(x, y):
    return (0, 0, 0, 1) if in_bars(x, y, span=0.9, height=0.9) else (0, 0, 0, 0)


def tray_recording(x, y):
    # Solid dot with the bars cut out: reads as "live" in the menu bar.
    inside = (x - 0.5) ** 2 + (y - 0.5) ** 2 <= 0.47 ** 2
    if inside and not in_bars(x, y, span=0.56, height=0.62):
        return (0, 0, 0, 1)
    return (0, 0, 0, 0)


def main():
    os.makedirs(ROOT, exist_ok=True)
    for name, size in [("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256), ("icon.png", 512)]:
        write_png(os.path.join(ROOT, name), size, size, render(size, app_icon))
    write_png(os.path.join(ROOT, "tray.png"), 36, 36, render(36, tray_idle))
    write_png(os.path.join(ROOT, "tray-recording.png"), 36, 36, render(36, tray_recording))
    print("icons written to", os.path.normpath(ROOT))


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Generates every bundle icon from the brand mark (§5.10).

The mark is the drift rail as a monogram: two vertical strokes, one dashed and
one solid, the solid one running slightly longer. It is legible at 16px — which
is the size that matters most, because that is the tray icon showing a running
timer — it works in monochrome, and it is literally the product's idea.

    python3 scripts/gen-icons.py

Writes PNG (all platforms), ICO (Windows — `tauri-build` requires
`icons/icon.ico` to generate the Windows resource file) and ICNS (macOS).
No image library: PNG, ICO and ICNS are all written by hand, so this runs
anywhere Python does and adds no build dependency.
"""

import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"

# §5.2 tokens. The icon sits on `--ink` rather than transparency: the two
# strokes are mid-tone, and on a white taskbar a transparent mark disappears.
INK = (0x0E, 0x11, 0x16, 255)
PLOT = (0x56, 0xC2, 0xD6, 255)
TRACK = (0xE9, 0xA6, 0x3C, 255)
CLEAR = (0, 0, 0, 0)


def render(size: int) -> list[list[tuple[int, int, int, int]]]:
    """The mark on a rounded `ink` plate, drawn on a 20-unit grid."""
    u = size / 20.0
    radius = size * 0.18
    px = [[CLEAR for _ in range(size)] for _ in range(size)]

    for y in range(size):
        for x in range(size):
            # Rounded corners: only the four corner quadrants get a distance test.
            cx = radius if x < radius else (size - radius - 1 if x > size - radius - 1 else x)
            cy = radius if y < radius else (size - radius - 1 if y > size - radius - 1 else y)
            if (x - cx) ** 2 + (y - cy) ** 2 <= radius * radius:
                px[y][x] = INK

    def rect(x0: float, y0: float, x1: float, y1: float, colour) -> None:
        for y in range(max(0, int(y0 * u)), min(size, int(round(y1 * u)))):
            for x in range(max(0, int(x0 * u)), min(size, int(round(x1 * u)))):
                px[y][x] = colour

    # Plot trace: dashed, 3 on / 3 off, y 4→15. What you intended.
    y = 4.0
    while y < 15:
        rect(6.5, y, 8.0, min(y + 1.7, 15), PLOT)
        y += 3.0
    # Track trace: solid, running longer, y 4→17. What you actually spent.
    rect(11.5, 4, 14.0, 17, TRACK)
    return px


# ─── PNG ────────────────────────────────────────────────────────────────

def png_bytes(px) -> bytes:
    size = len(px)
    raw = b"".join(b"\x00" + b"".join(bytes(p) for p in row) for row in px)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


# ─── ICO ────────────────────────────────────────────────────────────────

def dib_bytes(px) -> bytes:
    """
    A 32bpp BMP/DIB image for an ICO entry.

    DIB rather than PNG-in-ICO: Windows only accepts PNG entries from Vista on
    and only at 256px, whereas every tool everywhere reads a DIB. `biHeight` is
    doubled because an icon DIB carries an XOR image *and* an AND mask, and the
    rows run bottom-up.
    """
    size = len(px)
    xor = bytearray()
    for row in reversed(px):
        for r, g, b, a in row:
            xor += bytes((b, g, r, a))          # BGRA, not RGBA

    mask_row_bytes = ((size + 31) // 32) * 4    # 1bpp, padded to 4 bytes
    and_mask = bytearray()
    for row in reversed(px):
        bits = bytearray(mask_row_bytes)
        for x, (_, _, _, a) in enumerate(row):
            if a == 0:                          # 1 = transparent
                bits[x // 8] |= 0x80 >> (x % 8)
        and_mask += bits

    header = struct.pack(
        "<IiiHHIIiiII",
        40,             # biSize
        size,           # biWidth
        size * 2,       # biHeight — XOR image plus AND mask
        1,              # biPlanes
        32,             # biBitCount
        0,              # biCompression = BI_RGB
        len(xor) + len(and_mask),
        0, 0, 0, 0,
    )
    return header + bytes(xor) + bytes(and_mask)


def ico_bytes(sizes) -> bytes:
    # DIB up to 128px, PNG at 256px. An uncompressed 256×256 DIB is ~350KB of
    # the installer budget (§7.1) on its own, and every Windows since Vista
    # reads a PNG entry at that size.
    images = [
        png_bytes(render(s)) if s >= 256 else dib_bytes(render(s))
        for s in sizes
    ]
    offset = 6 + 16 * len(sizes)
    out = struct.pack("<HHH", 0, 1, len(sizes))   # reserved, type=icon, count
    for size, data in zip(sizes, images):
        out += struct.pack(
            "<BBBBHHII",
            size if size < 256 else 0,            # 0 means 256
            size if size < 256 else 0,
            0, 0, 1, 32,
            len(data),
            offset,
        )
        offset += len(data)
    return out + b"".join(images)


# ─── ICNS ───────────────────────────────────────────────────────────────

def icns_bytes() -> bytes:
    """macOS accepts PNG payloads for the modern icon types."""
    entries = [(b"ic07", 128), (b"ic08", 256), (b"ic09", 512), (b"ic13", 256), (b"ic14", 512)]
    body = b""
    for kind, size in entries:
        data = png_bytes(render(size))
        body += kind + struct.pack(">I", len(data) + 8) + data
    return b"icns" + struct.pack(">I", len(body) + 8) + body


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)

    for size, name in [
        (32, "32x32.png"),
        (128, "128x128.png"),
        (256, "128x128@2x.png"),
        (512, "icon.png"),
    ]:
        (OUT / name).write_bytes(png_bytes(render(size)))

    # tauri-build refuses to generate the Windows resource file without this one.
    (OUT / "icon.ico").write_bytes(ico_bytes([16, 32, 48, 64, 128, 256]))
    (OUT / "icon.icns").write_bytes(icns_bytes())

    for f in sorted(OUT.iterdir()):
        print(f"  {f.name:20} {f.stat().st_size:>8,} bytes")


if __name__ == "__main__":
    main()

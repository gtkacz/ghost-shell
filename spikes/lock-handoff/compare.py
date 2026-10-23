"""Compares the gate's last frame with the greeting overlay's resting frame, both PNGs from grab.py shot.

usage: uv run --no-project python -I compare.py <gate.png> <overlay.png> <scanline period px> <scanline width px>

Every pixel that differs must be the gate's scanline darkening the overlay's pixel: on a row of one residue class
group (width rows in every period), with each channel the overlay's times one common factor below 1. Prints the
counts and exits 1 if any other pixel differs.
"""

import struct
import sys
import zlib

EXIT_USAGE = 64
PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
BYTES_PER_PIXEL = 4
# QSG rounds a blended channel to a whole 8-bit value, so a darkened channel lands within this of the exact product.
ROUNDING = 1.5
# Channels this dark carry too little signal to read the darkening factor from.
MIN_CHANNEL_FOR_FACTOR = 8


def read_png(path: str) -> tuple[int, int, bytes]:
    """Reads grab.py's own PNGs: 8-bit RGBA, unfiltered rows, one IDAT."""
    data = open(path, "rb").read()
    if not data.startswith(PNG_SIGNATURE):
        raise ValueError(f"{path} is not a PNG")
    at = len(PNG_SIGNATURE)
    width = height = 0
    idat = b""
    while at < len(data):
        (length,) = struct.unpack(">I", data[at : at + 4])
        kind = data[at + 4 : at + 8]
        body = data[at + 8 : at + 8 + length]
        if kind == b"IHDR":
            width, height = struct.unpack(">II", body[:8])
        elif kind == b"IDAT":
            idat += body
        at += 12 + length
    raw = zlib.decompress(idat)
    row_bytes = width * BYTES_PER_PIXEL
    rows = []
    for y in range(height):
        start = y * (row_bytes + 1)
        if raw[start] != 0:
            raise ValueError(f"{path}: row {y} uses PNG filter {raw[start]}, not grab.py's 0")
        rows.append(raw[start + 1 : start + 1 + row_bytes])
    return width, height, b"".join(rows)


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        print(__doc__.strip().splitlines()[-4], file=sys.stderr)
        return EXIT_USAGE
    gate_w, gate_h, gate = read_png(argv[0])
    over_w, over_h, overlay = read_png(argv[1])
    period, line_width = int(argv[2]), int(argv[3])
    if (gate_w, gate_h) != (over_w, over_h):
        print(f"sizes differ: gate {gate_w}x{gate_h}, overlay {over_w}x{over_h}")
        return 1
    row_bytes = gate_w * BYTES_PER_PIXEL
    differing_rows: dict[int, int] = {}
    factors: list[float] = []
    not_darkening = 0
    differing = 0
    for y in range(gate_h):
        row = slice(y * row_bytes, (y + 1) * row_bytes)
        g_row, o_row = gate[row], overlay[row]
        if g_row == o_row:
            continue
        for x in range(0, row_bytes, BYTES_PER_PIXEL):
            g = g_row[x : x + 3]
            o = o_row[x : x + 3]
            if g == o:
                continue
            differing += 1
            differing_rows[y] = differing_rows.get(y, 0) + 1
            bright = max(range(3), key=lambda c: o[c])
            if o[bright] < MIN_CHANNEL_FOR_FACTOR or g[bright] > o[bright]:
                not_darkening += 1
                continue
            factor = g[bright] / o[bright]
            if any(abs(g[c] - o[c] * factor) > ROUNDING for c in range(3)):
                not_darkening += 1
                continue
            factors.append(factor)
    residues = sorted({y % period for y in differing_rows})
    print(f"pixels differing: {differing} of {gate_w * gate_h}, on {len(differing_rows)} rows")
    print(f"row residues mod {period}: {residues} (the scanline covers {line_width} of {period})")
    if factors:
        print(f"darkening factor: min {min(factors):.3f} max {max(factors):.3f}")
    print(f"differing pixels that are not a darkening of the overlay's: {not_darkening}")
    scanline_rows = len(residues) <= line_width and (
        not residues or (residues[-1] - residues[0]) % period < line_width
    )
    if not_darkening or not scanline_rows:
        print("FAIL: a pixel differs outside the scanline pattern")
        return 1
    print("PASS: the frames differ only by the scanline pattern")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

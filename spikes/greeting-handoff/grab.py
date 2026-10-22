"""Captures the headless KWin's scene through org.kde.KWin.ScreenShot2 on its private session bus.

usage:
    uv run --no-project --with dbus-next python -I grab.py shot <bus address> <out.png>
    uv run --no-project --with dbus-next python -I grab.py watch <bus address> <out.tsv> <seconds> <x> <y> <w> <h>

`shot` saves the whole workspace. `watch` captures one area as fast as KWin answers and writes, per capture,
its epoch milliseconds, mean red, green and blue and a class (red, ground, black or other); the first
capture of each run of one class is also saved next to the table as a PNG.
"""

import asyncio
import os
import struct
import sys
import time
import zlib

from dbus_next import Message, MessageType, Variant
from dbus_next.aio import MessageBus

EXIT_USAGE = 64
SERVICE = "org.kde.KWin"
PATH = "/org/kde/KWin/ScreenShot2"
INTERFACE = "org.kde.KWin.ScreenShot2"
READ_BYTES = 1 << 20
# QImage::Format values KWin hands out: 32-bit formats stored B, G, R, A in memory, and R, G, B, A.
BGRA_FORMATS = {4, 5, 6}
RGBA_FORMATS = {16, 17, 18}
BYTES_PER_PIXEL = 4
# The spike's colours: the overlay's ground #0a0f11 and the stand-in window's #ff0000, with room for
# rounding in KWin's blending.
GROUND = (10, 15, 17)
COLOUR_TOLERANCE = 6
RED_MIN = 200
RED_OTHER_MAX = 40


def png(width: int, height: int, rgba: bytes) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    row_bytes = width * BYTES_PER_PIXEL
    rows = b"".join(b"\x00" + rgba[y * row_bytes : (y + 1) * row_bytes] for y in range(height))
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows, 1)) + chunk(b"IEND", b"")


def to_rgba(raw: bytes, width: int, height: int, stride: int, image_format: int) -> bytes:
    row_bytes = width * BYTES_PER_PIXEL
    packed = bytearray(b"".join(raw[y * stride : y * stride + row_bytes] for y in range(height)))
    if image_format in RGBA_FORMATS:
        return bytes(packed)
    if image_format not in BGRA_FORMATS:
        raise ValueError(f"unsupported QImage format {image_format}")
    rgba = bytearray(packed)
    rgba[0::4] = packed[2::4]
    rgba[2::4] = packed[0::4]
    return bytes(rgba)


async def capture(bus: MessageBus, member: str, signature: str, args: list) -> tuple[int, int, bytes]:
    read_end, write_end = os.pipe()
    try:
        reply = await bus.call(
            Message(
                destination=SERVICE,
                path=PATH,
                interface=INTERFACE,
                member=member,
                signature=signature + "a{sv}h",
                body=[*args, {"native-resolution": Variant("b", True)}, 0],
                unix_fds=[write_end],
            )
        )
    finally:
        os.close(write_end)
    try:
        if reply.message_type == MessageType.ERROR:
            raise RuntimeError(f"{member} failed: {reply.error_name} {reply.body}")
        attributes = {key: value.value for key, value in reply.body[0].items()}
        chunks = []
        while chunk := await asyncio.to_thread(os.read, read_end, READ_BYTES):
            chunks.append(chunk)
    finally:
        os.close(read_end)
    width, height = attributes["width"], attributes["height"]
    rgba = to_rgba(b"".join(chunks), width, height, attributes["stride"], attributes["format"])
    return width, height, rgba


def classify(mean: tuple[float, float, float]) -> str:
    r, g, b = mean
    if r >= RED_MIN and g <= RED_OTHER_MAX and b <= RED_OTHER_MAX:
        return "red"
    if all(abs(value - target) <= COLOUR_TOLERANCE for value, target in zip(mean, GROUND)):
        return "ground"
    if max(mean) <= COLOUR_TOLERANCE:
        return "black"
    return "other"


async def shot(bus: MessageBus, out: str) -> None:
    width, height, rgba = await capture(bus, "CaptureWorkspace", "", [])
    with open(out, "wb") as file:
        file.write(png(width, height, rgba))
    print(f"grab saved {out} {width}x{height} at {time.time_ns() // 1_000_000}", flush=True)


async def watch(bus: MessageBus, out: str, seconds: float, area: list[int]) -> None:
    deadline = time.monotonic() + seconds
    previous = None
    stem = out.removesuffix(".tsv")
    with open(out, "w") as table:
        table.write("epoch_ms\tr\tg\tb\tclass\n")
        while time.monotonic() < deadline:
            width, height, rgba = await capture(bus, "CaptureArea", "iiuu", area)
            at = time.time_ns() // 1_000_000
            pixels = width * height
            mean = tuple(sum(rgba[channel::4]) / pixels for channel in range(3))
            kind = classify(mean)
            table.write(f"{at}\t{mean[0]:.1f}\t{mean[1]:.1f}\t{mean[2]:.1f}\t{kind}\n")
            if kind != previous:
                with open(f"{stem}-{at}-{kind}.png", "wb") as file:
                    file.write(png(width, height, rgba))
                previous = kind
    print(f"grab watch done {out}", flush=True)


async def run(argv: list[str]) -> int:
    if len(argv) == 3 and argv[0] == "shot":
        bus = await MessageBus(bus_address=argv[1], negotiate_unix_fd=True).connect()
        await shot(bus, argv[2])
        return 0
    if len(argv) == 8 and argv[0] == "watch":
        bus = await MessageBus(bus_address=argv[1], negotiate_unix_fd=True).connect()
        await watch(bus, argv[2], float(argv[3]), [int(value) for value in argv[4:]])
        return 0
    print("\n".join(__doc__.strip().splitlines()[2:4]), file=sys.stderr)
    return EXIT_USAGE


if __name__ == "__main__":
    sys.exit(asyncio.run(run(sys.argv[1:])))

"""Gives the headless KWin a keyboard through its org_kde_kwin_fake_input protocol and types one key per
line read from stdin, so the spike can see which surface holds the keyboard. KWin offers the protocol to
any client only with KWIN_WAYLAND_NO_PERMISSION_CHECKS=1. It speaks the Wayland wire format directly,
so it needs no Wayland bindings.

usage: uv run --no-project python -I fake_key.py <wayland socket path> < commands
    each line is an evdev key code to press and release; the device lives until stdin closes.
"""

import socket
import struct
import sys
import time

EXIT_USAGE = 64
DISPLAY_ID = 1
REGISTRY_ID = 2
DISPLAY_SYNC = 0
DISPLAY_GET_REGISTRY = 1
DISPLAY_ERROR_EVENT = 0
REGISTRY_BIND = 0
REGISTRY_GLOBAL_EVENT = 0
CALLBACK_DONE_EVENT = 0
FAKE_INPUT_INTERFACE = "org_kde_kwin_fake_input"
FAKE_INPUT_AUTHENTICATE = 0
FAKE_INPUT_KEYBOARD_KEY = 10
# keyboard_key arrived in version 4.
FAKE_INPUT_MIN_VERSION = 4
KEY_RELEASED = 0
KEY_PRESSED = 1
HEADER = struct.Struct("<IHH")
READ_BYTES = 65536


def encode_string(text: str) -> bytes:
    data = text.encode() + b"\x00"
    padding = -len(data) % 4
    return struct.pack("<I", len(data)) + data + b"\x00" * padding


def decode_string(payload: bytes, offset: int) -> tuple[str, int]:
    (length,) = struct.unpack_from("<I", payload, offset)
    start = offset + 4
    text = payload[start : start + length - 1].decode()
    return text, start + length + (-length % 4)


class Client:
    def __init__(self, path: str) -> None:
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.connect(path)
        self.pending = b""
        # The compositor wants client object ids in creation order, with no gaps.
        self.next_id = REGISTRY_ID + 1
        self.globals: dict[str, tuple[int, int]] = {}

    def send(self, object_id: int, opcode: int, payload: bytes) -> None:
        self.sock.sendall(HEADER.pack(object_id, opcode, HEADER.size + len(payload)) + payload)

    def new_id(self) -> int:
        object_id = self.next_id
        self.next_id += 1
        return object_id

    def roundtrip(self) -> None:
        callback = self.new_id()
        self.send(DISPLAY_ID, DISPLAY_SYNC, struct.pack("<I", callback))
        while True:
            object_id, opcode, payload = self.read_event()
            if object_id == callback and opcode == CALLBACK_DONE_EVENT:
                return
            if object_id == DISPLAY_ID and opcode == DISPLAY_ERROR_EVENT:
                failed_id, code = struct.unpack_from("<II", payload)
                message, _ = decode_string(payload, 8)
                raise RuntimeError(f"wayland error on object {failed_id}, code {code}: {message}")
            if object_id == REGISTRY_ID and opcode == REGISTRY_GLOBAL_EVENT:
                (name,) = struct.unpack_from("<I", payload)
                interface, offset = decode_string(payload, 4)
                (version,) = struct.unpack_from("<I", payload, offset)
                self.globals[interface] = (name, version)

    def read_event(self) -> tuple[int, int, bytes]:
        while len(self.pending) < HEADER.size or len(self.pending) < HEADER.unpack_from(self.pending)[2]:
            chunk = self.sock.recv(READ_BYTES)
            if not chunk:
                raise RuntimeError("the compositor closed the connection")
            self.pending += chunk
        object_id, opcode, size = HEADER.unpack_from(self.pending)
        payload = self.pending[HEADER.size : size]
        self.pending = self.pending[size:]
        return object_id, opcode, payload


def log(text: str) -> None:
    print(f"fake-key {text} at {time.time_ns() // 1_000_000}", flush=True)


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__.strip().splitlines()[-2], file=sys.stderr)
        return EXIT_USAGE
    client = Client(argv[0])
    client.send(DISPLAY_ID, DISPLAY_GET_REGISTRY, struct.pack("<I", REGISTRY_ID))
    client.roundtrip()
    if FAKE_INPUT_INTERFACE not in client.globals:
        print(f"{FAKE_INPUT_INTERFACE} is not offered", file=sys.stderr)
        return 1
    name, version = client.globals[FAKE_INPUT_INTERFACE]
    if version < FAKE_INPUT_MIN_VERSION:
        print(f"{FAKE_INPUT_INTERFACE} is version {version}, below {FAKE_INPUT_MIN_VERSION}", file=sys.stderr)
        return 1
    fake_input = client.new_id()
    bind = struct.pack("<I", name) + encode_string(FAKE_INPUT_INTERFACE)
    client.send(REGISTRY_ID, REGISTRY_BIND, bind + struct.pack("<II", FAKE_INPUT_MIN_VERSION, fake_input))
    client.send(fake_input, FAKE_INPUT_AUTHENTICATE, encode_string("greeting-handoff spike") + encode_string("spike"))
    client.roundtrip()
    log("ready")
    for line in sys.stdin:
        key = int(line)
        for state in (KEY_PRESSED, KEY_RELEASED):
            client.send(fake_input, FAKE_INPUT_KEYBOARD_KEY, struct.pack("<II", key, state))
        client.roundtrip()
        log(f"typed {key}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

"""Pointer and keyboard through KWin's org_kde_kwin_fake_input, one command per line on stdin: `click X Y`,
`move X Y`, `key CODE`. Reuses the wire client of scripts/fake-keys.py, which can only type.

usage: uv run --no-project python -I fake_input.py <wayland socket path> < commands
"""

import importlib.util
import pathlib
import struct
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("fake_keys", REPO / "scripts" / "fake-keys.py")
fk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fk)

FAKE_INPUT_BUTTON = 2
FAKE_INPUT_POINTER_MOTION_ABSOLUTE = 9
BTN_LEFT = 0x110
FIXED_ONE = 256


def main(path: str) -> int:
    client = fk.Client(path)
    client.send(fk.DISPLAY_ID, fk.DISPLAY_GET_REGISTRY, struct.pack("<I", fk.REGISTRY_ID))
    client.roundtrip()
    name, _ = client.globals[fk.FAKE_INPUT_INTERFACE]
    fake_input = client.new_id()
    bind = struct.pack("<I", name) + fk.encode_string(fk.FAKE_INPUT_INTERFACE)
    client.send(fk.REGISTRY_ID, fk.REGISTRY_BIND, bind + struct.pack("<II", fk.FAKE_INPUT_MIN_VERSION, fake_input))
    client.send(fake_input, fk.FAKE_INPUT_AUTHENTICATE, fk.encode_string("desktop-focus spike") + fk.encode_string("spike"))
    client.roundtrip()
    fk.log("ready")
    for line in sys.stdin:
        verb, *args = line.split()
        if verb in ("click", "move"):
            x, y = (int(a) * FIXED_ONE for a in args)
            client.send(fake_input, FAKE_INPUT_POINTER_MOTION_ABSOLUTE, struct.pack("<ii", x, y))
            client.roundtrip()
            if verb == "click":
                for state in (fk.KEY_PRESSED, fk.KEY_RELEASED):
                    client.send(fake_input, FAKE_INPUT_BUTTON, struct.pack("<II", BTN_LEFT, state))
        elif verb == "key":
            for state in (fk.KEY_PRESSED, fk.KEY_RELEASED):
                client.send(fake_input, fk.FAKE_INPUT_KEYBOARD_KEY, struct.pack("<II", int(args[0]), state))
        else:
            raise SystemExit(f"unknown command {line!r}")
        client.roundtrip()
        fk.log(line.strip())
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))

"""Types key chords into the headless KWin through the greeting-handoff spike's fake keyboard
(spikes/greeting-handoff/fake_key.py), which only taps one key at a time. Each stdin line is one or more
evdev key codes: they are pressed in order and released in reverse, so "125" taps Meta alone and
"125 31" is Meta+S.

usage: uv run --no-project python -I -B fake_chord.py <wayland socket path> < chords
"""

import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "greeting-handoff"))

import fake_key  # noqa: E402

EXIT_USAGE = 64


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return EXIT_USAGE
    client = fake_key.Client(argv[0])
    client.send(fake_key.DISPLAY_ID, fake_key.DISPLAY_GET_REGISTRY, struct.pack("<I", fake_key.REGISTRY_ID))
    client.roundtrip()
    if fake_key.FAKE_INPUT_INTERFACE not in client.globals:
        print(f"{fake_key.FAKE_INPUT_INTERFACE} is not offered", file=sys.stderr)
        return 1
    name, version = client.globals[fake_key.FAKE_INPUT_INTERFACE]
    if version < fake_key.FAKE_INPUT_MIN_VERSION:
        print(f"{fake_key.FAKE_INPUT_INTERFACE} is version {version}", file=sys.stderr)
        return 1
    fake_input = client.new_id()
    bind = struct.pack("<I", name) + fake_key.encode_string(fake_key.FAKE_INPUT_INTERFACE)
    client.send(
        fake_key.REGISTRY_ID,
        fake_key.REGISTRY_BIND,
        bind + struct.pack("<II", fake_key.FAKE_INPUT_MIN_VERSION, fake_input),
    )
    client.send(
        fake_input,
        fake_key.FAKE_INPUT_AUTHENTICATE,
        fake_key.encode_string("bar-bridge spike") + fake_key.encode_string("spike"),
    )
    client.roundtrip()
    fake_key.log("ready")
    for line in sys.stdin:
        keys = [int(code) for code in line.split()]
        if not keys:
            continue
        for key in keys:
            client.send(fake_input, fake_key.FAKE_INPUT_KEYBOARD_KEY, struct.pack("<II", key, fake_key.KEY_PRESSED))
            client.roundtrip()
        for key in reversed(keys):
            client.send(fake_input, fake_key.FAKE_INPUT_KEYBOARD_KEY, struct.pack("<II", key, fake_key.KEY_RELEASED))
            client.roundtrip()
        fake_key.log(f"typed {'+'.join(map(str, keys))}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

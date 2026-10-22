"""Prints the `data` messages ghostd publishes on the given topics, one JSON line each, for N seconds.

usage: uv run --no-project python -I scripts/topic-tap.py <socket> <seconds> <topic>...
"""

import json
import socket
import sys
import time

EXIT_USAGE = 64
PROTO_VERSION = 1
READ_BYTES = 65536


def main(argv: list[str]) -> int:
    if len(argv) < 3:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return EXIT_USAGE
    path, seconds, topics = argv[0], float(argv[1]), argv[2:]
    deadline = time.monotonic() + seconds
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.connect(path)
        hello = {"t": "hello", "proto": PROTO_VERSION, "topics": topics}
        client.sendall((json.dumps(hello) + "\n").encode())
        pending = b""
        while (left := deadline - time.monotonic()) > 0:
            client.settimeout(left)
            try:
                chunk = client.recv(READ_BYTES)
            except TimeoutError:
                break
            if not chunk:
                print("ghostd closed the connection", file=sys.stderr)
                return 1
            pending += chunk
            *lines, pending = pending.split(b"\n")
            for line in lines:
                message = json.loads(line)
                if message.get("t") == "error":
                    print(f"ghostd: {message.get('message')}", file=sys.stderr)
                    return 1
                # The first message for each topic may replay the daemon's last value; it is printed
                # too, and its seq tells it apart.
                if message.get("t") == "data" and message.get("topic") in topics:
                    print(line.decode(), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

"""Serves a stand-in org.freedesktop.login1 on the spike's private system bus, so the headless KWin's screen
locker finds a session of its own instead of the user's. SIGUSR1 emits the session's `Unlock` signal, which
kscreenlocker honours by ending its greeter. Every call is logged with its epoch milliseconds.

usage: uv run --no-project --with dbus-next python -I fake_logind.py <bus address>
"""

# dbus-next reads each D-Bus signature from a string annotation, which ruff takes for an undefined name.
# ruff: noqa: F821

import asyncio
import signal
import sys
import time

from dbus_next.aio import MessageBus
from dbus_next.constants import PropertyAccess
from dbus_next.service import ServiceInterface, dbus_property, method
from dbus_next.service import signal as dbus_signal

EXIT_USAGE = 64
SERVICE = "org.freedesktop.login1"
MANAGER_PATH = "/org/freedesktop/login1"
SESSION_PATH = "/org/freedesktop/login1/session/spike"


def log(text: str) -> None:
    print(f"fake-logind {text} at {time.time_ns() // 1_000_000}", flush=True)


class Manager(ServiceInterface):
    def __init__(self) -> None:
        super().__init__("org.freedesktop.login1.Manager")

    @method()
    def GetSession(self, session_id: "s") -> "o":
        log(f"GetSession({session_id!r})")
        return SESSION_PATH

    @dbus_signal()
    def PrepareForSleep(self, going: "b") -> "b":
        return going


class Session(ServiceInterface):
    def __init__(self) -> None:
        super().__init__("org.freedesktop.login1.Session")
        self.locked_hint = False

    @method()
    def SetLockedHint(self, locked: "b"):
        log(f"SetLockedHint({locked})")
        self.locked_hint = locked

    @dbus_property(access=PropertyAccess.READ)
    def LockedHint(self) -> "b":
        log("Get(LockedHint)")
        return self.locked_hint

    @dbus_signal(name="Unlock")
    def unlock(self) -> None:
        log("emit Unlock")


async def serve(address: str) -> None:
    bus = await MessageBus(bus_address=address).connect()
    session = Session()
    bus.export(MANAGER_PATH, Manager())
    bus.export(SESSION_PATH, session)
    await bus.request_name(SERVICE)
    log("ready")
    loop = asyncio.get_running_loop()
    loop.add_signal_handler(signal.SIGUSR1, session.unlock)
    stopped = loop.create_future()
    loop.add_signal_handler(signal.SIGTERM, stopped.set_result, None)
    await stopped


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return EXIT_USAGE
    asyncio.run(serve(argv[0]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

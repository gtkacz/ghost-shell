"""Stands in for ghostd on the spike's private session bus: owns org.ghostshell.Ghostd and serves the window
bridge the bar will use, plus a control object for the spike's driver. Every event is logged with its
epoch milliseconds, the clock KWin's script stamps its own log with.

    /org/ghostshell/Windows  org.ghostshell.Windows1.Report(s windows_json)
                             org.ghostshell.WindowCommands1.Next() -> s, a queued command at once, or ""
                             after NEXT_TIMEOUT_S
    /org/ghostshell/Desktop  org.ghostshell.Desktop1.ToggleLauncher(), and Report(s, b) from the copied
                             ghost-shell-desktop logic
    /org/ghostshell/Spike    org.ghostshell.Spike1.Queue(s command_json) and Windows() -> s, the last report

usage: uv run --no-project --with dbus-next python -I stub_ghostd.py <bus address>
"""

# dbus-next reads each D-Bus signature from a string annotation, which ruff takes for an undefined name.
# ruff: noqa: F821

import asyncio
import signal
import sys
import time

from dbus_next.aio import MessageBus
from dbus_next.service import ServiceInterface, method

EXIT_USAGE = 64
PRIVATE_BUS_PREFIX = "unix:"
# The user's own buses live under /run/user; the spike's private ones under /tmp.
LIVE_BUS_DIR = "/run/user/"
SERVICE = "org.ghostshell.Ghostd"
NEXT_TIMEOUT_S = 20


# dbus-next takes an empty address for the default session bus, which here would be the user's live one.
def private_bus(address: str) -> str:
    if not address.startswith(PRIVATE_BUS_PREFIX) or LIVE_BUS_DIR in address:
        raise SystemExit(f"refusing bus address {address!r}: only a private {PRIVATE_BUS_PREFIX} bus is allowed")
    return address


def log(text: str) -> None:
    print(f"stub {text} at {time.time_ns() // 1_000_000}", flush=True)


class Windows(ServiceInterface):
    def __init__(self, state: dict) -> None:
        super().__init__("org.ghostshell.Windows1")
        self.state = state

    @method()
    def Report(self, windows_json: "s"):
        self.state["windows"] = windows_json
        log(f"report {windows_json}")


class WindowCommands(ServiceInterface):
    def __init__(self, commands: asyncio.Queue) -> None:
        super().__init__("org.ghostshell.WindowCommands1")
        self.commands = commands

    @method()
    async def Next(self) -> "s":
        log("next called")
        try:
            command = await asyncio.wait_for(self.commands.get(), NEXT_TIMEOUT_S)
        except TimeoutError:
            log("next timed out")
            return ""
        log(f"next returns {command}")
        return command


class Desktop(ServiceInterface):
    def __init__(self) -> None:
        super().__init__("org.ghostshell.Desktop1")

    @method()
    def ToggleLauncher(self):
        log("ToggleLauncher")

    @method()
    def Report(self, covered: "s", active: "b"):
        log(f"desktop report covered={covered!r} active={active}")


class Spike(ServiceInterface):
    def __init__(self, state: dict, commands: asyncio.Queue) -> None:
        super().__init__("org.ghostshell.Spike1")
        self.state = state
        self.commands = commands

    @method()
    def Queue(self, command_json: "s"):
        log(f"queued {command_json}")
        self.commands.put_nowait(command_json)

    @method()
    def Windows(self) -> "s":
        return self.state["windows"]


async def serve(address: str) -> None:
    bus = await MessageBus(bus_address=address).connect()
    state = {"windows": ""}
    commands: asyncio.Queue = asyncio.Queue()
    bus.export("/org/ghostshell/Windows", Windows(state))
    bus.export("/org/ghostshell/Windows", WindowCommands(commands))
    bus.export("/org/ghostshell/Desktop", Desktop())
    bus.export("/org/ghostshell/Spike", Spike(state, commands))
    await bus.request_name(SERVICE)
    log("ready")
    stopped = asyncio.get_running_loop().create_future()
    asyncio.get_running_loop().add_signal_handler(signal.SIGTERM, stopped.set_result, None)
    await stopped


def main(argv: list[str]) -> int:
    if len(argv) != 1:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return EXIT_USAGE
    asyncio.run(serve(private_bus(argv[0])))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

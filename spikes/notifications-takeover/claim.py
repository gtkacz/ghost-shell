"""A stand-in for ghostd's notification server: requests org.freedesktop.Notifications and logs what happens.

usage: uv run --no-project --with dbus-next python -I -B claim.py <private bus address> <request-name flags>
    flags       integer; 2 is REPLACE_EXISTING alone (1 ALLOW_REPLACEMENT, 4 DO_NOT_QUEUE)
    SIGUSR1     release the name but keep running, to see whether plasmashell takes it back
    SIGTERM     exit
Every line is flushed to stdout: the RequestName reply, NameAcquired/NameLost, and each Notify call's arguments.
"""

# dbus-next reads each D-Bus signature from a string annotation, which ruff takes for an undefined name.
# ruff: noqa: F821

import asyncio
import signal
import sys

from dbus_next import Message, MessageType
from dbus_next.aio import MessageBus
from dbus_next.service import ServiceInterface, method

EXIT_USAGE = 64
PRIVATE_BUS_PREFIX = "unix:"
LIVE_BUS_DIR = "/run/user/"
BUS_SERVICE = "org.freedesktop.DBus"
BUS_PATH = "/org/freedesktop/DBus"
NOTIFICATIONS = "org.freedesktop.Notifications"
NOTIFICATIONS_PATH = "/org/freedesktop/Notifications"
REPLY_NAMES = {1: "PRIMARY_OWNER", 2: "IN_QUEUE", 3: "EXISTS", 4: "ALREADY_OWNER"}


def log(message: str) -> None:
    print(message, flush=True)


class Server(ServiceInterface):
    def __init__(self) -> None:
        super().__init__(NOTIFICATIONS)
        self.next_id = 1

    @method()
    def GetCapabilities(self) -> "as":
        return ["body", "actions", "persistence"]

    @method()
    def GetServerInformation(self) -> "ssss":
        return ["claim.py", "ghost-shell spike", "0", "1.2"]

    @method()
    def CloseNotification(self, id: "u") -> None:
        log(f"CloseNotification {id}")

    @method()
    def Notify(self, app: "s", replaces: "u", icon: "s", summary: "s", body: "s", actions: "as", hints: "a{sv}", timeout: "i") -> "u":
        log(f"Notify app={app!r} summary={summary!r} actions={actions} hints={sorted(hints)} timeout={timeout}")
        self.next_id += 1
        return self.next_id - 1


async def bus_call(bus: MessageBus, member: str, signature: str, body: list) -> list:
    reply = await bus.call(
        Message(destination=BUS_SERVICE, path=BUS_PATH, interface=BUS_SERVICE, member=member, signature=signature, body=body)
    )
    if reply.message_type == MessageType.ERROR:
        raise RuntimeError(f"{member} failed: {reply.error_name} {reply.body}")
    return reply.body


async def main(address: str, flags: int) -> None:
    bus = await MessageBus(bus_address=address).connect()
    bus.export(NOTIFICATIONS_PATH, Server())

    def on_signal(message: Message) -> None:
        if message.interface == BUS_SERVICE and message.member in ("NameAcquired", "NameLost") and message.body == [NOTIFICATIONS]:
            log(f"{message.member} {NOTIFICATIONS}")

    bus.add_message_handler(lambda m: on_signal(m) if m.message_type == MessageType.SIGNAL else None)
    (reply,) = await bus_call(bus, "RequestName", "su", [NOTIFICATIONS, flags])
    log(f"unique={bus.unique_name} RequestName flags={flags} -> {reply} {REPLY_NAMES[reply]}")

    loop = asyncio.get_running_loop()
    stopped = loop.create_future()
    loop.add_signal_handler(signal.SIGTERM, stopped.set_result, None)

    def release() -> None:
        asyncio.ensure_future(release_name(bus))

    loop.add_signal_handler(signal.SIGUSR1, release)
    await stopped


async def release_name(bus: MessageBus) -> None:
    (reply,) = await bus_call(bus, "ReleaseName", "s", [NOTIFICATIONS])
    log(f"ReleaseName -> {reply}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(EXIT_USAGE)
    if not sys.argv[1].startswith(PRIVATE_BUS_PREFIX) or LIVE_BUS_DIR in sys.argv[1]:
        raise SystemExit(f"refusing bus address {sys.argv[1]!r}: only a private {PRIVATE_BUS_PREFIX} bus is allowed")
    asyncio.run(main(sys.argv[1], int(sys.argv[2])))

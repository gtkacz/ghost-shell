"""D-Bus calls the plasma-hidden spike makes against its sandboxed plasmashell, kglobalaccel and bus.

usage: uv run --no-project --with dbus-next python -I -B plasma_dbus.py <command> <bus address> …
    eval <script.js>                       run a Plasma script through org.kde.PlasmaShell.evaluateScript
    pid <name>                             print the pid owning <name>, or "none"
    env <KEY=VALUE>…                       add to the bus's activation environment
    start <name>                           activate <name> (StartServiceByName)
    invoke <component> <action>            invokeShortcut on /component/<component>
    actions <component>                    print each action of <component> with its keys as ints
    setkeys <component> <action> <key>…    setForeignShortcutKeys, as the shortcuts KCM does; no key clears it
    unregister <component> <action>        KGlobalAccel's unregister
    own <name>                             own <name> until SIGTERM, so whatever watches for it sees it there
    notify <summary>                       send a notification through org.freedesktop.Notifications
"""

# dbus-next reads each D-Bus signature from a string annotation, which ruff takes for an undefined name.
# ruff: noqa: F821

import asyncio
import signal
import sys
from pathlib import Path

from dbus_next import Message, MessageType
from dbus_next.aio import MessageBus

EXIT_USAGE = 64
PRIVATE_BUS_PREFIX = "unix:"
# The user's own buses live under /run/user; the spike's private ones under /tmp.
LIVE_BUS_DIR = "/run/user/"
BUS_SERVICE = "org.freedesktop.DBus"
BUS_PATH = "/org/freedesktop/DBus"
KGA_SERVICE = "org.kde.kglobalaccel"
KGA_PATH = "/kglobalaccel"
KGA_INTERFACE = "org.kde.KGlobalAccel"
KGA_COMPONENT_INTERFACE = "org.kde.kglobalaccel.Component"
NO_KEY = 0
KEYS_PER_SEQUENCE = 4
NOTIFY_TIMEOUT_MS = 10_000
# Neither StartServiceByName nor RequestName needs a flag here.
NO_FLAGS = 0


# dbus-next takes an empty address for the default session bus, which here would be the user's live one.
def private_bus(address: str) -> str:
    if not address.startswith(PRIVATE_BUS_PREFIX) or LIVE_BUS_DIR in address:
        raise SystemExit(f"refusing bus address {address!r}: only a private {PRIVATE_BUS_PREFIX} bus is allowed")
    return address


def component_path(component: str) -> str:
    # KGlobalAccel names a component's object after its id, with `-` and `.` replaced by `_`.
    return "/component/" + component.replace("-", "_").replace(".", "_")


async def call(
    bus: MessageBus, destination: str, path: str, interface: str, member: str, signature: str = "", body=None
) -> list:
    reply = await bus.call(
        Message(
            destination=destination,
            path=path,
            interface=interface,
            member=member,
            signature=signature,
            body=body or [],
        )
    )
    if reply.message_type == MessageType.ERROR:
        raise RuntimeError(f"{member} failed: {reply.error_name} {reply.body}")
    return reply.body


def action_id(names: list[str]) -> list[str]:
    component, action = names
    return [component, action, "", ""]


async def run(command: str, bus: MessageBus, args: list[str]) -> int:
    if command == "eval" and len(args) == 1:
        script = Path(args[0]).read_text()
        (result,) = await call(
            bus, "org.kde.plasmashell", "/PlasmaShell", "org.kde.PlasmaShell", "evaluateScript", "s", [script]
        )
        print(result)
    elif command == "pid" and len(args) == 1:
        try:
            (pid,) = await call(bus, BUS_SERVICE, BUS_PATH, BUS_SERVICE, "GetConnectionUnixProcessID", "s", args)
        except RuntimeError:
            print("none")
        else:
            print(pid)
    elif command == "env" and args:
        pairs = dict(arg.split("=", 1) for arg in args)
        await call(bus, BUS_SERVICE, BUS_PATH, BUS_SERVICE, "UpdateActivationEnvironment", "a{ss}", [pairs])
    elif command == "start" and len(args) == 1:
        (result,) = await call(bus, BUS_SERVICE, BUS_PATH, BUS_SERVICE, "StartServiceByName", "su", [args[0], NO_FLAGS])
        print(result)
    elif command == "invoke" and len(args) == 2:
        await call(bus, KGA_SERVICE, component_path(args[0]), KGA_COMPONENT_INTERFACE, "invokeShortcut", "s", [args[1]])
    elif command == "actions" and len(args) == 1:
        (infos,) = await call(bus, KGA_SERVICE, component_path(args[0]), KGA_COMPONENT_INTERFACE, "allShortcutInfos")
        for info in infos:
            print(f"{info[0]}\tkeys={info[6]}\tdefaults={info[7]}")
    elif command == "setkeys" and len(args) >= 2:
        keys = [[int(key, 0)] + [NO_KEY] * (KEYS_PER_SEQUENCE - 1) for key in args[2:]]
        await call(
            bus,
            KGA_SERVICE,
            KGA_PATH,
            KGA_INTERFACE,
            "setForeignShortcutKeys",
            "asa(ai)",
            [action_id(args[:2]), [[key] for key in keys]],
        )
    elif command == "unregister" and len(args) == 2:
        (removed,) = await call(bus, KGA_SERVICE, KGA_PATH, KGA_INTERFACE, "unregister", "ss", args)
        print(removed)
    elif command == "notify" and len(args) == 1:
        (notification_id,) = await call(
            bus,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "Notify",
            "susssasa{sv}i",
            ["plasma-hidden spike", 0, "", args[0], "", [], {}, NOTIFY_TIMEOUT_MS],
        )
        print(f"notification {notification_id}")
    elif command == "own" and len(args) == 1:
        (reply,) = await call(bus, BUS_SERVICE, BUS_PATH, BUS_SERVICE, "RequestName", "su", [args[0], NO_FLAGS])
        print(f"own {args[0]}: {reply}", flush=True)
        stopped = asyncio.get_running_loop().create_future()
        asyncio.get_running_loop().add_signal_handler(signal.SIGTERM, stopped.set_result, None)
        await stopped
    else:
        print(__doc__.strip(), file=sys.stderr)
        return EXIT_USAGE
    return 0


async def connect_and_run(argv: list[str]) -> int:
    command, address, args = argv[0], argv[1], argv[2:]
    bus = await MessageBus(bus_address=address).connect()
    return await run(command, bus, args)


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__.strip(), file=sys.stderr)
        return EXIT_USAGE
    private_bus(argv[1])
    return asyncio.run(connect_and_run(argv))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

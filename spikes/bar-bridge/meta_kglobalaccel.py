"""Binds bare Meta through the headless KWin's KGlobalAccel, as ghostd registers its view shortcuts
(crates/ghostd/src/feeds/shortcuts.rs: doRegister, then setShortcutKeys with SET_PRESENT | NO_AUTOLOADING), and
logs each globalShortcutPressed with its epoch milliseconds.

usage: uv run --no-project --with dbus-next python -I -B meta_kglobalaccel.py <command> <bus address> …
    keys <component> <action>      print the action's stored keys as KGlobalAccel's ints
    bykey <key>                    print every action holding <key> (globalShortcutsByKey, Equal)
    register <component> <action> <friendly> <key> [autoload]
                                   register the action with <key>, keep the connection, and log presses of
                                   every component until SIGTERM. "autoload" registers as an app that keeps
                                   the keys KGlobalAccel stored (SET_PRESENT only) instead of <key>
"""

# dbus-next reads each D-Bus signature from a string annotation, which ruff takes for an undefined name.
# ruff: noqa: F821

import asyncio
import signal
import sys
import time

from dbus_next import Message, MessageType
from dbus_next.aio import MessageBus

EXIT_USAGE = 64
SERVICE = "org.kde.kglobalaccel"
ROOT_PATH = "/kglobalaccel"
ROOT_INTERFACE = "org.kde.KGlobalAccel"
COMPONENT_INTERFACE = "org.kde.kglobalaccel.Component"
# `SetShortcutFlag` in kglobalaccel 6.31's src/kglobalaccel_p.h.
SET_PRESENT = 2
NO_AUTOLOADING = 4
# `KGlobalAccel::MatchType::Equal`.
MATCH_EQUAL = 0
NO_KEY = 0
KEYS_PER_SEQUENCE = 4


def log(text: str) -> None:
    print(f"meta-kga {text} at {time.time_ns() // 1_000_000}", flush=True)


def component_path(component: str) -> str:
    # KGlobalAccel names a component's object after its id, with `-` replaced by `_`.
    return "/component/" + component.replace("-", "_").replace(".", "_")


async def call(
    bus: MessageBus, path: str, interface: str, member: str, signature: str = "", body=None, destination: str = SERVICE
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


def describe(info: list) -> str:
    action, friendly, component, component_friendly, context, context_friendly, keys, default_keys = info
    return f"{component}/{action} keys={keys} defaults={default_keys}"


async def keys(bus: MessageBus, component: str, action: str) -> None:
    (infos,) = await call(bus, component_path(component), COMPONENT_INTERFACE, "allShortcutInfos")
    for info in infos:
        if info[0] == action:
            print(describe(info))
            print("as hex: " + " ".join(hex(key) for key in info[6]))


async def bykey(bus: MessageBus, key: int) -> None:
    (holders,) = await call(
        bus, ROOT_PATH, ROOT_INTERFACE, "globalShortcutsByKey", "(ai)(i)", [[[key]], [MATCH_EQUAL]]
    )
    log(f"holders of {hex(key)}: {len(holders)}")
    for info in holders:
        print("  " + describe(info))


def on_message(message: Message) -> None:
    if message.message_type == MessageType.SIGNAL and message.member == "globalShortcutPressed":
        component, action, timestamp = message.body
        log(f"pressed {component}/{action} on {message.path}")


async def register(bus: MessageBus, action_id: list[str], key: int, autoload: bool) -> None:
    bus.add_message_handler(on_message)
    await call(
        bus,
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
        "s",
        [f"type='signal',sender='{SERVICE}',interface='{COMPONENT_INTERFACE}',member='globalShortcutPressed'"],
        destination="org.freedesktop.DBus",
    )
    await call(bus, ROOT_PATH, ROOT_INTERFACE, "doRegister", "as", [action_id])
    sequence = [key] + [NO_KEY] * (KEYS_PER_SEQUENCE - 1)
    flags = SET_PRESENT if autoload else SET_PRESENT | NO_AUTOLOADING
    (granted,) = await call(
        bus, ROOT_PATH, ROOT_INTERFACE, "setShortcutKeys", "asa(ai)u", [action_id, [[sequence]], flags]
    )
    log(f"registered {action_id[0]}/{action_id[1]} asked {hex(key)} flags={flags} granted {granted}")
    stopped = asyncio.get_running_loop().create_future()
    asyncio.get_running_loop().add_signal_handler(signal.SIGTERM, stopped.set_result, None)
    await stopped


async def run(argv: list[str]) -> int:
    command, address, args = argv[0], argv[1], argv[2:]
    bus = await MessageBus(bus_address=address).connect()
    if command == "keys" and len(args) == 2:
        await keys(bus, args[0], args[1])
    elif command == "bykey" and len(args) == 1:
        await bykey(bus, int(args[0], 0))
    elif command == "register" and len(args) in (4, 5):
        action_id = [args[0], args[1], args[0], args[2]]
        await register(bus, action_id, int(args[3], 0), args[4:] == ["autoload"])
    else:
        print(__doc__.strip(), file=sys.stderr)
        return EXIT_USAGE
    return 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__.strip(), file=sys.stderr)
        return EXIT_USAGE
    return asyncio.run(run(argv))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

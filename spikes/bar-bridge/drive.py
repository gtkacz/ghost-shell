"""Drives the window bridge through stub_ghostd.py's control object, then reads the logs for the figures.

usage: uv run --no-project --with dbus-next python -I drive.py <command> …
    wait <bus> <count>                  wait until the last report lists at least <count> windows
    order <bus>                         activate windows in a set order and check each report's order
    cycle <bus> <samples>               <samples> activations alternating between two windows, then
                                        <samples> minimize + activate rounds on one window
    close <bus> <caption prefix>        close every window whose caption starts with the prefix, one by one
    focus <bus> <caption>               activate that window and wait for the report to show it
    state <bus> <caption> <seconds>     wait up to <seconds> for that window, then whether it is active and first
    latency <stub.log> <kwin.log>       queue-to-signal latencies per action
    traffic <bus.log> <from ms> <to ms> D-Bus messages in a window of time, the bridge's apart

Times in both logs are epoch milliseconds.
"""

import asyncio
import json
import re
import statistics
import sys
import time

from dbus_next import Message, MessageType
from dbus_next.aio import MessageBus

EXIT_USAGE = 64
PRIVATE_BUS_PREFIX = "unix:"
# The user's own buses live under /run/user; the spike's private ones under /tmp.
LIVE_BUS_DIR = "/run/user/"
SERVICE = "org.ghostshell.Ghostd"
SPIKE_PATH = "/org/ghostshell/Spike"
SPIKE_INTERFACE = "org.ghostshell.Spike1"
POLL_S = 0.005
STEP_TIMEOUT_S = 3.0
WAIT_TIMEOUT_S = 30.0
# After each command, the driver lets the bridge settle so the next sample starts from a quiet KWin.
SETTLE_S = 0.2
SIGNAL_FOR = {"activate": "windowActivated", "minimize": "minimizedChanged", "close": "windowRemoved"}
QUEUED = re.compile(r"stub queued (\{.*\}) at (\d+)")
REPORTED = re.compile(r"stub report (.*) at (\d+)$")
KWIN_EVENT = re.compile(r"bar-bridge (\w+) (\S+) at (\d+)")
KWIN_COMMAND = re.compile(r"bar-bridge command (\w+) (\S+) at (\d+)")
MONITOR_HEADER = re.compile(r"^(method call|method return|signal|error) time=([0-9.]+) sender=(\S+) -> destination=(\S+)")


# dbus-next takes an empty address for the default session bus, which here would be the user's live one.
def private_bus(address: str) -> str:
    if not address.startswith(PRIVATE_BUS_PREFIX) or LIVE_BUS_DIR in address:
        raise SystemExit(f"refusing bus address {address!r}: only a private {PRIVATE_BUS_PREFIX} bus is allowed")
    return address


class Bridge:
    def __init__(self, bus: MessageBus) -> None:
        self.bus = bus

    async def call(self, member: str, signature: str = "", body: list | None = None) -> list:
        reply = await self.bus.call(
            Message(
                destination=SERVICE,
                path=SPIKE_PATH,
                interface=SPIKE_INTERFACE,
                member=member,
                signature=signature,
                body=body or [],
            )
        )
        if reply.message_type == MessageType.ERROR:
            raise RuntimeError(f"{member} failed: {reply.error_name} {reply.body}")
        return reply.body

    async def windows(self) -> list[dict]:
        (report,) = await self.call("Windows")
        return json.loads(report) if report else []

    async def queue(self, action: str, window_id: str) -> None:
        await self.call("Queue", "s", [json.dumps({"action": action, "id": window_id}, separators=(",", ":"))])

    async def until(self, check, timeout: float = STEP_TIMEOUT_S) -> list[dict]:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            windows = await self.windows()
            if check(windows):
                return windows
            await asyncio.sleep(POLL_S)
        raise TimeoutError("the reports never showed the expected state")

    async def act(self, action: str, window: dict, check) -> list[dict]:
        await self.queue(action, window["id"])
        windows = await self.until(check)
        await asyncio.sleep(SETTLE_S)
        return windows


def by_caption(windows: list[dict], caption: str) -> dict:
    return next(window for window in windows if window["caption"] == caption)


def is_head(window_id: str):
    return lambda windows: bool(windows) and windows[0]["id"] == window_id and windows[0]["active"]


def is_minimized(window_id: str):
    return lambda windows: any(window["id"] == window_id and window["minimized"] for window in windows)


def is_gone(window_id: str):
    return lambda windows: all(window["id"] != window_id for window in windows)


def captions(windows: list[dict]) -> str:
    return ", ".join(window["caption"] + ("*" if window["active"] else "") for window in windows)


async def wait(bridge: Bridge, count: int) -> None:
    windows = await bridge.until(lambda windows: len(windows) >= count, WAIT_TIMEOUT_S)
    print(f"report: {captions(windows)}")
    for window in windows:
        fields = {key: value for key, value in window.items() if key not in ("id", "caption")}
        print(f"  {window['caption']}: {json.dumps(fields)}")


async def order(bridge: Bridge) -> None:
    windows = await bridge.windows()
    # The newest window, b-1, starts active, and activating the active window emits nothing.
    sequence = ["bar-bridge-a-1", "bar-bridge-b-1", "bar-bridge-a-2", "bar-bridge-a-1"]
    expected = [window["caption"] for window in windows]
    matches = 0
    for caption in sequence:
        target = by_caption(windows, caption)
        reported = await bridge.act("activate", target, is_head(target["id"]))
        expected = [caption] + [other for other in expected if other != caption]
        got = [window["caption"] for window in reported]
        matches += got == expected
        print(f"activate {caption}: report {captions(reported)}; expected {', '.join(expected)}")
    print(f"order: {matches}/{len(sequence)} reports in activation order")


async def cycle(bridge: Bridge, samples: int) -> None:
    windows = await bridge.windows()
    pair = [by_caption(windows, "bar-bridge-b-1"), by_caption(windows, "bar-bridge-a-1")]
    for index in range(samples):
        target = pair[index % len(pair)]
        await bridge.act("activate", target, is_head(target["id"]))
    target = by_caption(windows, "bar-bridge-a-2")
    for index in range(samples):
        after = await bridge.act("minimize", target, is_minimized(target["id"]))
        if index == 0:
            print(f"after minimize: {captions(after)}")
        await bridge.act("activate", target, is_head(target["id"]))
    print(f"cycle: {samples} activations and {samples} minimize + activate rounds")


async def close(bridge: Bridge, prefix: str) -> None:
    windows = await bridge.windows()
    targets = [window for window in windows if window["caption"].startswith(prefix)]
    for target in targets:
        after = await bridge.act("close", target, is_gone(target["id"]))
        if target is targets[0]:
            print(f"after close: {captions(after)}")
    print(f"close: {len(targets)} windows closed")


def latency(stub_log: str, kwin_log: str) -> None:
    with open(stub_log) as file:
        stub_lines = file.read().splitlines()
    with open(kwin_log, errors="replace") as file:
        text = file.read()
    events = [(match[1], match[2], int(match[3])) for match in KWIN_EVENT.finditer(text)]
    commands = [(match[1], match[2], int(match[3])) for match in KWIN_COMMAND.finditer(text)]
    reports = [(json.loads(match[1]), int(match[2])) for line in stub_lines if (match := REPORTED.search(line))]
    shows = {"activate": is_head, "minimize": is_minimized, "close": is_gone}
    queued = [(json.loads(match[1]), int(match[2])) for line in stub_lines if (match := QUEUED.search(line))]
    figures: dict[str, dict[str, list[int]]] = {}
    for command, at in queued:
        action, window_id = command["action"], command["id"]
        received = next((t for name, wid, t in commands if name == action and wid == window_id and t >= at), None)
        reached = next((t for name, wid, t in events if name == SIGNAL_FOR[action] and wid == window_id and t >= at), None)
        if received is None or reached is None:
            print(f"unmatched {action} {window_id} queued at {at}")
            continue
        # KWin may report the change from an earlier signal: minimizing activates the next window first.
        shown = shows[action](window_id)
        reported = next((t for windows, t in reports if t >= at and shown(windows)), None)
        unminimized = any(
            name == "minimizedChanged" and wid == window_id and at <= t <= reached for name, wid, t in events
        )
        kind = "activate from minimized" if action == "activate" and unminimized else action
        parts = figures.setdefault(kind, {"to script": [], "to signal": [], "to report at stub": []})
        parts["to script"].append(received - at)
        parts["to signal"].append(reached - at)
        if reported is not None:
            parts["to report at stub"].append(reported - at)
    for action, parts in figures.items():
        for part, values in parts.items():
            ordered = sorted(values)
            print(
                f"{action} {part}: n={len(values)} median={statistics.median(values):g} ms "
                f"min={ordered[0]} p90={ordered[int(0.9 * (len(ordered) - 1))]} max={ordered[-1]}"
            )


def traffic(bus_log: str, start_ms: int, end_ms: int) -> None:
    calls: dict[tuple[str, str], str] = {}
    total = 0
    bridge: dict[str, int] = {}
    with open(bus_log, errors="replace") as file:
        for line in file:
            header = MONITOR_HEADER.match(line)
            if header:
                at = float(header[2]) * 1000
                if not start_ms <= at < end_ms:
                    continue
                total += 1
                kind, sender, destination = header[1], header[3], header[4]
                serial = re.search(r" serial=(\d+)", line)
                reply = re.search(r"reply_serial=(\d+)", line)
                interface = re.search(r"interface=(\S+?);", line)
                member = re.search(r"member=(\S+)", line)
                if kind == "method call" and interface and interface[1].startswith("org.ghostshell."):
                    name = f"call {interface[1]}.{member[1]}"
                    bridge[name] = bridge.get(name, 0) + 1
                    calls[(sender, serial[1])] = name
                elif kind in ("method return", "error") and reply and (destination, reply[1]) in calls:
                    name = f"{kind} to {calls[(destination, reply[1])]}"
                    bridge[name] = bridge.get(name, 0) + 1
    minutes = (end_ms - start_ms) / 60000
    print(f"traffic over {minutes:g} min: {total} messages on the bus, {total / minutes:g}/min")
    for name, count in sorted(bridge.items()):
        print(f"  {name}: {count} ({count / minutes:g}/min)")


async def focus(bridge: Bridge, caption: str) -> None:
    target = by_caption(await bridge.windows(), caption)
    await bridge.act("activate", target, is_head(target["id"]))


async def state(bridge: Bridge, caption: str, seconds: float) -> None:
    def has(windows: list[dict]) -> bool:
        return any(window["caption"] == caption for window in windows)

    try:
        await bridge.until(has, seconds)
    except TimeoutError:
        print(f"{caption} not listed after {seconds:g} s; report: {captions(await bridge.windows())}")
        return
    await asyncio.sleep(SETTLE_S)
    windows = await bridge.windows()
    listed = [window for window in windows if window["caption"] == caption]
    window = listed[0]
    print(
        f"{caption} active={window['active']} first={windows[0] is window} "
        f"demandsAttention={window['demandsAttention']}; report: {captions(windows)}"
    )


async def run_bus(command: str, address: str, args: list[str]) -> None:
    bus = await MessageBus(bus_address=address).connect()
    bridge = Bridge(bus)
    if command == "wait":
        await wait(bridge, int(args[0]))
    elif command == "order":
        await order(bridge)
    elif command == "cycle":
        await cycle(bridge, int(args[0]))
    elif command == "close":
        await close(bridge, args[0])
    elif command == "focus":
        await focus(bridge, args[0])
    elif command == "state":
        await state(bridge, args[0], float(args[1]))


def main(argv: list[str]) -> int:
    bus_commands = {"wait": 1, "order": 0, "cycle": 1, "close": 1, "focus": 1, "state": 2}
    if len(argv) >= 2 and argv[0] in bus_commands and len(argv) == 2 + bus_commands[argv[0]]:
        asyncio.run(run_bus(argv[0], private_bus(argv[1]), argv[2:]))
        return 0
    if len(argv) == 3 and argv[0] == "latency":
        latency(argv[1], argv[2])
        return 0
    if len(argv) == 4 and argv[0] == "traffic":
        traffic(argv[1], int(argv[2]), int(argv[3]))
        return 0
    print(__doc__.strip(), file=sys.stderr)
    return EXIT_USAGE


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

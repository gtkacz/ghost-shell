"""Opens one JobViewV2 on the sandbox plasmashell's org.kde.JobViewServer, as KIO's job tracker does for a copy.

usage: uv run --no-project --with dbus-next python -I -B job.py <private bus address>
Reports the object path it got, shows a half-done 2 GiB copy and holds it open until SIGTERM, then terminates it.
kioclient copy cannot stand in: it has no UI delegate and never calls JobViewServer.
"""

# dbus-next reads each D-Bus signature from a string annotation, which ruff takes for an undefined name.
# ruff: noqa: F821

import asyncio
import signal
import sys

from dbus_next import Variant
from dbus_next.aio import MessageBus

EXIT_USAGE = 64
PRIVATE_BUS_PREFIX = "unix:"
LIVE_BUS_DIR = "/run/user/"
JOB_SERVER = "org.kde.JobViewServer"
JOB_SERVER_PATH = "/JobViewServer"
JOB_VIEW = "org.kde.JobViewV2"
NO_CAPABILITIES = 0
TOTAL_BYTES = 2 * 1024**3
PERCENT = 50
BYTES_PER_SECOND = 200 * 1024**2


async def proxy(bus: MessageBus, path: str, interface: str):
    node = await bus.introspect(JOB_SERVER, path)
    return bus.get_proxy_object(JOB_SERVER, path, node).get_interface(interface)


async def main(address: str) -> None:
    bus = await MessageBus(bus_address=address).connect()
    server = await proxy(bus, JOB_SERVER_PATH, JOB_SERVER)
    path = await server.call_request_view("ghost-shell spike", "edit-copy", NO_CAPABILITIES)
    print(f"job view {path}", flush=True)
    view = await proxy(bus, path, JOB_VIEW)
    await view.call_set_info_message("Copying")
    await view.call_set_description_field(0, "Source", "file:///spike/big.bin")
    await view.call_set_description_field(1, "Destination", "file:///spike/big-copy.bin")
    await view.call_set_total_amount(TOTAL_BYTES, "bytes")
    await view.call_set_processed_amount(TOTAL_BYTES * PERCENT // 100, "bytes")
    await view.call_set_percent(PERCENT)
    await view.call_set_speed(BYTES_PER_SECOND)
    await view.call_set_dest_url(Variant("s", "file:///spike/big-copy.bin"))
    print("job shown", flush=True)
    stopped = asyncio.get_running_loop().create_future()
    asyncio.get_running_loop().add_signal_handler(signal.SIGTERM, stopped.set_result, None)
    await stopped
    await view.call_terminate("")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        sys.exit(EXIT_USAGE)
    if not sys.argv[1].startswith(PRIVATE_BUS_PREFIX) or LIVE_BUS_DIR in sys.argv[1]:
        raise SystemExit(f"refusing bus address {sys.argv[1]!r}: only a private {PRIVATE_BUS_PREFIX} bus is allowed")
    asyncio.run(main(sys.argv[1]))

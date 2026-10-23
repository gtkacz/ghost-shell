#!/usr/bin/env bash
set -euo pipefail

# Prototype of the call behind plasma.popup: toggles one of Plasma's popups from outside, through the
# "activate widget <id>" action plasma-panel.sh's retire gave that widget.
#   popup.sh <session bus> <backup dir> <network|bluetooth|volume|kdeconnect|clipboard|tray|calendar>
# The clipboard's opens only while its history holds something: with none, the tray hides that applet.

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)

if (($# != 3)); then
    sed -n '4,8p' "$0" >&2
    exit 64
fi
bus=$1
backup=$2
name=$3
# Spike only: the real call runs on the user's session bus.
if [[ $bus != unix:* || $bus == *"/run/user/"* ]]; then
    echo "refusing bus address ${bus@Q}" >&2
    exit 1
fi
id=$(awk -F '\t' -v name="$name" '$1 == name { print $2 }' "$backup/popups")
if [[ -z $id ]]; then
    echo "no popup named $name in $backup/popups" >&2
    exit 1
fi
"${py[@]}" "$spike/plasma_dbus.py" invoke "$bus" plasmashell "activate widget $id"

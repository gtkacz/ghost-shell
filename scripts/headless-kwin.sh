#!/usr/bin/env bash
# Starts a headless KWin on a private bus, isolated from the user's session, so a qs run never shows on
# the user's screen. usage: headless-kwin.sh <name>; prints the WAYLAND_DISPLAY to export and the pid
# to stop: kwin_wayland's own pid, so `kill <pid>` ends kwin and, with it, its private bus.
set -euo pipefail
name=${1:?usage: headless-kwin.sh <name>}
root=/tmp/gs-sbx-$name
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
mkdir -p "$root"/{run,config,data,cache,state}
chmod 700 "$root/run"
env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width 2560 --height 1440' \
    "$root" "ghost-sbx-$name" > "$root/kwin.log" 2>&1 &
for _ in $(seq 50); do
    [[ -S $root/run/ghost-sbx-$name ]] && break
    sleep 0.1
done
pid=$(cat "$root/kwin.pid")
[[ -S $root/run/ghost-sbx-$name ]] || { echo "no socket; see $root/kwin.log" >&2; kill "$pid"; exit 1; }
echo "WAYLAND_DISPLAY=$root/run/ghost-sbx-$name"
echo "pid=$pid"

#!/usr/bin/env bash
# Starts a headless KWin on a private bus, isolated from the user's session, so a qs run never shows on
# the user's screen. usage: headless-kwin.sh <name> [outputs]; prints the WAYLAND_DISPLAY to export and the pid
# to stop: kwin_wayland's own pid, so `kill <pid>` ends kwin and, with it, its private bus.
# [outputs] is how many 2560x1440 outputs it has (default 1).
set -euo pipefail
name=${1:?usage: headless-kwin.sh <name> [outputs]}
outputs=${2:-1}
root=/tmp/gs-sbx-$name
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
mkdir -p "$root"/{run,config,data,cache,state}
chmod 700 "$root/run"
env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --output-count "$2" --width 2560 --height 1440' \
    "$root" "ghost-sbx-$name" "$outputs" > "$root/kwin.log" 2>&1 &
session_pid=$!
for _ in $(seq 50); do
    [[ -S $root/run/ghost-sbx-$name ]] && break
    sleep 0.1
done
if [[ ! -S $root/run/ghost-sbx-$name ]]; then
    echo "no socket; kwin.log follows" >&2
    cat -- "$root/kwin.log" >&2 || true
    # The session writes KWin's pid as it starts, so a session still alive gets a moment to write it.
    for _ in $(seq 50); do
        [[ -s $root/kwin.pid ]] || ! kill -0 "$session_pid" 2> /dev/null && break
        sleep 0.1
    done
    if [[ -s $root/kwin.pid ]]; then
        kill "$(< "$root/kwin.pid")" 2> /dev/null || true
    fi
    # The sandbox goes only once the session has ended, since KWin writes there until then; a later run
    # under the same name would otherwise be handed this one's leftovers.
    for _ in $(seq 50); do
        kill -0 "$session_pid" 2> /dev/null || break
        sleep 0.1
    done
    if kill -0 "$session_pid" 2> /dev/null; then
        echo "KWin's session $session_pid did not stop; $root is left in place" >&2
    else
        rm -r -- "$root"
    fi
    exit 1
fi
pid=$(cat "$root/kwin.pid")
echo "WAYLAND_DISPLAY=$root/run/ghost-sbx-$name"
echo "pid=$pid"

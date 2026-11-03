#!/usr/bin/env bash
set -euo pipefail

# Checks that the bar is on screen after its output goes away and after that output returns. The check is on captured
# pixels, because the shell logs "bar on <output>" with or without a window. KWin won't disable its last output, so this
# removes the main one of two, and it cannot reach the path that lost the bar in use: with no outputs left KWin moves
# surfaces to a placeholder output and closes them when a real one returns. So this guards the bar's rebuild and passes
# without it; only the login screen taking the display, and returning, checks that path. Everything runs under env -i
# with a KWin, bus and ghostd of its own, so nothing reaches the user's screen or their running ghostd.

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
name=output-return-$$
root=/tmp/gs-sbx-$name
wayland=ghost-sbx-$name
# Measured: 8 colours with no bar, over 1000 with one.
bar_min_colours=100
deadline_s=20
kwin_stop_s=5

sandbox=(env -i "HOME=$root" PATH=/usr/bin:/bin "XDG_RUNTIME_DIR=$root/run" "XDG_CONFIG_HOME=$root/config"
    "XDG_DATA_HOME=$root/data" "XDG_CACHE_HOME=$root/cache" "XDG_STATE_HOME=$root/state")
grab=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B
    "$repo/spikes/greeting-handoff/grab.py")

session_pid=
daemon_pid=
shell_pid=

stop() {
    local -n pid_var=$1
    if [[ -n $pid_var ]]; then
        kill "$pid_var" 2> /dev/null || true
        wait "$pid_var" 2> /dev/null || true
    fi
}

# The KWin is not a child of this script, so it is polled; it writes to its sandbox until it has gone.
stop_kwin() {
    local kwin_pid deadline=$((SECONDS + kwin_stop_s))
    kwin_pid=$(< "$root/kwin.pid")
    kill "$kwin_pid" 2> /dev/null || true
    while kill -0 "$kwin_pid" 2> /dev/null && (( SECONDS < deadline )); do
        sleep 0.1
    done
}

cleanup() {
    stop shell_pid
    stop daemon_pid
    if [[ -s $root/kwin.pid ]]; then
        stop_kwin
    fi
    stop session_pid
    rm -r -- "$root"
}

fail() {
    echo "output-return: FAIL ($1); the shell's log follows" >&2
    tail -n 20 -- "$root/shell.log" >&2 || true
    exit 1
}

wait_for() {
    local end=$((SECONDS + deadline_s))
    until "$@" > /dev/null 2>&1; do
        if (( SECONDS >= end )); then
            return 1
        fi
        sleep 0.2
    done
}

shot() {
    "${grab[@]}" shot "$session_bus" "$root/shot.png" > /dev/null
}

# Across the whole workspace's top edge, since the bar is on only one of the outputs.
strip_colours() {
    shot
    magick "$root/shot.png" -crop x50+0+0 +repage -format '%k' info:
}

bar_is_showing() {
    (( $(strip_colours) >= bar_min_colours ))
}

workspace_is_wide() {
    shot
    [[ $(magick identify -format '%w' "$root/shot.png") == "$1" ]]
}

set_output() {
    "${sandbox[@]}" "WAYLAND_DISPLAY=$wayland" "DBUS_SESSION_BUS_ADDRESS=$session_bus" QT_QPA_PLATFORM=wayland \
        kscreen-doctor "output.Virtual-0.$1" > /dev/null 2>&1
}

logged() {
    grep -aqF -- "$1" "$root/shell.log"
}

mkdir -p "$root"/{run,config,data,cache,state}
chmod 700 "$root/run"
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

"${sandbox[@]}" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --output-count 2 --width 2560 --height 1440' \
    "$root" "$wayland" > "$root/kwin.log" 2>&1 &
session_pid=$!
wait_for test -S "$root/run/$wayland" || fail "KWin made no socket"
wait_for test -s "$root/session.addr" || fail "KWin wrote no bus address"
session_bus=$(< "$root/session.addr")

cargo build -q -p ghostd --manifest-path "$repo/Cargo.toml"
"$repo/target/debug/ghostd" default-config > "$root/config.toml"
"${sandbox[@]}" "DBUS_SESSION_BUS_ADDRESS=$session_bus" "$repo/target/debug/ghostd" run --fake --socket "$root/ghost.sock" \
    --config "$root/config.toml" --widgets-dir "$repo/widgets" > "$root/daemon.log" 2>&1 &
daemon_pid=$!
"${sandbox[@]}" "WAYLAND_DISPLAY=$wayland" "DBUS_SESSION_BUS_ADDRESS=$session_bus" QT_QPA_PLATFORM=wayland \
    PIPEWIRE_REMOTE=/nonexistent GHOST_GREETING=off "GHOST_SOCKET=$root/ghost.sock" \
    "QML_IMPORT_PATH=$repo/shell/kit" qs --no-color -p "$repo/shell" > "$root/shell.log" 2>&1 &
shell_pid=$!

wait_for logged 'desktop on Virtual-0' || fail "the shell never started on Virtual-0"
wait_for bar_is_showing || fail "no bar at the start, $(strip_colours) colours"
echo "output-return: bar at the start, $(strip_colours) colours"

set_output disable || fail "kscreen-doctor could not disable Virtual-0"
wait_for logged 'bar on Virtual-1' || fail "the shell never moved the bar to Virtual-1"
wait_for workspace_is_wide 2560 || fail "Virtual-0 never went away"
wait_for bar_is_showing || fail "no bar after its output went away, $(strip_colours) colours"
echo "output-return: bar after its output went away, $(strip_colours) colours"

set_output enable || fail "kscreen-doctor could not enable Virtual-0"
wait_for workspace_is_wide 5120 || fail "Virtual-0 never returned"
wait_for bar_is_showing || fail "no bar after the output returned, $(strip_colours) colours"
echo "output-return: bar after the output returned, $(strip_colours) colours"

if grep -aE 'TypeError|ReferenceError|Component failed|is not a type' "$root/shell.log" \
    | grep -avE "Desktop\.qml\[[0-9]+:-1\]: TypeError: Cannot read property 'width' of null"; then
    fail "the shell logged an error"
fi
echo "output-return: PASS"

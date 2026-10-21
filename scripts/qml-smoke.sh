#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
daemon=$repo/target/debug/ghostd

socket=/tmp/ghost-smoke.sock
shell_log=/tmp/ghost-smoke.log
daemon_log=/tmp/ghost-smoke-daemon.log

error_pattern='ReferenceError|TypeError|is not defined|Cannot assign|Error:|failed to load|Unable to'
connected_line='ghostd connected'
required_connects=2

startup_s=6
daemon_down_s=3
recovery_s=5

# A bus address that cannot exist keeps the daemon's config warnings off the user's desktop notifications.
no_session_bus=unix:path=/nonexistent

config=
daemon_pid=
shell_pid=

stop() {
    local pid=$1
    if [[ -n $pid ]] && kill -0 "$pid" 2> /dev/null; then
        kill "$pid"
        wait "$pid" 2> /dev/null || true
    fi
}

cleanup() {
    stop "$shell_pid"
    stop "$daemon_pid"
    rm -f "$socket"
    if [[ -n $config ]]; then
        rm -f "$config"
    fi
}

start_daemon() {
    DBUS_SESSION_BUS_ADDRESS=$no_session_bus "$daemon" run --fake --socket "$socket" --config "$config" \
        --widgets-dir "$repo/widgets" >> "$daemon_log" 2>&1 &
    daemon_pid=$!
}

trap cleanup EXIT
# Turning signals into a plain exit makes bash run the EXIT trap for them too.
trap 'exit 130' INT
trap 'exit 143' TERM

cargo build -q -p ghostd --manifest-path "$repo/Cargo.toml"

config=$(mktemp --suffix=.toml -t ghost-smoke-config.XXXXXX)
"$daemon" default-config > "$config"

: > "$daemon_log"
start_daemon
GHOST_SOCKET=$socket QML_IMPORT_PATH=$repo/shell/kit qs --no-color -p "$repo/shell" > "$shell_log" 2>&1 &
shell_pid=$!

sleep "$startup_s"
stop "$daemon_pid"
sleep "$daemon_down_s"
start_daemon
sleep "$recovery_s"

stop "$shell_pid"
stop "$daemon_pid"

# grep -c prints 0 and exits 1 when nothing matches, which is a valid count here.
connects=$(grep -c "$connected_line" "$shell_log" || true)
errors=$(grep -cE "$error_pattern" "$shell_log" || true)

if (( errors > 0 )); then
    echo "smoke: FAIL ($errors error lines in $shell_log)" >&2
    grep -E "$error_pattern" "$shell_log" >&2
    exit 1
fi
if (( connects < required_connects )); then
    echo "smoke: FAIL ($connects connects, expected at least $required_connects; see $shell_log)" >&2
    exit 1
fi
echo "smoke: ok ($connects connects, $errors errors)"

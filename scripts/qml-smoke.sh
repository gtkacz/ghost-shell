#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
daemon=$repo/target/debug/ghostd

socket=/tmp/ghost-smoke.sock
shell_log=/tmp/ghost-smoke.log
daemon_log=/tmp/ghost-smoke-daemon.log

# The last term catches the daemon refusing the shell, which Data logs at error level and which would
# otherwise only show up as a reconnect loop.
error_pattern='ReferenceError|TypeError|is not defined|Cannot assign|Error:|failed to load|Unable to|ghostd refused the shell'
connected_line='ghostd connected'
required_connects=2

startup_s=6
daemon_down_s=3
reconnect_deadline_s=12
# Long enough for the shell to handle the re-sent config, layout, history and a round of data, so an
# error from any of them reaches the log before the shell stops.
settle_s=2
poll_s=0.2

# A bus address that cannot exist keeps the daemon's config warnings off the user's desktop notifications.
no_session_bus=unix:path=/nonexistent

config=
daemon_pid=
shell_pid=

# Takes the name of a pid variable and clears it, so a later call (the EXIT trap) never signals a pid
# that has already been reaped.
stop() {
    local -n pid_var=$1
    if [[ -n $pid_var ]]; then
        kill "$pid_var" 2> /dev/null || true
        wait "$pid_var" 2> /dev/null || true
        pid_var=
    fi
}

cleanup() {
    stop shell_pid
    stop daemon_pid
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

# grep -c prints 0 and exits 1 when nothing matches, which is a valid count here.
connect_count() {
    grep -c "$connected_line" "$shell_log" || true
}

# Returns once the shell has connected $1 times, or when the deadline passes.
wait_for_connects() {
    local wanted=$1
    local deadline=$((SECONDS + reconnect_deadline_s))
    while (( SECONDS < deadline )); do
        if (( $(connect_count) >= wanted )); then
            return
        fi
        sleep "$poll_s"
    done
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
stop daemon_pid
sleep "$daemon_down_s"
start_daemon
wait_for_connects "$required_connects"
sleep "$settle_s"

stop shell_pid
stop daemon_pid

connects=$(connect_count)
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

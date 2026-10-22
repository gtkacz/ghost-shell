#!/usr/bin/env bash
set -euo pipefail

# GHOST_SMOKE_BUS=private runs the daemon and the shell on a bus made for this run; otherwise the
# daemon gets a bus that cannot exist and the shell keeps the session's.
bus=${GHOST_SMOKE_BUS:-}
# View actions sent to the daemon over that bus once the shell has reconnected, in order, for example
# "monitor monitor swap". Each one must change the view, so pass only actions the config can take.
read -ra actions <<< "${GHOST_SMOKE_ACTIONS:-}"

if [[ -n $bus && $bus != private ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_BUS must be empty or \"private\", got \"$bus\")" >&2
    exit 1
fi
if (( ${#actions[@]} > 0 )) && [[ $bus != private ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_ACTIONS needs GHOST_SMOKE_BUS=private, or they would reach the session's own ghostd)" >&2
    exit 1
fi
# One dbus-run-session around both processes: the script runs again inside it, told which bus it
# left behind and where to write its pid, and this one waits for it. Stopping dbus-run-session would
# leave the shell, the daemon and the bus running, so a stop of this script goes to the inner one,
# whose cleanup stops the shell and the daemon; the bus then ends with dbus-run-session.
if [[ $bus == private && -z ${GHOST_SMOKE_OUTER_BUS+set} ]]; then
    # The inner script writes its pid before it starts anything, so this wait is short.
    inner_start_s=5
    bus_run_dir=$(mktemp -d -t ghost-smoke-bus.XXXXXX)
    inner_pid_file=$bus_run_dir/inner.pid
    session_pid=

    stop_inner() {
        local deadline=$((SECONDS + inner_start_s))
        while [[ ! -s $inner_pid_file ]] && kill -0 "$session_pid" 2> /dev/null && (( SECONDS < deadline )); do
            sleep 0.1
        done
        # Only while dbus-run-session lives, which it does for as long as the inner script does, so the
        # pid cannot have been reused.
        if [[ -s $inner_pid_file ]] && kill -0 "$session_pid" 2> /dev/null; then
            kill "$(< "$inner_pid_file")" 2> /dev/null || true
        fi
        wait "$session_pid" 2> /dev/null || true
    }

    leave_private_bus() {
        if [[ -n $session_pid ]]; then
            stop_inner
        fi
        rm -r -- "$bus_run_dir"
    }

    trap leave_private_bus EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    env GHOST_SMOKE_OUTER_BUS="${DBUS_SESSION_BUS_ADDRESS:-}" GHOST_SMOKE_PID_FILE="$inner_pid_file" \
        dbus-run-session --config-file="$(dirname "${BASH_SOURCE[0]}")/private-bus.conf" -- "${BASH_SOURCE[0]}" "$@" &
    session_pid=$!
    session_status=0
    wait "$session_pid" || session_status=$?
    session_pid=
    exit "$session_status"
fi
if [[ $bus == private ]] && [[ -z ${DBUS_SESSION_BUS_ADDRESS:-} || $DBUS_SESSION_BUS_ADDRESS == "$GHOST_SMOKE_OUTER_BUS" ]]; then
    echo "smoke: FAIL (not on a bus of its own: DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-})" >&2
    exit 1
fi
if [[ -n ${GHOST_SMOKE_PID_FILE:-} ]]; then
    echo "$$" > "$GHOST_SMOKE_PID_FILE"
fi

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
daemon=$repo/target/debug/ghostd

# Two smokes at once, such as one per worktree, each need a socket of their own; the logs sit beside it.
socket=${GHOST_SMOKE_SOCKET:-/tmp/ghost-smoke.sock}
shell_log=${socket%.sock}.log
daemon_log=${socket%.sock}-daemon.log

# shellcheck source=shell-log-errors.sh
source "$repo/scripts/shell-log-errors.sh"
connected_line='ghostd connected'
# What Views logs for a change it prints; a change it shows at once takes 0 ms.
view_change_line='qml: view [^ ]+ -> [^ ]+ \([1-9][0-9]* ms\)'

if ! allow_problem=$(check_extra_allowed); then
    echo "smoke: FAIL ($allow_problem)" >&2
    exit 1
fi
required_connects=2

startup_s=6
daemon_down_s=3
reconnect_deadline_s=12
# Long enough for the shell to handle the re-sent config, layout, history and a round of data, so an
# error from any of them reaches the log before the shell stops.
settle_s=2
poll_s=0.2
# Longer than the slowest print (640 ms), so no action lands while the views are busy.
action_gap_s=1

# A bus address that cannot exist keeps the daemon's config warnings off the user's desktop notifications.
no_session_bus=unix:path=/nonexistent
daemon_bus=$no_session_bus
if [[ $bus == private ]]; then
    daemon_bus=$DBUS_SESSION_BUS_ADDRESS
fi

# GHOST_CONFIG runs the smoke test against a given config; without it a temporary copy of the
# defaults is used and removed afterwards.
config=${GHOST_CONFIG:-}
temp_config=
# The shell's own state, such as the media widget's remembered track, lives in a directory made for
# this run, so a smoke run never reads or writes the desktop shell's.
state_dir=
# Set once this run has made sure no other run's socket is at $socket, so cleanup never removes one.
owns_socket=
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
    if [[ -n $owns_socket ]]; then
        rm -f -- "$socket"
    fi
    if [[ -n $temp_config ]]; then
        rm -f "$temp_config"
    fi
    if [[ -n $state_dir ]]; then
        rm -r -- "$state_dir"
    fi
}

start_daemon() {
    DBUS_SESSION_BUS_ADDRESS=$daemon_bus "$daemon" run --fake --socket "$socket" --config "$config" \
        --widgets-dir "$repo/widgets" >> "$daemon_log" 2>&1 &
    daemon_pid=$!
}

desktop_call() {
    if ! busctl --user call org.ghostshell.Ghostd /org/ghostshell/Desktop org.ghostshell.Desktop1 "$@"; then
        echo "smoke: FAIL (busctl call $* failed; see $daemon_log)" >&2
        exit 1
    fi
}

# The restarted daemon has had no report, and takes actions only while the desktop has focus.
send_actions() {
    desktop_call Report sb "" true
    local action
    for action in "${actions[@]}"; do
        sleep "$action_gap_s"
        desktop_call Activate s "$action"
    done
    sleep "$action_gap_s"
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

if [[ -z $config ]]; then
    temp_config=$(mktemp --suffix=.toml -t ghost-smoke-config.XXXXXX)
    "$daemon" default-config > "$temp_config"
    config=$temp_config
elif [[ ! -r $config ]]; then
    echo "smoke: FAIL (cannot read GHOST_CONFIG $config)" >&2
    exit 1
fi

state_dir=$(mktemp -d -t ghost-smoke-state.XXXXXX)
# The shell's audio rows and the daemon's spectrum would otherwise read the user's own PipeWire graph,
# and a held MIC LIVE alert from their record streams would change what the run sees.
mkdir -- "$state_dir/pipewire"
export PIPEWIRE_REMOTE=/nonexistent PIPEWIRE_RUNTIME_DIR=$state_dir/pipewire

if [[ -e $socket ]]; then
    echo "smoke: FAIL ($socket exists: another run is using it, or a stopped one left it behind)" >&2
    exit 1
fi
owns_socket=1
: > "$daemon_log"
start_daemon
# The greeting would hide the dashboard for its first seconds; GHOST_GREETING=login or auto plays it.
GHOST_GREETING=${GHOST_GREETING:-off} GHOST_SOCKET=$socket QML_IMPORT_PATH=$repo/shell/kit XDG_STATE_HOME=$state_dir qs --no-color -p "$repo/shell" > "$shell_log" 2>&1 &
shell_pid=$!

sleep "$startup_s"
stop daemon_pid
sleep "$daemon_down_s"
start_daemon
wait_for_connects "$required_connects"
sleep "$settle_s"
if (( ${#actions[@]} > 0 )); then
    send_actions
fi

stop shell_pid
stop daemon_pid

connects=$(connect_count)
view_changes=$(grep -cE "$view_change_line" "$shell_log" || true)

error_lines=$(shell_log_errors "$shell_log")
errors=$(grep -c . <<< "$error_lines" || true)
allowed=$(shell_log_allowed "$shell_log")
mapfile -t missing < <(missing_fixture_lines "$config" "$shell_log")

if (( errors > 0 )); then
    echo "smoke: FAIL ($errors error lines in $shell_log)" >&2
    echo "$error_lines" >&2
    exit 1
fi
if (( ${#missing[@]} > 0 )); then
    echo "smoke: FAIL (the layout names a test fixture, but $shell_log lacks its line)" >&2
    printf '  missing: %s\n' "${missing[@]}" >&2
    exit 1
fi
if (( connects < required_connects )); then
    echo "smoke: FAIL ($connects connects, expected at least $required_connects; see $shell_log)" >&2
    exit 1
fi
# More changes than actions would be an action replayed, for example after the reconnect.
if (( view_changes != ${#actions[@]} )); then
    echo "smoke: FAIL (${#actions[@]} view actions sent, $view_changes view changes in $shell_log)" >&2
    exit 1
fi
if (( view_changes > 0 )); then
    grep -oE "$view_change_line" "$shell_log" | sed 's/^qml: /smoke: /'
fi
echo "smoke: ok ($connects connects, $errors errors, $allowed allowed, $view_changes view changes)"

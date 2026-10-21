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
# left behind.
if [[ $bus == private && -z ${GHOST_SMOKE_OUTER_BUS+set} ]]; then
    exec env GHOST_SMOKE_OUTER_BUS="${DBUS_SESSION_BUS_ADDRESS:-}" \
        dbus-run-session --config-file="$(dirname "${BASH_SOURCE[0]}")/private-bus.conf" -- "${BASH_SOURCE[0]}" "$@"
fi
if [[ $bus == private ]] && [[ -z ${DBUS_SESSION_BUS_ADDRESS:-} || $DBUS_SESSION_BUS_ADDRESS == "$GHOST_SMOKE_OUTER_BUS" ]]; then
    echo "smoke: FAIL (not on a bus of its own: DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-})" >&2
    exit 1
fi

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
daemon=$repo/target/debug/ghostd

socket=/tmp/ghost-smoke.sock
shell_log=/tmp/ghost-smoke.log
daemon_log=/tmp/ghost-smoke-daemon.log

# "ghostd refused the shell" catches the daemon refusing the shell, which Data logs at error level and
# which would otherwise only show up as a reconnect loop. "ghostd config problem" catches a layout
# naming a widget that is missing or unusable, which the daemon reports instead of refusing.
error_pattern='ReferenceError|TypeError|is not defined|Cannot assign|Error:|failed to load|Unable to|ghostd refused the shell|ghostd config problem'
connected_line='ghostd connected'
# What Views logs for a change it prints; a change it shows at once takes 0 ms.
view_change_line='qml: view [^ ]+ -> [^ ]+ \([1-9][0-9]* ms\)'

# The two test fixtures exist to show that a broken widget and a missing one are reported, so their
# lines never count as errors, and a layout that names a fixture must produce its line.
declare -A fixture_lines=(
    [broken-example]='widget "broken-example" failed to load'
    [not-installed]='no widget named "not-installed" is installed'
)
# Extra shell-log lines to accept, as an extended regex, e.g. for widgets a layout names before they
# exist.
extra_allowed=${GHOST_SMOKE_ALLOW:-}
# A pattern grep cannot compile (exit 2) makes the final filter drop every line, and so does one that
# matches an empty line (exit 0), such as `.*` or `foo|`: either way the smoke test would pass with
# all its errors hidden.
if [[ -n $extra_allowed ]]; then
    allow_status=0
    grep -qE -- "$extra_allowed" <<< "" || allow_status=$?
    if (( allow_status == 2 )); then
        echo "smoke: FAIL (GHOST_SMOKE_ALLOW is not a valid extended regex: $extra_allowed)" >&2
        exit 1
    fi
    if (( allow_status == 0 )); then
        echo "smoke: FAIL (GHOST_SMOKE_ALLOW matches an empty line, so it would hide every error: $extra_allowed)" >&2
        exit 1
    fi
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
    if [[ -n $temp_config ]]; then
        rm -f "$temp_config"
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
if (( ${#actions[@]} > 0 )); then
    send_actions
fi

stop shell_pid
stop daemon_pid

connects=$(connect_count)
view_changes=$(grep -cE "$view_change_line" "$shell_log" || true)

allowed_pattern=$(IFS='|'; echo "${fixture_lines[*]}")
if [[ -n $extra_allowed ]]; then
    allowed_pattern+="|$extra_allowed"
fi
# grep exits 1 when nothing is left, which is the passing case here.
error_lines=$(grep -E "$error_pattern" "$shell_log" | grep -vE "$allowed_pattern" || true)
errors=$(grep -c . <<< "$error_lines" || true)
allowed=$(grep -E "$error_pattern" "$shell_log" | grep -cE "$allowed_pattern" || true)

missing=()
for fixture in "${!fixture_lines[@]}"; do
    if grep -qE "widget *= *\"$fixture\"" "$config" && ! grep -qF "${fixture_lines[$fixture]}" "$shell_log"; then
        missing+=("${fixture_lines[$fixture]}")
    fi
done

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
echo "smoke: ok ($connects connects, $errors errors, $allowed allowed, $view_changes view changes)"

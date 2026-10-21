#!/usr/bin/env bash
set -euo pipefail

# Measures the dashboard while windows cover it (spec §15: zero dashboard frames, and the shell and the
# daemon together at most 0.5 % of one core), then checks that it shows again once they are gone. The
# covers are reported the way the KWin script reports them, to a release ghostd started for this run on
# a bus made for this run.

if [[ $# -gt 1 || ! ${1:-60} =~ ^[1-9][0-9]*$ ]]; then
    echo "usage: ${0##*/} [seconds=60]" >&2
    exit 64
fi
seconds=${1:-60}

# One dbus-run-session around both processes, as in qml-smoke.sh: the script runs again inside it, told
# which bus it left behind and where to write its pid, and this one waits for it. Stopping
# dbus-run-session would leave the shell, the daemon and the bus running, so a stop of this script goes
# to the inner one, whose cleanup stops the shell and the daemon; the bus then ends with dbus-run-session.
if [[ -z ${GHOST_MEASURE_OUTER_BUS+set} ]]; then
    # The inner script writes its pid before it starts anything, so this wait is short.
    inner_start_s=5
    bus_run_dir=$(mktemp -d -t ghost-covered-bus.XXXXXX)
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
    # The stock session config would let the shell activate the desktop's portals, which draw on the
    # user's display; this one has no service directories.
    env GHOST_MEASURE_OUTER_BUS="${DBUS_SESSION_BUS_ADDRESS:-}" GHOST_MEASURE_PID_FILE="$inner_pid_file" \
        dbus-run-session --config-file="$(dirname "${BASH_SOURCE[0]}")/private-bus.conf" -- "${BASH_SOURCE[0]}" "$@" &
    session_pid=$!
    session_status=0
    wait "$session_pid" || session_status=$?
    session_pid=
    exit "$session_status"
fi

# A Report on the user's own bus would reach their running ghostd and blank their dashboard.
user_bus=unix:path=${XDG_RUNTIME_DIR:-/run/user/$UID}/bus
if [[ -z ${DBUS_SESSION_BUS_ADDRESS:-} || $DBUS_SESSION_BUS_ADDRESS == "$GHOST_MEASURE_OUTER_BUS" \
    || $DBUS_SESSION_BUS_ADDRESS == "$user_bus"* ]]; then
    echo "covered: FAIL (not on a bus of its own: DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-})" >&2
    exit 1
fi
echo "$$" > "$GHOST_MEASURE_PID_FILE"

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
daemon=$repo/target/release/ghostd
measure_cpu=$repo/scripts/measure-cpu.sh

shell_log=/tmp/ghost-covered.log
daemon_log=/tmp/ghost-covered-daemon.log

daemon_name=org.ghostshell.Ghostd
frame_line='syncAndRender: frame rendered'
# What Desktop logs: the main window's output at start, then each change of the covered state.
output_prefix='qml: desktop on '
covered_line='qml: dashboard covered'
visible_line='qml: dashboard visible'

startup_s=8
# Long enough for the shell to unload the view and the daemon to stop the collectors it dropped, so
# the window holds only the steady covered state.
settle_s=5
visible_within_s=2
poll_s=0.1
us_per_s=1000000
max_frames=0
max_total_pct=0.50

run_dir=
daemon_pid=
shell_pid=
shell_meter_pid=
daemon_meter_pid=

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
    stop shell_meter_pid
    stop daemon_meter_pid
    stop shell_pid
    stop daemon_pid
    if [[ -n $run_dir ]]; then
        rm -r -- "$run_dir"
    fi
}

fail() {
    echo "covered: FAIL ($1)" >&2
    exit 1
}

# grep -c prints 0 and exits 1 when nothing matches, which is a valid count here.
line_count() {
    grep -acF -- "$1" "$shell_log" || true
}

# EPOCHREALTIME's separator follows the locale, so every non-digit is dropped.
now_us() {
    echo "${EPOCHREALTIME//[!0-9]/}"
}

# Returns once the shell log holds $2 lines with $1, or fails once visible_within_s has passed.
wait_for_lines() {
    local deadline=$(( $(now_us) + visible_within_s * us_per_s ))
    while (( $(line_count "$1") < $2 )); do
        if (( $(now_us) >= deadline )); then
            return 1
        fi
        sleep "$poll_s"
    done
}

# The Report must reach the daemon started here and no other.
check_bus_owner() {
    local reply
    reply=$(busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus \
        GetConnectionUnixProcessID s "$daemon_name" 2> /dev/null) || fail "$daemon_name has no owner on this bus; see $daemon_log"
    if [[ $reply != "u $daemon_pid" ]]; then
        fail "$daemon_name on this bus is not the ghostd started here ($reply, expected u $daemon_pid)"
    fi
}

report() {
    busctl --user call "$daemon_name" /org/ghostshell/Desktop org.ghostshell.Desktop1 Report sb "$1" false \
        || fail "busctl Report \"$1\" failed; see $daemon_log"
}

cpu_of() {
    sed -n 's/^cpu_pct=\([0-9.]*\) .*/\1/p' "$1"
}

trap cleanup EXIT
# Turning signals into a plain exit makes bash run the EXIT trap for them too.
trap 'exit 130' INT
trap 'exit 143' TERM

cargo build -q --release -p ghostd --manifest-path "$repo/Cargo.toml"

run_dir=$(mktemp -d -t ghost-covered.XXXXXX)
socket=$run_dir/ghostd.sock
# The defaults have no location and no providers, so no weather or sky feed runs.
config=$run_dir/config.toml
"$daemon" default-config > "$config"

"$daemon" run --socket "$socket" --config "$config" --widgets-dir "$repo/widgets" > "$daemon_log" 2>&1 &
daemon_pid=$!
# Without the variables that add a debug overlay, a gallery or a snapshot, whatever the caller has set.
env -u GHOST_DEBUG -u GHOST_VIEW -u GHOST_SNAPSHOT QSG_RENDER_TIMING=1 GHOST_SOCKET="$socket" \
    QML_IMPORT_PATH="$repo/shell/kit" qs --no-color -p "$repo/shell" > "$shell_log" 2>&1 &
shell_pid=$!

sleep "$startup_s"
output=$(sed -n "s/.*$output_prefix//p" "$shell_log" | tail -n 1)
if [[ -z $output ]]; then
    fail "the shell logged no \"${output_prefix#qml: }<output>\" line; see $shell_log"
fi
check_bus_owner

covered_before=$(line_count "$covered_line")
report "$output"
sleep "$settle_s"
covered_logged=$(( $(line_count "$covered_line") > covered_before ))

echo "covered: measuring $output for $seconds s"
frames_before=$(line_count "$frame_line")
"$measure_cpu" "$shell_pid" "$seconds" > "$run_dir/shell.cpu" &
shell_meter_pid=$!
"$measure_cpu" "$daemon_pid" "$seconds" > "$run_dir/daemon.cpu" &
daemon_meter_pid=$!
wait "$shell_meter_pid" || fail "could not measure the shell; see $shell_log"
shell_meter_pid=
wait "$daemon_meter_pid" || fail "could not measure the daemon; see $daemon_log"
daemon_meter_pid=
frames=$(( $(line_count "$frame_line") - frames_before ))

shell_cpu=$(cpu_of "$run_dir/shell.cpu")
daemon_cpu=$(cpu_of "$run_dir/daemon.cpu")
total=$(awk -v shell="$shell_cpu" -v daemon="$daemon_cpu" 'BEGIN { printf "%.2f", shell + daemon }')
echo "covered: frames=$frames shell_cpu=$shell_cpu daemon_cpu=$daemon_cpu total=$total"

visible_before=$(line_count "$visible_line")
report ""
visible_logged=1
wait_for_lines "$visible_line" $(( visible_before + 1 )) || visible_logged=0

failures=()
if (( ! covered_logged )); then
    failures+=("the shell never logged \"${covered_line#qml: }\" after the cover was reported")
fi
if (( frames > max_frames )); then
    failures+=("$frames frames while covered, expected $max_frames")
fi
if ! awk -v total="$total" -v max="$max_total_pct" 'BEGIN { exit !(total <= max) }'; then
    failures+=("total $total % while covered, expected at most $max_total_pct %")
fi
if (( ! visible_logged )); then
    failures+=("no \"${visible_line#qml: }\" within $visible_within_s s of the uncover")
fi
if (( ${#failures[@]} > 0 )); then
    printf 'covered: FAIL (%s)\n' "${failures[@]}" >&2
    exit 1
fi
echo "covered: ok (${visible_line#qml: } within $visible_within_s s of the uncover)"

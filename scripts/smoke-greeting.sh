#!/usr/bin/env bash
set -euo pipefail

# Plays the login greeting and checks what the shell logs: it starts, its first frame comes within the spec's
# 500 ms (§15), it reveals the dashboard, whose texts scramble only after the reveal, and it ends in time. A second
# run with the login marker in place must not greet at all. The shell renders in a headless KWin of its own whose
# runtime directory it uses, so its marker never lands in the user's, and it and a fake ghostd reach no session bus.

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
daemon=$repo/target/debug/ghostd
# Two runs at once, such as one per worktree, each need a socket of their own; the logs sit beside it.
socket=${GHOST_SMOKE_SOCKET:-/tmp/ghost-smoke.sock}
login_log=${socket%.sock}-greeting-login.log
auto_log=${socket%.sock}-greeting-auto.log
daemon_log=${socket%.sock}-greeting-daemon.log
kwin_name=greeting-$(basename "${socket%.sock}" | tr -c 'A-Za-z0-9-\n' '-')
kwin_root=/tmp/gs-sbx-$kwin_name
marker=$kwin_root/run/ghost-shell-greeted

# shellcheck source=shell-log-errors.sh
source "$repo/scripts/shell-log-errors.sh"
playing_line='qml: greeting playing login'
first_frame_line='qml: greeting first frame at '
revealed_line='qml: greeting revealed'
done_line='qml: greeting done'
scramble_line='qml: scramble '

first_frame_budget_ms=500
done_budget_ms=12000
# With the marker present nothing may start the greeting for this long.
quiet_s=5
# qs's log reaches the file late.
settle_s=1
poll_s=0.1
kwin_stop_s=5
no_session_bus=unix:path=/nonexistent

kwin_pid=
wayland_display=
daemon_pid=
shell_pid=
config=
state_dir=
# Set once this run has made sure no other run's socket is at $socket, so cleanup never removes one.
owns_socket=

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

# The KWin is not a child of this script, so it is polled rather than waited for; its sandbox is removed only once
# it has gone, since it writes there until then.
stop_kwin() {
    local deadline=$((SECONDS + kwin_stop_s))
    kill "$kwin_pid" 2> /dev/null || true
    while kill -0 "$kwin_pid" 2> /dev/null && (( SECONDS < deadline )); do
        sleep "$poll_s"
    done
    if kill -0 "$kwin_pid" 2> /dev/null; then
        echo "greeting: KWin $kwin_pid did not stop; $kwin_root is left in place" >&2
        return
    fi
    rm -r -- "$kwin_root"
}

cleanup() {
    stop shell_pid
    stop daemon_pid
    if [[ -n $owns_socket ]]; then
        rm -f -- "$socket"
    fi
    if [[ -n $kwin_pid ]]; then
        stop_kwin
    fi
    if [[ -n $config ]]; then
        rm -- "$config"
    fi
    if [[ -n $state_dir ]]; then
        rm -r -- "$state_dir"
    fi
}

fail() {
    echo "greeting: FAIL ($1)" >&2
    exit 1
}

# Starts the shell with GHOST_GREETING=$1, logging to $2, and leaves the time it was launched at in launch_ms.
start_shell() {
    launch_ms=$(date +%s%3N)
    env -u DISPLAY GHOST_GREETING="$1" GHOST_DEBUG=1 GHOST_SOCKET="$socket" QML_IMPORT_PATH="$repo/shell/kit" \
        XDG_STATE_HOME="$state_dir" XDG_RUNTIME_DIR="$kwin_root/run" WAYLAND_DISPLAY="$wayland_display" \
        QT_QPA_PLATFORM=wayland DBUS_SESSION_BUS_ADDRESS="$no_session_bus" \
        qs --no-color -p "$repo/shell" > "$2" 2>&1 &
    shell_pid=$!
}

# The number of the first line of log $1 holding $2, or nothing.
line_of() {
    grep -anF -- "$2" "$1" | head -n 1 | cut -d: -f1
}

check_errors() {
    local error_lines
    error_lines=$(shell_log_errors "$1")
    if [[ -n $error_lines ]]; then
        fail "$(grep -c . <<< "$error_lines") error lines in $1:"$'\n'"$error_lines"
    fi
}

trap cleanup EXIT
# Turning signals into a plain exit makes bash run the EXIT trap for them too.
trap 'exit 130' INT
trap 'exit 143' TERM

# A sandbox left by another run would hand this one its stale socket.
if [[ -e $kwin_root ]]; then
    fail "$kwin_root exists: another run is using it, or a stopped one left it behind"
fi
if [[ -e $socket ]]; then
    fail "$socket exists: another run is using it, or a stopped one left it behind"
fi
owns_socket=1

cargo build -q -p ghostd --manifest-path "$repo/Cargo.toml"
config=$(mktemp --suffix=.toml -t ghost-greeting-config.XXXXXX)
"$daemon" default-config > "$config"
state_dir=$(mktemp -d -t ghost-greeting-state.XXXXXX)
# The shell's audio rows and the daemon's spectrum would otherwise read the user's own PipeWire graph.
mkdir -- "$state_dir/pipewire"
export PIPEWIRE_REMOTE=/nonexistent PIPEWIRE_RUNTIME_DIR=$state_dir/pipewire

kwin_out=$("$repo/scripts/headless-kwin.sh" "$kwin_name")
kwin_pid=$(sed -n 's/^pid=//p' <<< "$kwin_out")
wayland_display=$(sed -n 's/^WAYLAND_DISPLAY=//p' <<< "$kwin_out")

: > "$daemon_log"
DBUS_SESSION_BUS_ADDRESS=$no_session_bus "$daemon" run --fake --socket "$socket" --config "$config" \
    --widgets-dir "$repo/widgets" >> "$daemon_log" 2>&1 &
daemon_pid=$!

# The login greeting.
start_shell login "$login_log"
done_after_ms=
while (( $(date +%s%3N) - launch_ms <= done_budget_ms )); do
    if grep -qF -- "$done_line" "$login_log"; then
        done_after_ms=$(( $(date +%s%3N) - launch_ms ))
        break
    fi
    sleep "$poll_s"
done
sleep "$settle_s"
stop shell_pid
check_errors "$login_log"

playing_at=$(line_of "$login_log" "$playing_line")
first_frame_at=$(line_of "$login_log" "$first_frame_line")
revealed_at=$(line_of "$login_log" "$revealed_line")
done_at=$(line_of "$login_log" "$done_line")
[[ -n $playing_at ]] || fail "no \"${playing_line#qml: }\" in $login_log"
[[ -n $first_frame_at ]] || fail "no \"${first_frame_line#qml: }<t>\" in $login_log"
[[ -n $revealed_at ]] || fail "no \"${revealed_line#qml: }\" in $login_log"
[[ -n $done_at ]] || fail "no \"${done_line#qml: }\" in $login_log"
if ! (( playing_at < first_frame_at && first_frame_at < revealed_at && revealed_at < done_at )); then
    fail "playing, first frame, revealed and done are out of order in $login_log (lines $playing_at, $first_frame_at, $revealed_at, $done_at)"
fi
first_frame_ms=$(sed -n "${first_frame_at}s/.*${first_frame_line}\([0-9]*\).*/\1/p" "$login_log")
first_frame_after_ms=$(( first_frame_ms - launch_ms ))
if (( first_frame_after_ms > first_frame_budget_ms )); then
    fail "first frame $first_frame_after_ms ms after launch, over $first_frame_budget_ms ms"
fi
early_scrambles=$(sed -n "${playing_at},${revealed_at}p" "$login_log" | grep -acF -- "$scramble_line" || true)
late_scrambles=$(sed -n "${revealed_at},\$p" "$login_log" | grep -acF -- "$scramble_line" || true)
if (( early_scrambles > 0 )); then
    fail "$early_scrambles scramble lines before the reveal in $login_log"
fi
if (( late_scrambles == 0 )); then
    fail "no scramble line after the reveal in $login_log"
fi
if [[ -z $done_after_ms ]]; then
    fail "greeting done not logged within $done_budget_ms ms of launch"
fi
if [[ -e $marker ]]; then
    fail "the login greeting wrote $marker, which only GHOST_GREETING=auto may write"
fi

# A shell started again in a login that has been greeted.
echo greeted > "$marker"
start_shell auto "$auto_log"
sleep "$quiet_s"
stop shell_pid
check_errors "$auto_log"
if grep -qF -- "qml: greeting playing" "$auto_log"; then
    fail "the greeting played with the marker in place; see $auto_log"
fi

echo "greeting: ok (first frame ${first_frame_after_ms} ms after launch, done ${done_after_ms} ms after launch, $late_scrambles scrambles after the reveal, none with the marker)"

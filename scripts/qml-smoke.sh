#!/usr/bin/env bash
set -euo pipefail

# By default the daemon and the shell run on a bus made for this run, so the shell's widgets never
# reach the user's media player or notifications. GHOST_SMOKE_BUS=session gives the daemon a bus that
# cannot exist and lets the shell keep the session's.
bus=${GHOST_SMOKE_BUS:-private}
# View actions sent to the daemon over that bus once the shell has reconnected, in order, for example
# "monitor monitor swap". Each one must change the view, so pass only actions the config can take.
read -ra actions <<< "${GHOST_SMOKE_ACTIONS:-}"

if [[ $bus != private && $bus != session ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_BUS must be \"private\" or \"session\", got \"$bus\")" >&2
    exit 1
fi
if (( ${#actions[@]} > 0 )) && [[ $bus != private ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_ACTIONS need the private bus, or they would reach the session's own ghostd)" >&2
    exit 1
fi
if [[ -n ${GHOST_SMOKE_LAUNCHER:-} && $bus != private ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_LAUNCHER needs the private bus, or its power calls could reach the session's own ghostd)" >&2
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

# GHOST_SMOKE_BAR picks the bar a run shows, as words separated by commas or spaces: top or bottom, a, b or c, and
# broken, which gives the bar a slot naming a panel-only widget that must show as an error mark. Words edit the run's
# own copy of the defaults, so a given GHOST_CONFIG, which the run leaves as it is, can't be combined with them.
smoke_bar_allow=
# What the daemon reports for the broken slot. The shell shows it as a mark, so the line is expected, and its absence is not.
broken_slot_line='ghostd config problem: layout\.bar\.right\[0\]: widget "processor" can.t be placed in the bar'
apply_smoke_bar() {
    local word words=${GHOST_SMOKE_BAR:-}
    for word in ${words//,/ }; do
        case $word in
            top | bottom) sed -i "s/^position = .*/position = \"$word\"/" "$1" ;;
            a | b | c) sed -i "s/^layout = \"[abc]\"/layout = \"$word\"/" "$1" ;;
            broken)
                smoke_bar_allow=$broken_slot_line
                printf '\n[layout.bar]\nleft = [{ widget = "brand" }]\nright = [{ widget = "processor" }, { widget = "view-chip" }, { widget = "bar-clock" }]\n' >> "$1"
                ;;
            *)
                echo "smoke: FAIL (GHOST_SMOKE_BAR takes top, bottom, a, b, c or broken, got \"$word\")" >&2
                exit 1
                ;;
        esac
    done
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

# GHOST_SMOKE_LAUNCHER=1 drives the launcher through its debug IPC. It launches programs, so the shell sees only fake
# desktop entries whose Exec writes a line to a file, and runs with the runtime and system bus of a sandbox, so no
# launch route (systemd, KIO, a service on the bus) can reach the user's session.
launcher=${GHOST_SMOKE_LAUNCHER:-}
launcher_env=()
launcher_launches=
# Gap between two presses of one power button, longer than the launcher's key-repeat guard.
launcher_press_gap_s=0.5
launcher_ipc_s=1
# Long enough for a new window to map and take the focus.
launcher_focus_s=4

# name, id, generic name, categories, keywords
write_launcher_entry() {
    local dir=$1 id=$2 name=$3 generic=$4 categories=$5 keywords=$6
    cat > "$dir/$id.desktop" << ENTRY
[Desktop Entry]
Type=Application
Name=$name
GenericName=$generic
Categories=$categories
Keywords=$keywords
Exec=sh -c "echo launched-$id >> $launcher_launches; cat /proc/self/cgroup > $launcher_launches.cgroup"
ENTRY
}

make_launcher_fixture() {
    local root=$state_dir/launcher
    launcher_launches=$root/launches
    mkdir -p -- "$root/data/applications" "$root/sys/applications" "$root/bin" "$state_dir/run"
    chmod 700 "$state_dir/run"
    : > "$launcher_launches"
    write_launcher_entry "$root/data/applications" firefox Firefox "Web Browser" "Network;WebBrowser;" "browser;"
    write_launcher_entry "$root/data/applications" firewall-config Firewall "Firewall Configuration" "System;Settings;" "security;"
    write_launcher_entry "$root/data/applications" thorium Thorium "Web Browser" "Network;" "browser;"
    write_launcher_entry "$root/data/applications" dolphin Dolphin "File Manager" "System;" "files;"
    write_launcher_entry "$root/data/applications" kate Kate "Text Editor" "Utility;" ""
    printf 'NoDisplay=true\n' >> "$root/data/applications/kate.desktop"
    # The same Dolphin as a system entry, which the launcher must merge into the user's.
    cp -- "$root/data/applications/dolphin.desktop" "$root/sys/applications/org.kde.dolphin.desktop"
    launcher_env=(
        "XDG_RUNTIME_DIR=$state_dir/run"
        "XDG_DATA_HOME=$root/data"
        "XDG_DATA_DIRS=$root/sys"
        "PATH=$root/bin:$PATH"
        "DBUS_SYSTEM_BUS_ADDRESS=$no_session_bus"
        "GHOST_DEBUG=1"
    )
}

launcher_ipc() {
    env "XDG_RUNTIME_DIR=$state_dir/run" qs ipc --pid "$shell_pid" call launcher "$@" > /dev/null \
        || { echo "smoke: FAIL (qs ipc launcher $* failed; see $shell_log)" >&2; exit 1; }
    sleep "$launcher_ipc_s"
}

launcher_count() {
    grep -c -- "$1" "$shell_log" || true
}

launcher_expect() {
    if [[ $2 != "$3" ]]; then
        echo "smoke: FAIL (launcher: $1: expected $3, got $2; see $shell_log)" >&2
        exit 1
    fi
}

run_launcher_phase() {
    launcher_ipc toggle
    launcher_ipc type fire
    launcher_ipc key Down
    launcher_ipc key Return
    launcher_expect "launched firewall-config logged" "$(launcher_count 'launched firewall-config')" 1
    launcher_expect "the fake entry's Exec ran" "$(grep -c '^launched-firewall-config$' "$launcher_launches" || true)" 1
    # With no systemd on the sandbox bus KIO starts a plain child. A launch that stays in the shell's own cgroup never
    # reached the session's systemd manager, which would have put it in an app scope.
    launcher_expect "the launch stayed in the sandbox's cgroup" "$(< "$launcher_launches.cgroup")" "$(< "/proc/$shell_pid/cgroup")"
    # The panel closes 140 ms after a launch.
    launcher_expect "closed after a launch" "$(launcher_count 'launcher closed')" 1

    launcher_ipc toggle
    launcher_ipc power shutdown
    sleep "$launcher_press_gap_s"
    launcher_ipc power shutdown
    launcher_expect "one shutdown fired" "$(launcher_count 'launcher power shutdown')" 1
    launcher_expect "closed after a power action" "$(launcher_count 'launcher closed')" 2

    # Two presses with no gap are a key repeat, which must not confirm.
    launcher_ipc toggle
    launcher_ipc_s=0 launcher_ipc power restart
    launcher_ipc power restart
    launcher_expect "key repeat did not fire" "$(launcher_count 'launcher power restart')" 0

    # A held Return on the focused SHUT DOWN button arms it once; its auto-repeats, even after the key-repeat delay, must not confirm.
    launcher_ipc_s=0 launcher_ipc key Backtab
    launcher_ipc_s=0 launcher_ipc key Return
    launcher_ipc_s=0.03 launcher_ipc repeat Return
    launcher_ipc_s=0.6 launcher_ipc repeat Return
    launcher_ipc repeat Return
    launcher_expect "held Return did not confirm" "$(launcher_count 'launcher power shutdown')" 1
    launcher_ipc key Escape
    launcher_expect "Escape closed" "$(launcher_count 'launcher closed')" 3

    # Another window taking the keyboard focus closes the launcher, which has no catcher over the screen.
    launcher_ipc toggle
    mkdir -p -- "$state_dir/window"
    printf 'import QtQuick\nimport Quickshell\nShellRoot {\n    FloatingWindow {\n        implicitWidth: 300\n        implicitHeight: 200\n        title: "smoke stand-in"\n    }\n}\n' > "$state_dir/window/shell.qml"
    env ${launcher_env[@]+"${launcher_env[@]}"} qs --no-color -p "$state_dir/window" > "$state_dir/window.log" 2>&1 &
    local window_pid=$!
    sleep "$launcher_focus_s"
    kill "$window_pid" 2> /dev/null || true
    wait "$window_pid" 2> /dev/null || true
    launcher_expect "focus moved to another window closed it" "$(launcher_count 'launcher closed')" 4
    launcher_expect "nothing was dropped or refused" "$(grep -cE 'dropped call|ghostd refused call' "$shell_log" || true)" 0
    # No want for the recent files outlives the panel.
    if grep -F 'qml: wants' "$shell_log" | tail -n 1 | grep -q '"recent"'; then
        echo "smoke: FAIL (launcher: the recent topic is still wanted after close)" >&2
        exit 1
    fi
}

trap cleanup EXIT
# Turning signals into a plain exit makes bash run the EXIT trap for them too.
trap 'exit 130' INT
trap 'exit 143' TERM

cargo build -q -p ghostd --manifest-path "$repo/Cargo.toml"

if [[ -z $config ]]; then
    temp_config=$(mktemp --suffix=.toml -t ghost-smoke-config.XXXXXX)
    "$daemon" default-config > "$temp_config"
    apply_smoke_bar "$temp_config"
    config=$temp_config
elif [[ -n ${GHOST_SMOKE_BAR:-} ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_BAR edits the default config, so it can't be combined with GHOST_CONFIG)" >&2
    exit 1
elif [[ ! -r $config ]]; then
    echo "smoke: FAIL (cannot read GHOST_CONFIG $config)" >&2
    exit 1
fi

state_dir=$(mktemp -d -t ghost-smoke-state.XXXXXX)
if [[ -n $launcher ]]; then
    make_launcher_fixture
fi
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
env ${launcher_env[@]+"${launcher_env[@]}"} GHOST_GREETING=${GHOST_GREETING:-off} GHOST_SOCKET=$socket QML_IMPORT_PATH=$repo/shell/kit XDG_STATE_HOME=$state_dir qs --no-color -p "$repo/shell" > "$shell_log" 2>&1 &
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

if [[ -n $launcher ]]; then
    run_launcher_phase
fi

stop shell_pid
stop daemon_pid

connects=$(connect_count)
view_changes=$(grep -cE "$view_change_line" "$shell_log" || true)

error_lines=$(shell_log_errors "$shell_log" ${smoke_bar_allow:+"$smoke_bar_allow"})
errors=$(grep -c . <<< "$error_lines" || true)
allowed=$(shell_log_allowed "$shell_log" ${smoke_bar_allow:+"$smoke_bar_allow"})
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
if [[ -n $smoke_bar_allow ]] && ! grep -qE -- "$smoke_bar_allow" "$shell_log"; then
    echo "smoke: FAIL (the layout gives the bar a broken slot, but $shell_log lacks the daemon's report of it)" >&2
    exit 1
fi
# The daemon's report only says the slot is broken; the mark line says the shell drew it.
if [[ -n $smoke_bar_allow ]] && ! grep -qF -- 'qml: bar mark processor' "$shell_log"; then
    echo "smoke: FAIL (the daemon reported the broken bar slot, but $shell_log lacks the shell's \"bar mark processor\" line)" >&2
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

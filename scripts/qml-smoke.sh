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
# Set by a failed snapshot check, so the snapshots outlive the run.
keep_state=
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
    if [[ -n $state_dir && -z $keep_state ]]; then
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

# Seconds between the fake daemon's own new notifications; empty for none.
notify_every=
# 1 starts the fake daemon with an empty notification history.
notify_empty=

start_daemon() {
    GHOST_FAKE_NOTIFY_EVERY_S=$notify_every GHOST_FAKE_NOTIFY_EMPTY=$notify_empty DBUS_SESSION_BUS_ADDRESS=$daemon_bus "$daemon" run --fake --socket "$socket" --config "$config" \
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

# GHOST_SMOKE_TOASTS=1 has the fake daemon send a notification every 2 s, waits for three toasts and looks at the stack
# in snapshots taken by GHOST_SMOKE_SHOT, an executable that writes a PNG of the whole output to the path it is given
# (the compositor is the caller's, so the snapshot is too). The daemon then restarts with no new notifications, which
# makes every toast leave, and the clock holds must be what they were before the first toast.
toasts=${GHOST_SMOKE_TOASTS:-}
shot_cmd=${GHOST_SMOKE_SHOT:-}
toast_wait_s=20
# The dashboard's alerts hold the clock for appearance.alert_attention_s (10 s) after each connect, so the holds are only
# comparable once that has passed.
attention_s=10
# The first toast times out after 6 s, so this is after it is gone and past the arrival that follows.
toast_gap_s=7
# Wider and taller than three cards and the waiting line at the test scale (QT_SCALE_FACTOR=1.15, where a design pixel is
# about two physical ones), starting past the bar.
stack_region_w=780
stack_region_h=640
stack_region_inset=48
# Cards, borders and text change far more pixels than the dashboard's own motion does.
stack_min_diff_px=3000

# The lines that announce a toast; each toast also logs one saying how it ended.
toast_count() {
    { grep -E 'qml: toast [0-9]+ ' "$shell_log" || true; } | { grep -vc ' ended ' || true; }
}

# The id the first announcement names, or nothing.
first_toast_id() {
    { grep -E 'qml: toast [0-9]+ ' "$shell_log" || true; } | { grep -v ' ended ' || true; } | head -n 1 | sed -n 's/^.*qml: toast \([0-9][0-9]*\) .*$/\1/p'
}

latest_holds() {
    local line
    line=$(grep -aF 'qml: holds ' "$shell_log" | tail -n 1 || true)
    echo "${line#*holds }"
}

# Writes the part of the screen the stack is in to $1, cropped from a snapshot.
take_stack_shot() {
    local whole=$state_dir/whole.png w h x y
    "$shot_cmd" "$whole" || { echo "smoke: FAIL (GHOST_SMOKE_SHOT failed)" >&2; exit 1; }
    read -r w h < <(magick identify -format '%w %h\n' "$whole")
    x=$((w - stack_region_w))
    y=$stack_region_inset
    if [[ $(sed -n 's/^position = "\(.*\)"/\1/p' "$config" | head -n 1) == bottom ]]; then
        y=$((h - stack_region_inset - stack_region_h))
    fi
    magick "$whole" -crop "${stack_region_w}x${stack_region_h}+${x}+${y}" +repage "$1"
}

# Fails unless the stack differs between the snapshots $1 and $2 in at least $4 (default stack_min_diff_px) pixels.
expect_stack_changed() {
    local diff min=${4:-$stack_min_diff_px}
    diff=$(compare -metric AE -fuzz 8% "$1" "$2" null: 2>&1 || true)
    # The metric is a float on some builds.
    diff=${diff%%[. ]*}
    if ! [[ $diff =~ ^[0-9]+$ ]] || (( diff < min )); then
        echo "smoke: FAIL (${shot_phase:-toasts}: $3: $diff pixels differ, expected $min; snapshots in $state_dir)" >&2
        keep_state=1
        exit 1
    fi
}

# Fails unless the toast announced first logs that it ended by timeout within popup_timeout_s + 2 s of its announcement,
# which $1 is the time of, in SECONDS. The pixels cannot show it: new toasts keep arriving and every life line moves.
expect_first_toast_timed_out() {
    local id life_s
    id=$(first_toast_id)
    life_s=$(sed -n 's/^popup_timeout_s *= *\([0-9][0-9]*\).*/\1/p' "$config" | head -n 1)
    life_s=${life_s:-6}
    until grep -qF "qml: toast $id ended timeout" "$shell_log"; do
        if (( SECONDS - ${1:-$SECONDS} > life_s + 2 )); then
            echo "smoke: FAIL (toasts: toast $id did not log 'ended timeout' within $((life_s + 2)) s of its announcement; see $shell_log)" >&2
            exit 1
        fi
        sleep "$poll_s"
    done
}

run_toasts_phase() {
    local before baseline deadline
    sleep $((attention_s + settle_s))
    baseline=$(latest_holds)
    take_stack_shot "$state_dir/stack-empty.png"
    stop daemon_pid
    notify_every=2
    before=$(connect_count)
    start_daemon
    wait_for_connects $((before + 1))
    deadline=$((SECONDS + toast_wait_s))
    first_at=
    while (( $(toast_count) < 3 )); do
        if [[ -z $first_at ]] && (( $(toast_count) > 0 )); then
            first_at=$SECONDS
        fi
        if (( SECONDS >= deadline )); then
            echo "smoke: FAIL (toasts: $(toast_count) toast lines within $toast_wait_s s, expected 3; see $shell_log)" >&2
            exit 1
        fi
        sleep "$poll_s"
    done
    expect_first_toast_timed_out "$first_at"
    sleep "$settle_s"
    take_stack_shot "$state_dir/stack-1.png"
    expect_stack_changed "$state_dir/stack-empty.png" "$state_dir/stack-1.png" "three toasts look like none"
    sleep "$toast_gap_s"
    take_stack_shot "$state_dir/stack-2.png"
    expect_stack_changed "$state_dir/stack-1.png" "$state_dir/stack-2.png" "the stack did not change after the first toast timed out"
    if [[ -n ${GHOST_SMOKE_KEEP_SHOTS:-} ]]; then
        cp -- "$state_dir"/stack-*.png "$GHOST_SMOKE_KEEP_SHOTS"/
    fi

    stop daemon_pid
    notify_every=
    before=$(connect_count)
    start_daemon
    wait_for_connects $((before + 1))
    sleep $((attention_s + settle_s))
    if [[ $(latest_holds) != "$baseline" ]]; then
        echo "smoke: FAIL (toasts: clock holds $(latest_holds) after every toast ended, were $baseline before; see $shell_log)" >&2
        exit 1
    fi
}

# GHOST_SMOKE_CENTRE=1 drives the notification centre through its debug IPC and looks at it in snapshots (see
# GHOST_SMOKE_TOASTS for GHOST_SMOKE_SHOT). The fake daemon logs no calls, so a call that reached it shows in what it
# did: invoking a record it offered a default for removes the record from the history.
centre=${GHOST_SMOKE_CENTRE:-}
centre_ipc_s=1
# Wider and taller than the centre at the test scale (a design pixel is about two physical ones), starting past the bar.
centre_region_w=900
centre_region_h=1250
centre_region_inset=48
# The dashboard behind moves by about 4500 pixels between snapshots; the panel is as dark as the ground it covers, so it
# only changes the pixels of its frame and text.
centre_min_diff_px=15000
# A filter changes the rows only, which is less than the whole panel but more than the dashboard's motion.
centre_min_rows_diff_px=8000

centre_ipc() {
    qs ipc --pid "$shell_pid" call notifications "$@" \
        || { echo "smoke: FAIL (qs ipc notifications $* failed; see $shell_log)" >&2; exit 1; }
}

# Runs the debug IPC call and gives the shell a moment to act on it.
centre_do() {
    centre_ipc "$@" > /dev/null
    sleep "$centre_ipc_s"
}

centre_expect() {
    if [[ $2 != "$3" ]]; then
        echo "smoke: FAIL (centre: $1: expected \"$3\", got \"$2\"; see $shell_log)" >&2
        keep_state=1
        exit 1
    fi
}

centre_log_count() {
    grep -c -- "qml: $1" "$shell_log" || true
}

# Writes the part of the screen the centre is in to $1; the centre is right of the bell, or 12 design pixels from the
# right edge before any click on it.
take_centre_shot() {
    local whole=$state_dir/whole.png w h y
    "$shot_cmd" "$whole" || { echo "smoke: FAIL (GHOST_SMOKE_SHOT failed)" >&2; exit 1; }
    read -r w h < <(magick identify -format '%w %h\n' "$whole")
    y=$centre_region_inset
    if [[ $(sed -n 's/^position = "\(.*\)"/\1/p' "$config" | head -n 1) == bottom ]]; then
        y=$((h - centre_region_inset - centre_region_h))
    fi
    magick "$whole" -crop "${centre_region_w}x${centre_region_h}+$((w - centre_region_w))+${y}" +repage "$1"
}

run_centre_phase() {
    local shot_phase=centre before
    sleep "$settle_s"
    take_centre_shot "$state_dir/centre-closed.png"

    centre_do toggle
    centre_expect "opened by toggle" "$(centre_log_count 'centre opened')" 1
    sleep "$settle_s"
    take_centre_shot "$state_dir/centre-filled.png"
    expect_stack_changed "$state_dir/centre-closed.png" "$state_dir/centre-filled.png" "the open centre looks like none" "$centre_min_diff_px"

    centre_do type disc
    centre_expect "the filter keeps the DISCORD records" "$(centre_ipc status)" "open query=disc index=0 shown=2"
    sleep "$settle_s"
    take_centre_shot "$state_dir/centre-filtered.png"
    expect_stack_changed "$state_dir/centre-filled.png" "$state_dir/centre-filtered.png" "filtering changed nothing" "$centre_min_rows_diff_px"

    centre_do key Down
    centre_expect "Down moves the selection" "$(centre_ipc status)" "open query=disc index=1 shown=2"
    # The second DISCORD record offers no default, so the daemon keeps it; the panel still closes.
    centre_do key Return
    centre_expect "closed after Enter" "$(centre_log_count 'centre closed')" 1

    # The first DISCORD record offers a default, which the daemon answers by removing the record.
    centre_do toggle
    centre_do type disc
    centre_do key Return
    centre_expect "closed after the second Enter" "$(centre_log_count 'centre closed')" 2
    centre_do toggle
    centre_do type disc
    centre_expect "the invoked record is gone from the daemon's history" "$(centre_ipc status)" "open query=disc index=0 shown=1"
    centre_do key Escape
    centre_expect "closed by Escape" "$(centre_log_count 'centre closed')" 3

    stop daemon_pid
    notify_empty=1
    before=$(connect_count)
    start_daemon
    wait_for_connects $((before + 1))
    sleep "$settle_s"
    centre_do toggle
    centre_expect "the empty history lists nothing" "$(centre_ipc status)" "open query= index=0 shown=0"
    sleep "$settle_s"
    take_centre_shot "$state_dir/centre-empty.png"
    expect_stack_changed "$state_dir/centre-closed.png" "$state_dir/centre-empty.png" "the empty centre looks like none" "$centre_min_diff_px"
    centre_do key Escape
    centre_expect "closed after the empty state" "$(centre_log_count 'centre closed')" 4
    centre_expect "nothing was dropped or refused" "$(grep -cE 'dropped call|ghostd refused call' "$shell_log" || true)" 0

    if [[ -n ${GHOST_SMOKE_KEEP_SHOTS:-} ]]; then
        cp -- "$state_dir"/centre-*.png "$GHOST_SMOKE_KEEP_SHOTS"/
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
if [[ ( -n $toasts || -n $centre ) && ( -z $shot_cmd || ! -x $shot_cmd ) ]]; then
    echo "smoke: FAIL (GHOST_SMOKE_TOASTS and GHOST_SMOKE_CENTRE need GHOST_SMOKE_SHOT, an executable that writes a PNG of the output to its argument)" >&2
    exit 1
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
env ${launcher_env[@]+"${launcher_env[@]}"} ${toasts:+GHOST_DEBUG=1} ${centre:+GHOST_DEBUG=1} GHOST_GREETING=${GHOST_GREETING:-off} GHOST_SOCKET=$socket QML_IMPORT_PATH=$repo/shell/kit XDG_STATE_HOME=$state_dir qs --no-color -p "$repo/shell" > "$shell_log" 2>&1 &
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
if [[ -n $toasts ]]; then
    run_toasts_phase
fi
if [[ -n $centre ]]; then
    run_centre_phase
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

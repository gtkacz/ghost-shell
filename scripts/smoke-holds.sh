#!/usr/bin/env bash
set -euo pipefail

# Checks that every FrameClock hold and every Data.want claim returns to where it started after the
# dashboard is covered and uncovered, after every view swap, after every tab of every tabs node and after
# a live style change: a leaked claim keeps costing CPU for as long as the shell runs, and only a
# measurement would show it. The shell renders in a headless KWin of its own, and it and a fake ghostd
# share a bus made for this run, so nothing reaches the user's screen or their running ghostd.

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# Two runs at once, such as one per worktree, each need a socket of their own; the logs sit beside it.
socket=${GHOST_SMOKE_SOCKET:-/tmp/ghost-smoke.sock}

# The outer script starts the KWin, then runs again inside one dbus-run-session, told which bus it left
# behind and where to write its pid, and waits for it. Stopping dbus-run-session would leave the shell,
# the daemon and the bus running, so a stop of this script goes to the inner one, whose cleanup stops
# the shell and the daemon; the bus then ends with dbus-run-session, and the KWin is stopped here.
if [[ -z ${GHOST_HOLDS_OUTER_BUS+set} ]]; then
    # The inner script writes its pid before it starts anything, so this wait is short.
    inner_start_s=5
    kwin_stop_s=5
    kwin_name=holds-$(basename "${socket%.sock}" | tr -c 'A-Za-z0-9-\n' '-')
    kwin_root=/tmp/gs-sbx-$kwin_name
    bus_run_dir=
    session_pid=
    kwin_pid=

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

    # The KWin is not a child of this script, so it is polled rather than waited for; its sandbox is
    # removed only once it has gone, since it writes there until then.
    stop_kwin() {
        local deadline=$((SECONDS + kwin_stop_s))
        kill "$kwin_pid" 2> /dev/null || true
        while kill -0 "$kwin_pid" 2> /dev/null && (( SECONDS < deadline )); do
            sleep 0.1
        done
        if kill -0 "$kwin_pid" 2> /dev/null; then
            echo "holds: KWin $kwin_pid did not stop; $kwin_root is left in place" >&2
            return
        fi
        rm -r -- "$kwin_root"
    }

    leave() {
        if [[ -n $session_pid ]]; then
            stop_inner
        fi
        if [[ -n $kwin_pid ]]; then
            stop_kwin
        fi
        if [[ -n $bus_run_dir ]]; then
            rm -r -- "$bus_run_dir"
        fi
    }

    # A sandbox left by another run would hand this one its stale socket.
    if [[ -e $kwin_root ]]; then
        echo "holds: FAIL ($kwin_root exists: another run is using it, or a stopped one left it behind)" >&2
        exit 1
    fi
    trap leave EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    kwin_out=$("$repo/scripts/headless-kwin.sh" "$kwin_name")
    kwin_pid=$(sed -n 's/^pid=//p' <<< "$kwin_out")
    wayland_display=$(sed -n 's/^WAYLAND_DISPLAY=//p' <<< "$kwin_out")

    bus_run_dir=$(mktemp -d -t ghost-holds-bus.XXXXXX)
    inner_pid_file=$bus_run_dir/inner.pid
    # Without DISPLAY, so nothing can fall back to the user's X server.
    env -u DISPLAY GHOST_HOLDS_OUTER_BUS="${DBUS_SESSION_BUS_ADDRESS:-}" GHOST_HOLDS_PID_FILE="$inner_pid_file" \
        WAYLAND_DISPLAY="$wayland_display" QT_QPA_PLATFORM=wayland \
        dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- "${BASH_SOURCE[0]}" "$@" &
    session_pid=$!
    session_status=0
    wait "$session_pid" || session_status=$?
    session_pid=
    exit "$session_status"
fi

# A Report on the user's own bus would reach their running ghostd and blank their dashboard.
user_bus=unix:path=${XDG_RUNTIME_DIR:-/run/user/$UID}/bus
if [[ -z ${DBUS_SESSION_BUS_ADDRESS:-} || $DBUS_SESSION_BUS_ADDRESS == "$GHOST_HOLDS_OUTER_BUS" \
    || $DBUS_SESSION_BUS_ADDRESS == "$user_bus"* ]]; then
    echo "holds: FAIL (not on a bus of its own: DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS:-})" >&2
    exit 1
fi
echo "$$" > "$GHOST_HOLDS_PID_FILE"

daemon=$repo/target/debug/ghostd
shell_log=${socket%.sock}-holds.log
daemon_log=${socket%.sock}-holds-daemon.log

daemon_name=org.ghostshell.Ghostd
connected_line='ghostd connected'
# What Desktop logs: the main window's output at start, then each change of the covered state.
output_prefix='qml: desktop on '
covered_line='qml: dashboard covered'
visible_line='qml: dashboard visible'
# What Views logs for each change of view.
view_line='qml: view [^ ]+ -> [^ ]+'
# What FrameClock and Data log under GHOST_DEBUG=1.
holds_prefix='qml: holds '
wants_prefix='qml: wants '
reload_line='config reloaded'
# The IPC target Views serves under GHOST_DEBUG=1 to list and choose the tabs on screen.
tabs_target=tabs

connect_deadline_s=12
# Long enough for the layout, the history and a round of data to create every widget and its claims.
startup_s=4
# C21: qs's log reaches the file late, and widgets release their claims through Qt.callLater.
settle_s=2
# Longer than the slowest print (640 ms), so no action lands while the views are busy and every view
# or tab body has been created before the next step.
action_gap_s=1
change_within_s=3
reload_within_s=5
poll_s=0.1
# More main views than any config has; the swap comes back to the first one long before.
max_swaps=16

run_dir=
daemon_pid=
shell_pid=
baseline_holds=
baseline_wants=
rows=()
tabs_shown=0

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
    if [[ -n $run_dir ]]; then
        rm -r -- "$run_dir"
    fi
}

print_table() {
    printf '  %-9s %-34s %-30s %s\n' phase "what ran" holds wants
    printf '  %s\n' "${rows[@]}"
}

fail() {
    echo "holds: FAIL ($1)" >&2
    if (( ${#rows[@]} > 0 )); then
        print_table >&2
    fi
    exit 1
}

# grep -c prints 0 and exits 1 when nothing matches, which is a valid count here.
line_count() {
    grep -acE -- "$2" "$1" || true
}

# The text after $1 on the shell log's latest line holding it, or nothing.
latest() {
    local line
    line=$(grep -aF -- "$1" "$shell_log" | tail -n 1 || true)
    if [[ -n $line ]]; then
        echo "${line#*"$1"}"
    fi
}

# Returns once log $1 holds more than $3 lines matching $2, or fails once $4 seconds have passed.
wait_for_more() {
    local deadline=$((SECONDS + $4))
    while (( $(line_count "$1" "$2") <= $3 )); do
        if (( SECONDS >= deadline )); then
            return 1
        fi
        sleep "$poll_s"
    done
}

desktop_call() {
    busctl --user call "$daemon_name" /org/ghostshell/Desktop org.ghostshell.Desktop1 "$@" \
        || fail "busctl call $* failed; see $daemon_log"
}

ipc() {
    qs ipc --pid "$shell_pid" "$@" || fail "qs ipc $* failed; see $shell_log"
}

# Settles, then adds the latest counts to the table and fails unless they equal the baseline's.
check() {
    local phase=$1 ran=$2 holds wants
    sleep "$settle_s"
    holds=$(latest "$holds_prefix")
    wants=$(latest "$wants_prefix")
    rows+=("$(printf '%-9s %-34s %-30s %s' "$phase" "$ran" "$holds" "$wants")")
    if [[ $holds != "$baseline_holds" ]]; then
        fail "$phase: holds $holds where the baseline is holds $baseline_holds"
    fi
    if [[ $wants != "$baseline_wants" ]]; then
        fail "$phase: wants $wants where the baseline is wants $baseline_wants"
    fi
}

# Sends a view action and prints the change it made as "<from> <to>", or nothing if it made none: the
# config may have one main view or no monitor view, and a gallery shows no view at all.
view_after() {
    local before change
    before=$(line_count "$shell_log" "$view_line")
    desktop_call Activate s "$1" > /dev/null
    if ! wait_for_more "$shell_log" "$view_line" "$before" "$change_within_s"; then
        return 0
    fi
    sleep "$action_gap_s"
    change=$(grep -aoE -- "$view_line" "$shell_log" | tail -n 1)
    change=${change#qml: view }
    echo "${change/ -> / }"
}

# Runs the command $1 on the view shown now, on every other main view the swap action reaches and on
# the monitor view, and ends on the view it started on.
visit_views() {
    local visit=$1 change from to start= swaps=0
    "$visit"
    while true; do
        change=$(view_after swap)
        if [[ -z $change ]]; then
            break
        fi
        read -r from to <<< "$change"
        start=${start:-$from}
        if [[ $to == "$start" ]]; then
            break
        fi
        swaps=$((swaps + 1))
        if (( swaps > max_swaps )); then
            fail "$max_swaps swaps never came back to $start"
        fi
        "$visit"
    done
    change=$(view_after monitor)
    if [[ -n $change ]]; then
        "$visit"
        change=$(view_after monitor)
        if [[ -z $change ]]; then
            fail "the second monitor action did not leave the monitor view"
        fi
    fi
}

no_visit() {
    :
}

# Shows every tab of every tabs node on screen, then the tab each showed before. A shell that never
# created Views, such as a gallery, serves no tabs target, and has no tabs to show.
cycle_tabs() {
    local listing node nodes count chosen index
    listing=$(ipc show)
    if ! grep -qxF "target $tabs_target" <<< "$listing"; then
        return 0
    fi
    listing=$(ipc call "$tabs_target" list)
    if [[ -z $listing ]]; then
        return 0
    fi
    mapfile -t nodes <<< "$listing"
    for node in "${!nodes[@]}"; do
        read -r count chosen <<< "${nodes[node]}"
        for ((index = 0; index < count; index++)); do
            if (( index != chosen )); then
                ipc call "$tabs_target" select "$node" "$index"
                sleep "$action_gap_s"
                tabs_shown=$((tabs_shown + 1))
            fi
        done
        ipc call "$tabs_target" select "$node" "$chosen"
        sleep "$action_gap_s"
    done
}

# The styles a widget's manifest lists, one a line, or nothing for a widget without the setting.
style_values() {
    local manifest=$repo/widgets/$1/manifest.toml
    if [[ -r $manifest ]]; then
        sed -n '/^\[settings\.style\]/,/^\[/s/^values *= *\[\(.*\)\]/\1/p' "$manifest" | grep -oE '"[^"]+"' | tr -d '"'
    fi
}

# "<line> <widget> <style>" for each slot of the config that sets a style, as the config is written:
# a slot's widget comes before its settings in the same table, or on the same line in a tabs list.
styled_slots() {
    awk '
        function quoted(key) {
            if (match($0, key " *= *\"[^\"]+\"")) {
                value = substr($0, RSTART, RLENGTH)
                sub(/^[^"]*"/, "", value)
                sub(/"$/, "", value)
                return value
            }
            return ""
        }
        /^[[:space:]]*\[/ { widget = "" }
        { found = quoted("widget"); if (found != "") { widget = found } }
        { style = quoted("style"); if (style != "" && widget != "") { print NR, widget, style } }
    ' "$1"
}

# Writes to config the sed script that moves every styled slot to the next style its widget lists, and
# prints how many it moves.
flip_styles() {
    local line widget style values index next moved=0 script=()
    while read -r line widget style; do
        mapfile -t values < <(style_values "$widget")
        next=
        for index in "${!values[@]}"; do
            if [[ ${values[index]} == "$style" ]]; then
                next=${values[(index + 1) % ${#values[@]}]}
            fi
        done
        if [[ -n $next && $next != "$style" ]]; then
            script+=(-e "${line}s/style *= *\"$style\"/style = \"$next\"/")
            moved=$((moved + 1))
        fi
    done < <(styled_slots "$config_original")
    if (( moved > 0 )); then
        sed "${script[@]}" "$config_original" > "$config.next"
    fi
    echo "$moved"
}

# Replaces the config by rename, as an editor saves it, and waits for the daemon to apply it.
replace_config() {
    local before
    before=$(line_count "$daemon_log" "$reload_line")
    mv -- "$1" "$config"
    wait_for_more "$daemon_log" "$reload_line" "$before" "$reload_within_s" \
        || fail "style: the daemon did not reload $config; see $daemon_log"
    sleep "$action_gap_s"
}

trap cleanup EXIT
# Turning signals into a plain exit makes bash run the EXIT trap for them too.
trap 'exit 130' INT
trap 'exit 143' TERM

cargo build -q -p ghostd --manifest-path "$repo/Cargo.toml"

run_dir=$(mktemp -d -t ghost-holds.XXXXXX)
# A copy even of a given config, since the style phase edits it; the original is kept to go back to.
config=$run_dir/config.toml
config_original=$run_dir/config.original.toml
if [[ -n ${GHOST_CONFIG:-} ]]; then
    if [[ ! -r $GHOST_CONFIG ]]; then
        fail "cannot read GHOST_CONFIG $GHOST_CONFIG"
    fi
    cp -- "$GHOST_CONFIG" "$config_original"
else
    "$daemon" default-config > "$config_original"
fi
cp -- "$config_original" "$config"
# The shell's own state lives in a directory made for this run, so it never reads or writes the desktop
# shell's.
state_dir=$run_dir/state
mkdir -- "$state_dir"

: > "$daemon_log"
"$daemon" run --fake --socket "$socket" --config "$config" --widgets-dir "$repo/widgets" >> "$daemon_log" 2>&1 &
daemon_pid=$!
GHOST_DEBUG=1 GHOST_SOCKET=$socket QML_IMPORT_PATH=$repo/shell/kit XDG_STATE_HOME=$state_dir \
    qs --no-color -p "$repo/shell" > "$shell_log" 2>&1 &
shell_pid=$!

wait_for_more "$shell_log" "$connected_line" 0 "$connect_deadline_s" \
    || fail "the shell never connected; see $shell_log and $daemon_log"
output=
deadline=$((SECONDS + connect_deadline_s))
while [[ -z $output ]] && (( SECONDS < deadline )); do
    sleep "$poll_s"
    output=$(latest "$output_prefix")
done
if [[ -z $output ]]; then
    fail "the shell logged no \"${output_prefix#qml: }<output>\" line; see $shell_log"
fi
# The daemon takes view actions only while the desktop has focus.
desktop_call Report sb "" true
sleep "$startup_s"
# An alert holds the frame clock while it draws attention and the seconds clock once its window is spent,
# so the baseline waits until every alert on the starting view has spent it; the config may leave the
# window at the daemon's default.
attention_s=$(sed -n 's/^alert_attention_s *= *\([0-9.]*\).*/\1/p' "$config_original")
if [[ -z $attention_s ]]; then
    attention_s=$("$daemon" default-config | sed -n 's/^alert_attention_s *= *\([0-9.]*\).*/\1/p')
fi
sleep "$attention_s"
sleep "$settle_s"

baseline_holds=$(latest "$holds_prefix")
baseline_wants=$(latest "$wants_prefix")
if [[ -z $baseline_holds ]]; then
    fail "no holds line was ever logged; see $shell_log"
fi
if [[ -z $baseline_wants ]]; then
    fail "no wants line was ever logged; see $shell_log"
fi
rows+=("$(printf '%-9s %-34s %-30s %s' baseline "start, uncovered" "$baseline_holds" "$baseline_wants")")

covered_before=$(line_count "$shell_log" "$covered_line")
desktop_call Report sb "$output" false
wait_for_more "$shell_log" "$covered_line" "$covered_before" "$change_within_s" \
    || fail "cover: the shell never logged \"${covered_line#qml: }\" after $output was reported covered"
sleep "$settle_s"
visible_before=$(line_count "$shell_log" "$visible_line")
desktop_call Report sb "" true
wait_for_more "$shell_log" "$visible_line" "$visible_before" "$change_within_s" \
    || fail "cover: the shell never logged \"${visible_line#qml: }\" after the uncover"
sleep "$action_gap_s"
check cover "$output covered, then uncovered"

views_before=$(line_count "$shell_log" "$view_line")
visit_views no_visit
check views "$(( $(line_count "$shell_log" "$view_line") - views_before )) view changes"

views_before=$(line_count "$shell_log" "$view_line")
visit_views cycle_tabs
check tabs "$tabs_shown tabs shown, $(( $(line_count "$shell_log" "$view_line") - views_before )) view changes"

moved=$(flip_styles)
if (( moved == 0 )); then
    echo "style: skipped (no styled widget)"
    rows+=("$(printf '%-9s %s' style "skipped (no styled widget)")")
else
    replace_config "$config.next"
    cp -- "$config_original" "$config.next"
    replace_config "$config.next"
    check style "$moved styles changed, then back"
fi

stop shell_pid
stop daemon_pid

echo "holds: ok"
print_table

#!/usr/bin/env bash
set -euo pipefail

# Plan 3b Task 2: can the desktop-layer surface take keyboard focus on a click, and give it back on Esc?
# usage: run.sh switch|hover|permanent
#   switch      keyboardFocus starts at None, becomes OnDemand on a click on the field, back to None on Esc
#   hover       as switch, and OnDemand while the pointer is over the field
#   permanent   keyboardFocus is OnDemand from the start
# Runs in a headless KWin of its own with no session bus for the clients; nothing reaches the user's display or buses.
#   SPIKE_OUT   directory for logs and the step results, kept afterwards (default: a new temp dir)
#   SPIKE_REMAP 1 makes Esc also unmap the surface for 200 ms (the tag "-remap" goes on the result names)

mode=${1:?usage: run.sh switch|hover|permanent}
spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
tag=$mode${SPIKE_REMAP:+-remap}
name=3b-t2-$tag
root=/tmp/gs-sbx-$name
out=${SPIKE_OUT:-$(mktemp -d -t desktop-focus.XXXXXX)}
results=$out/results-$tag.txt

py=(uv run --no-project python -I)
ready_deadline_s=30
step_s=1
key_a=30
key_b=48
key_esc=1
# Where the stand-in window, the dashboard's claiming area and the search field lie, in physical pixels of the 2560x1440 output
# (QT_SCALE_FACTOR=1.15 puts the QML positions at these divided by 1.15). The window is 1200x700 QML pixels, and
# 800,500 is inside it wherever KWin places it.
other_at="800 500"
widget_at="2127 201"
field_at="2185 1311"

other_pid=
desktop_pid=
key_pid=
kwin_pid=

stop() {
    local -n pid_var=$1
    if [[ -n $pid_var ]]; then
        kill "$pid_var" 2> /dev/null || true
        wait "$pid_var" 2> /dev/null || true
        pid_var=
    fi
}

cleanup() {
    exec 3>&- 2> /dev/null || true
    stop desktop_pid
    stop other_pid
    stop key_pid
    if [[ -n $kwin_pid ]]; then
        kill "$kwin_pid" 2> /dev/null || true
        sleep 1
    fi
    if [[ -d $root ]]; then
        rm -r -- "$root"
    fi
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

note() {
    echo "$*" | tee -a "$results"
}

fail() {
    note "FAILED: $*"
    exit 1
}

has_line() {
    grep -aq -- "$1" "$2" 2> /dev/null
}

wait_for_line() {
    local deadline=$((SECONDS + ready_deadline_s))
    while ((SECONDS < deadline)); do
        has_line "$1" "$2" && return 0
        sleep 0.1
    done
    return 1
}

start_qs() {
    local file=$1 log=$2
    shift 2
    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
        SPIKE_REMAP="${SPIKE_REMAP:-}" WAYLAND_DISPLAY="$wayland" QT_QPA_PLATFORM=wayland QT_SCALE_FACTOR=1.15 GHOST_GREETING=off \
        DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent "$@" qs --no-color -p "$file" > "$log" 2>&1 &
}

# Runs the commands, then prints what each client logged meanwhile.
step() {
    local label=$1
    shift
    local other_before desktop_before cmd
    other_before=$(wc -l < "$out/other-$tag.log")
    desktop_before=$(wc -l < "$out/desktop-$tag.log")
    for cmd in "$@"; do
        echo "$cmd" >&3
        sleep "$step_s"
    done
    note "== $label"
    tail -n +"$((other_before + 1))" "$out/other-$tag.log" | grep -a 'spike' | sed 's/^/   /' | tee -a "$results" || true
    tail -n +"$((desktop_before + 1))" "$out/desktop-$tag.log" | grep -a 'spike' | sed 's/^/   /' | tee -a "$results" || true
}

if [[ -e $root ]]; then
    echo "$root exists; another run is using it or left it behind" >&2
    trap - EXIT
    exit 1
fi
mkdir -p "$out" "$root/data/applications"
: > "$results"
: > "$out/desktop-$tag.log"
note "mode=$mode out=$out"

# KWin grants fake input to the executable a desktop file in its (sandboxed) data dir names; written before it starts.
python_exe=$("${py[@]}" -c 'import os, sys; print(os.path.realpath(sys.executable))')
printf '[Desktop Entry]\nType=Application\nName=desktop-focus input\nExec=%s\nNoDisplay=true\nX-KDE-Wayland-Interfaces=org_kde_kwin_fake_input\n' \
    "$python_exe" > "$root/data/applications/gs-desktop-focus-input.desktop"

kwin_out=$("$repo/scripts/headless-kwin.sh" "$name")
kwin_pid=$(sed -n 's/^pid=//p' <<< "$kwin_out")
wayland=$(sed -n 's/^WAYLAND_DISPLAY=//p' <<< "$kwin_out")
note "kwin pid=$kwin_pid wayland=$wayland; clients get no DBUS_SESSION_BUS_ADDRESS (inherited: ${DBUS_SESSION_BUS_ADDRESS:-unset})"

mkfifo "$root/keys.in"
"${py[@]}" "$spike/fake_input.py" "$wayland" < "$root/keys.in" > "$out/fake-input-$tag.log" 2>&1 &
key_pid=$!
exec 3> "$root/keys.in"
wait_for_line "fake-key ready" "$out/fake-input-$tag.log" || fail "no fake input, see $out/fake-input-$tag.log"

start_qs "$spike/other.qml" "$out/other-$tag.log"
other_pid=$!
wait_for_line "spike other active true" "$out/other-$tag.log" || fail "the stand-in window never became active"
step "1 key A with only the stand-in window mapped" "key $key_a"

start_qs "$spike/focus.qml" "$out/desktop-$tag.log" SPIKE_MODE="$mode"
desktop_pid=$!
wait_for_line "spike desktop ready" "$out/desktop-$tag.log" || fail "the desktop surface never started"
sleep "$step_s"
step "2 key A after the desktop surface mapped" "key $key_a"
step "3 click the stand-in window, key A" "click $other_at" "key $key_a"
step "4 click a claiming area with no field, key A" "click $widget_at" "key $key_a"
step "5 hover the field, click it, keys A B" "move $field_at" "click $field_at" "key $key_a" "key $key_b"
step "6 click the field again, keys A B" "click $field_at" "key $key_a" "key $key_b"
step "7 Esc, then key A" "key $key_esc" "key $key_a"
step "8 click the field, click the stand-in window, key A" "click $field_at" "click $other_at" "key $key_a"
step "9 click the field, click the claiming area, key A" "click $field_at" "click $widget_at" "key $key_a"

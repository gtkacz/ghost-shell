#!/usr/bin/env bash
set -euo pipefail

# Locks a headless KWin over a red stand-in window, maps a ground-coloured Overlay layer under the lock
# screen, unlocks through a stand-in logind and records what the scene shows around the unlock. Nothing
# reaches the user's session: KWin runs on its own session bus, and its screen locker's system bus is a
# second private bus whose only logind is fake_logind.py. Every process is stopped by the pid it was
# started with.
#   SPIKE_OUT        directory for logs, captures and results, kept afterwards (default: a new temp dir)
#   SPIKE_NAMESPACE  the overlay's layer-shell namespace (default: dock). KWin's scale effect fades in a
#                    layer surface whose namespace it does not know (window type Normal), and the fade only
#                    starts at the unlock; `dock` is a type no default open effect animates.

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
name=2c-t1-handoff
root=/tmp/gs-sbx-$name
out=${SPIKE_OUT:-$(mktemp -d -t greeting-handoff-spike.XXXXXX)}
results=$out/results.txt
surface_namespace=${SPIKE_NAMESPACE:-dock}

socket_deadline_s=10
ready_deadline_s=30
lock_deadline_s=30
settle_s=2
locked_watch_s=3
watch_lead_s=1
watch_s=5
key_wait_s=1
poll_s=0.1
# The screen's centre, where the stand-in window, the lock screen and the overlay all lie.
watch_area=(1248 688 64 64)
key_a=30

py=(uv run --no-project --with dbus-next python -I)

system_pid=
system_monitor_pid=
logind_pid=
session_pid=
session_monitor_pid=
key_pid=
red_pid=
overlay_pid=
watch_pid=

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
    stop watch_pid
    stop overlay_pid
    stop red_pid
    stop key_pid
    stop session_monitor_pid
    if [[ -s $root/kwin.pid ]]; then
        kill "$(< "$root/kwin.pid")" 2> /dev/null || true
    fi
    if [[ -n $session_pid ]]; then
        wait "$session_pid" 2> /dev/null || true
    fi
    stop logind_pid
    stop system_monitor_pid
    stop system_pid
    if [[ -d $root ]]; then
        rm -r -- "$root"
    fi
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

wait_for() {
    local deadline=$((SECONDS + $1))
    shift
    while ((SECONDS < deadline)); do
        if "$@"; then
            return 0
        fi
        sleep "$poll_s"
    done
    return 1
}

has_line() {
    grep -aq -- "$1" "$2" 2> /dev/null
}

# dbus-monitor prints each signal's header line, with its time in seconds, before its arguments.
active_changed_ms() {
    awk -v want="boolean $1" '
        /member=ActiveChanged/ { match($0, /time=[0-9.]+/); at = substr($0, RSTART + 5, RLENGTH - 5); next }
        at != "" && index($0, want) { printf "%d\n", at * 1000; exit }
        { at = "" }' "$out/session-bus.log"
}

has_active_changed() {
    [[ -n $(active_changed_ms "$1") ]]
}

note() {
    echo "$*" | tee -a "$results"
}

fail() {
    note "FAILED: $*"
    exit 1
}

grab() {
    "${py[@]}" "$spike/grab.py" shot "$session_bus" "$out/$1.png" | tee -a "$results"
}

type_key() {
    echo "$key_a" >&3
}

start_qs() {
    local file=$1 log=$2
    shift 2
    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
        WAYLAND_DISPLAY="$wayland" QT_QPA_PLATFORM=wayland QT_SCALE_FACTOR=1.15 \
        DBUS_SESSION_BUS_ADDRESS="$session_bus" DBUS_SYSTEM_BUS_ADDRESS="$system_bus" "$@" \
        qs --no-color -p "$file" > "$log" 2>&1 &
}

if [[ -e $root ]]; then
    echo "$root exists; another run is using it or left it behind" >&2
    trap - EXIT
    exit 1
fi
mkdir -p "$root"/{run,config,data,cache,state} "$out"
chmod 700 "$root/run"
: > "$results"
note "out=$out namespace=$surface_namespace"

dbus-daemon --config-file="$repo/scripts/private-bus.conf" --nofork --print-address=3 3> "$root/system.addr" &
system_pid=$!
wait_for "$socket_deadline_s" test -s "$root/system.addr" || fail "no private system bus"
system_bus=$(head -n 1 "$root/system.addr")
dbus-monitor --address "$system_bus" > "$out/system-bus.log" 2>&1 &
system_monitor_pid=$!
"${py[@]}" "$spike/fake_logind.py" "$system_bus" > "$out/fake-logind.log" 2>&1 &
logind_pid=$!
wait_for "$ready_deadline_s" has_line "fake-logind ready" "$out/fake-logind.log" || fail "fake logind never ready"

wayland=$root/run/ghost-sbx-$name
# The lock screen stays on (no --no-lockscreen). Its logind lookups go to the private system bus, and both
# permission switches hold only inside this KWin: ScreenShot2 for grab.py, and fake input for fake_key.py.
env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    DBUS_SYSTEM_BUS_ADDRESS="$system_bus" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --socket "$1" --width 2560 --height 1440' \
    "$root" "ghost-sbx-$name" > "$out/kwin.log" 2>&1 &
session_pid=$!
wait_for "$socket_deadline_s" test -S "$wayland" || fail "KWin made no socket, see $out/kwin.log"
session_bus=$(< "$root/session.addr")
dbus-monitor --address "$session_bus" > "$out/session-bus.log" 2>&1 &
session_monitor_pid=$!

mkfifo "$root/keys.in"
"${py[@]}" "$spike/fake_key.py" "$wayland" < "$root/keys.in" > "$out/fake-key.log" 2>&1 &
key_pid=$!
exec 3> "$root/keys.in"
wait_for "$ready_deadline_s" has_line "fake-key ready" "$out/fake-key.log" || fail "no fake keyboard, see $out/fake-key.log"

start_qs "$spike/window.qml" "$out/red.log"
red_pid=$!
wait_for "$ready_deadline_s" has_line "spike red active true" "$out/red.log" || fail "red window never active"
type_key
wait_for "$ready_deadline_s" has_line "spike red key" "$out/red.log" || fail "the red window got no key before the lock"
note "before lock: the red window holds the keyboard"
grab 1-before-lock

start_qs "$spike/shell.qml" "$out/overlay.log" SPIKE_ARM_FILE="$root/arm" SPIKE_FOCUS_FILE="$root/focus" \
    SPIKE_NAMESPACE="$surface_namespace"
overlay_pid=$!
wait_for "$ready_deadline_s" has_line "spike overlay ready" "$out/overlay.log" || fail "overlay never ready"

dbus-send --bus="$session_bus" --print-reply --dest=org.freedesktop.ScreenSaver /ScreenSaver \
    org.freedesktop.ScreenSaver.Lock > "$out/lock-reply.log" 2>&1 &
wait_for "$lock_deadline_s" has_active_changed true || fail "no ActiveChanged(true), see $out/kwin.log"
note "locked: ActiveChanged(true) at $(active_changed_ms true)"
sleep "$settle_s"
grab 2-locked

touch "$root/arm"
wait_for "$ready_deadline_s" has_line "spike overlay frame 1 " "$out/overlay.log" || fail "the overlay rendered no frame while locked"
note "overlay mapped while locked: $(grep -a -m 1 'spike overlay frame 1 ' "$out/overlay.log")"
sleep "$locked_watch_s"
note "overlay frames after ${locked_watch_s} s under the lock: $(grep -ac 'spike overlay frame ' "$out/overlay.log")"
grab 3-locked-armed

"${py[@]}" "$spike/grab.py" watch "$session_bus" "$out/watch.tsv" "$watch_s" "${watch_area[@]}" >> "$results" 2>&1 &
watch_pid=$!
sleep "$watch_lead_s"
note "overlay frames just before the unlock: $(grep -ac 'spike overlay frame ' "$out/overlay.log")"
kill -USR1 "$logind_pid"
wait_for "$lock_deadline_s" has_active_changed false || fail "no ActiveChanged(false), see $out/kwin.log"
unlocked_ms=$(active_changed_ms false)
note "unlocked: ActiveChanged(false) at $unlocked_ms"

touch "$root/focus"
wait_for "$ready_deadline_s" has_line "spike overlay exclusive keyboard requested" "$out/overlay.log" || fail "overlay never asked for the keyboard"
sleep "$key_wait_s"
type_key
sleep "$key_wait_s"
note "key after the unlock, overlay: $(grep -a 'spike overlay key' "$out/overlay.log" || echo none)"
note "key after the unlock, red: $(grep -a 'spike red key' "$out/red.log" | tail -n +2 || echo none)"
wait "$watch_pid" || true
watch_pid=
grab 4-after-unlock

first_frame=$(awk -v after="$unlocked_ms" '/spike overlay frame / && $NF > after { print; exit }' "$out/overlay.log")
note "first overlay frame after the unlock: $first_frame"
note "captures by class, from the unlock on:"
awk -v after="$unlocked_ms" 'NR > 1 && $1 >= after { count[$5]++ } END { for (kind in count) print "  " kind, count[kind] }' \
    "$out/watch.tsv" | tee -a "$results"
note "class changes in the watch:"
awk 'NR > 1 && $5 != last { print "  " $1, $5; last = $5 }' "$out/watch.tsv" | tee -a "$results"

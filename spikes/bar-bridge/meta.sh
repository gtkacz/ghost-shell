#!/usr/bin/env bash
set -euo pipefail

# Bare Meta as a KGlobalAccel shortcut of ghost-shell's own component, in a headless KWin like run.sh's, with the
# user's kwinrc and kglobalshortcutsrc copied into the sandbox.
#   SPIKE_OUT        directory for logs and results, kept afterwards (default: a new temp dir)
#   SPIKE_META_MODE  retired: bare Meta is first removed from plasmashell's launcher entry in the copy, as the
#                    plan's retire step would;
#                    shared: the entry is kept, and a stand-in plasmashell takes its stored keys before
#                    ghost-shell registers, as the real one does at login
#   SPIKE_META_ORDER plasmashell-first (default) or ghost-first: which registers first in shared mode

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
name=3a-t1-meta
root=/tmp/gs-sbx-$name
out=${SPIKE_OUT:-$(mktemp -d -t bar-bridge-meta.XXXXXX)}
results=$out/results.txt
mode=${SPIKE_META_MODE:-retired}
order=${SPIKE_META_ORDER:-plasmashell-first}

launcher_action="activate application launcher"
launcher_friendly="Activate Application Launcher"
socket_deadline_s=10
ready_deadline_s=30
settle_s=1
key_gap_s=0.5
taps=5
poll_s=0.1
key_meta=125
key_s=31
key_1=2
# Unbound in the user's shortcuts with Meta; Meta+L would lock.
key_k=37

# -B: fake_chord.py imports the 2c spike's fake_key.py, which would otherwise leave a bytecode cache in its dir.
py=(uv run --no-project --with dbus-next python -I -B)

system_pid=
session_pid=
monitor_pid=
plasma_pid=
ghost_pid=
tap_pid=
chord_pid=

stop() {
    local -n pid_var=$1
    if [[ -n $pid_var ]]; then
        kill "$pid_var" 2> /dev/null || true
        wait "$pid_var" 2> /dev/null || true
        pid_var=
    fi
}

cleanup() {
    exec 3>&- 4>&- 2> /dev/null || true
    stop chord_pid
    stop tap_pid
    stop ghost_pid
    stop plasma_pid
    stop monitor_pid
    if [[ -s $root/kwin.pid ]]; then
        kill "$(< "$root/kwin.pid")" 2> /dev/null || true
    fi
    if [[ -n $session_pid ]]; then
        wait "$session_pid" 2> /dev/null || true
    fi
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

count_from() {
    awk -v want="$1" -v from="$3" 'index($0, want) && $NF >= from { n++ } END { print n + 0 }' "$2"
}

now_ms() {
    date +%s%3N
}

note() {
    echo "$*" | tee -a "$results"
}

fail() {
    note "FAILED: $*"
    exit 1
}

# Only for calls that return: a function run in the background leaves its own subshell as $!.
kga() {
    "${py[@]}" "$spike/meta_kglobalaccel.py" "$@"
}

if [[ $mode != retired && $mode != shared ]]; then
    echo "SPIKE_META_MODE must be retired or shared" >&2
    trap - EXIT
    exit 64
fi
if [[ -e $root ]]; then
    echo "$root exists; another run is using it or left it behind" >&2
    trap - EXIT
    exit 1
fi
mkdir -p "$root"/{run,config,data,cache,state} "$out"
chmod 700 "$root/run"
: > "$results"
note "out=$out mode=$mode order=$order"

cp -- "$HOME/.config/kwinrc" "$HOME/.config/kglobalshortcutsrc" "$root/config/"
shortcuts=$root/config/kglobalshortcutsrc
note "plasmashell's launcher entry in the copy: $(grep -a "^$launcher_action=" "$shortcuts")"
if [[ $mode == retired ]]; then
    # The entry is "<keys>,<default keys>,<name>", keys joined by a literal "\t"; only bare Meta leaves the first
    # field.
    awk -v action="$launcher_action" -F ',' -v OFS=',' '
        /^\[/ { in_plasma = ($0 == "[plasmashell]") }
        in_plasma && index($0, action "=") == 1 {
            sub("^" action "=", "", $1)
            n = split($1, keys, /\\t/); kept = ""
            for (i = 1; i <= n; i++) { if (keys[i] != "Meta") { kept = kept (kept == "" ? "" : "\\t") keys[i] } }
            $1 = action "=" (kept == "" ? "none" : kept)
        }
        { print }' "$shortcuts" > "$shortcuts.new"
    mv -- "$shortcuts.new" "$shortcuts"
    note "after removing bare Meta: $(grep -a "^$launcher_action=" "$shortcuts")"
fi

dbus-daemon --config-file="$repo/scripts/private-bus.conf" --nofork --print-address=3 3> "$root/system.addr" &
system_pid=$!
wait_for "$socket_deadline_s" test -s "$root/system.addr" || fail "no private system bus"
system_bus=$(head -n 1 "$root/system.addr")

wayland=$root/run/ghost-sbx-$name
# The fake input switch holds only inside this KWin, for fake_key.py and fake_chord.py.
env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    DBUS_SYSTEM_BUS_ADDRESS="$system_bus" KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 QT_FORCE_STDERR_LOGGING=1 \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width 2560 --height 1440' \
    "$root" "ghost-sbx-$name" > "$out/kwin.log" 2>&1 &
session_pid=$!
wait_for "$socket_deadline_s" test -S "$wayland" || fail "KWin made no socket, see $out/kwin.log"
session_bus=$(< "$root/session.addr")
wait_for "$ready_deadline_s" kga keys "$session_bus" plasmashell "$launcher_action" > /dev/null 2>&1 \
    || fail "KGlobalAccel never answered, see $out/kwin.log"
dbus-monitor --address "$session_bus" > "$out/session-bus.log" 2>&1 &
monitor_pid=$!

stored=$(kga keys "$session_bus" plasmashell "$launcher_action" 2>&1 | grep -a -v -i zoxide)
note "KGlobalAccel's plasmashell/$launcher_action: $(echo "$stored" | head -n 1)"
# The default keys hold bare Meta in both modes. KGlobalAccel stores it as Qt::Key_Meta with no modifier bit.
qt_key_meta=$((0x01000022))
meta_key=$(echo "$stored" | sed -n 's/.*defaults=\[\([0-9, ]*\)\].*/\1/p' | tr ',' '\n' | tr -d ' ' | grep -x "$qt_key_meta" || true)
[[ -n $meta_key ]] || fail "no stored key for bare Meta"
note "bare Meta as KGlobalAccel stores it: $meta_key ($(printf '0x%x' "$meta_key"))"

register_plasmashell() {
    "${py[@]}" "$spike/meta_kglobalaccel.py" register "$session_bus" plasmashell "$launcher_action" "$launcher_friendly" 0 autoload > "$out/plasmashell.log" 2>&1 &
    plasma_pid=$!
    wait_for "$ready_deadline_s" has_line "meta-kga registered" "$out/plasmashell.log" || fail "stand-in plasmashell never registered"
    note "stand-in plasmashell: $(grep -a 'meta-kga registered' "$out/plasmashell.log")"
}
register_ghost_shell() {
    "${py[@]}" "$spike/meta_kglobalaccel.py" register "$session_bus" ghost-shell launcher "Open the launcher" "$meta_key" > "$out/ghost-shell.log" 2>&1 &
    ghost_pid=$!
    wait_for "$ready_deadline_s" has_line "meta-kga registered" "$out/ghost-shell.log" || fail "ghost-shell never registered"
    note "ghost-shell: $(grep -a 'meta-kga registered' "$out/ghost-shell.log")"
}
if [[ $mode == shared && $order == plasmashell-first ]]; then
    register_plasmashell
fi
register_ghost_shell
if [[ $mode == shared && $order == ghost-first ]]; then
    register_plasmashell
fi
note "$(kga bykey "$session_bus" "$meta_key" 2>&1 | grep -a -v -i zoxide | tr '\n' ';')"
note "KGlobalAccel's plasmashell/$launcher_action now: $(kga keys "$session_bus" plasmashell "$launcher_action" 2>&1 | grep -a -v -i zoxide | head -n 1)"
note "KGlobalAccel's ghost-shell/launcher now: $(kga keys "$session_bus" ghost-shell launcher 2>&1 | grep -a -v -i zoxide | head -n 1)"

mkfifo "$root/taps.in" "$root/chords.in"
"${py[@]}" "$spike/../greeting-handoff/fake_key.py" "$wayland" < "$root/taps.in" > "$out/fake-key.log" 2>&1 &
tap_pid=$!
exec 3> "$root/taps.in"
"${py[@]}" "$spike/fake_chord.py" "$wayland" < "$root/chords.in" > "$out/fake-chord.log" 2>&1 &
chord_pid=$!
exec 4> "$root/chords.in"
wait_for "$ready_deadline_s" has_line "fake-key ready" "$out/fake-key.log" || fail "no fake keyboard, see $out/fake-key.log"
wait_for "$ready_deadline_s" has_line "fake-key ready" "$out/fake-chord.log" || fail "no chord keyboard, see $out/fake-chord.log"
sleep "$settle_s"

press_logs=("$out/ghost-shell.log")
if [[ $mode == shared ]]; then
    press_logs+=("$out/plasmashell.log")
fi
pressed() {
    local from=$1 counts=""
    for log in "${press_logs[@]}"; do
        counts+="$(basename "$log" .log) heard: ghost-shell/launcher $(count_from "pressed ghost-shell/launcher" "$log" "$from"), plasmashell/$launcher_action $(count_from "pressed plasmashell/$launcher_action" "$log" "$from"); "
    done
    echo "$counts"
}

taps_from=$(now_ms)
for ((tap = 1; tap <= taps; tap++)); do
    echo "$key_meta" >&3
    sleep "$key_gap_s"
done
sleep "$settle_s"
note "after $taps Meta taps: $(pressed "$taps_from")"
note "tap to press, ms: $(awk -v from="$taps_from" '/fake-key typed/ && $NF >= from { print $NF }' "$out/fake-key.log" | paste -sd ' ')" \
    "| presses at: $(awk -v from="$taps_from" '/meta-kga pressed/ && $NF >= from { print $NF }' "${press_logs[@]}" | paste -sd ' ')"

for chord in "$key_meta $key_s" "$key_meta $key_1" "$key_meta $key_k"; do
    chord_from=$(now_ms)
    echo "$chord" >&4
    sleep "$settle_s"
    krunner=$(awk -v from="$chord_from" '/^method call/ && /destination=org.kde.krunner/ && /member=Activate/ {
        match($0, /time=[0-9.]+/); if (substr($0, RSTART + 5, RLENGTH - 5) * 1000 >= from) { n++ } } END { print n + 0 }' \
        "$out/session-bus.log")
    note "chord ${chord// /+}: $(pressed "$chord_from") Activate calls to org.kde.krunner: $krunner"
done
note "globalShortcutPressed signals on the bus in all: $(grep -ac 'member=globalShortcutPressed' "$out/session-bus.log" || true)"
note "done"

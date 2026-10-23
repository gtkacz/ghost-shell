#!/usr/bin/env bash
set -euo pipefail

# Plan 3a Task 1 in one headless KWin with its own session bus and a second private bus for its system bus:
# the window bridge, Meta on its own, launch focus and popup fades. The user's kwinrc and kglobalshortcutsrc
# are copied into the sandbox and only the copies are edited.
#   SPIKE_OUT            directory for logs, captures and results, kept afterwards (default: a new temp dir)
#   SPIKE_LIVE_EFFECTS   1 also copies ~/.local/share/kwin/effects, the user's own effects, which their kwinrc
#                        turns on; 0 leaves KWin's stock effects as that kwinrc configures them

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
name=3a-t1
root=/tmp/gs-sbx-$name
out=${SPIKE_OUT:-$(mktemp -d -t bar-bridge-spike.XXXXXX)}
results=$out/results.txt
live_effects=${SPIKE_LIVE_EFFECTS:-0}

script_id=bar-bridge-spike
entry_id=bar-bridge-launched
toggle_target=org.ghostshell.Ghostd,/org/ghostshell/Desktop,org.ghostshell.Desktop1,ToggleLauncher
socket_deadline_s=10
ready_deadline_s=30
startup_failures_s=2.5
stub_gone_s=3.5
samples=20
pool=20
idle_s=60
key_gap_s=0.5
launch_wait_s=5
launch_trials=3
fade_reps=3
fade_watch_s=1.5
fade_lead_s=0.3
fade_rest_s=1.2
poll_s=0.1
# Inside every sample surface whether or not Quickshell scales layer-shell margins with QT_SCALE_FACTOR.
fade_area=(500 100 64 64)
key_meta=125
key_s=31
# Unbound in the user's shortcuts with Meta; Meta+L would lock.
key_k=37

# -B: fake_chord.py imports the 2c spike's fake_key.py, which would otherwise leave a bytecode cache in its dir.
# The user's bus addresses are dropped too, so no helper can fall back to their live session.
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)
qml_runtime=/usr/lib64/qt6/bin/qml

system_pid=
session_pid=
monitor_pid=
stub_pid=
keys_pid=
app_a_pid=
app_b_pid=
pool_pid=
shell_pid=
stub_runs=0
command_seq=0

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
    stop shell_pid
    stop pool_pid
    stop app_b_pid
    stop app_a_pid
    stop keys_pid
    stop stub_pid
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

count_lines() {
    grep -ac -- "$1" "$2" 2> /dev/null || true
}

now_ms() {
    date +%s%3N
}

is_private_bus() {
    [[ $1 == unix:* && $1 != *"/run/user/"* ]]
}

note() {
    echo "$*" | tee -a "$results"
}

fail() {
    note "FAILED: $*"
    exit 1
}

kwin_call() {
    dbus-send --bus="$session_bus" --print-reply --dest=org.kde.KWin "$@" 2>&1 | tail -n +2 | tr -s ' \n' ' '
}

drive() {
    "${py[@]}" "$spike/drive.py" "$@" 2>&1 | tee -a "$results"
}

client_env() {
    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
        WAYLAND_DISPLAY="$wayland" QT_QPA_PLATFORM=wayland QT_SCALE_FACTOR=1.15 GHOST_GREETING=off \
        DBUS_SESSION_BUS_ADDRESS="$session_bus" DBUS_SYSTEM_BUS_ADDRESS="$system_bus" "$@"
}

start_stub() {
    stub_runs=$((stub_runs + 1))
    stub_log=$out/stub-$stub_runs.log
    "${py[@]}" "$spike/stub_ghostd.py" "$session_bus" > "$stub_log" 2>&1 &
    stub_pid=$!
    wait_for "$ready_deadline_s" has_line "stub ready" "$stub_log" || fail "stub never ready, see $stub_log"
}

send_command() {
    command_seq=$((command_seq + 1))
    echo "$command_seq $*" > "$root/command"
}

if [[ -e $root ]]; then
    echo "$root exists; another run is using it or left it behind" >&2
    trap - EXIT
    exit 1
fi
mkdir -p "$root"/{run,config,data/kwin/scripts,data/applications,cache,state} "$out"
chmod 700 "$root/run"
: > "$results"
note "out=$out live_effects=$live_effects"

cp -- "$HOME/.config/kwinrc" "$HOME/.config/kglobalshortcutsrc" "$root/config/"
sandbox_config() {
    env -i HOME="$root" PATH=/usr/bin:/bin XDG_CONFIG_HOME="$root/config" kwriteconfig6 --file "$root/config/$1" "${@:2}"
}
sandbox_config kwinrc --group ModifierOnlyShortcuts --key Meta "$toggle_target"
sandbox_config kwinrc --group Plugins --key "${script_id}Enabled" true
cp -r -- "$spike/kwin/$script_id" "$root/data/kwin/scripts/"
if [[ $live_effects == 1 ]]; then
    cp -rL -- "$HOME/.local/share/kwin/effects" "$root/data/kwin/"
fi
cp -- "$root/config/kwinrc" "$out/kwinrc"
note "FocusStealingPreventionLevel in the copied kwinrc: $(grep -a '^FocusStealingPreventionLevel' "$root/config/kwinrc" || echo 'unset, so KWin default 1 (Low)')"
entry_path=$root/data/applications/$entry_id.desktop
cat > "$entry_path" << EOF
[Desktop Entry]
Type=Application
Name=bar-bridge launched
Exec=sh -c "sleep 1; exec $qml_runtime $spike/plain.qml -- bar-bridge-launched 1"
EOF

dbus-daemon --config-file="$repo/scripts/private-bus.conf" --nofork --print-address=3 3> "$root/system.addr" &
system_pid=$!
wait_for "$socket_deadline_s" test -s "$root/system.addr" || fail "no private system bus"
system_bus=$(head -n 1 "$root/system.addr")
is_private_bus "$system_bus" || fail "the private system bus has no unix: address: ${system_bus@Q}"

wayland=$root/run/ghost-sbx-$name
# Both permission switches hold only inside this KWin: ScreenShot2 for grab.py, fake input for fake_chord.py.
# Without QT_FORCE_STDERR_LOGGING, KWin's log, and with it the script's timestamped lines, misses kwin.log.
env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    DBUS_SYSTEM_BUS_ADDRESS="$system_bus" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
    QT_FORCE_STDERR_LOGGING=1 \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width 2560 --height 1440' \
    "$root" "ghost-sbx-$name" > "$out/kwin.log" 2>&1 &
session_pid=$!
wait_for "$socket_deadline_s" test -S "$wayland" || fail "KWin made no socket, see $out/kwin.log"
session_bus=$(< "$root/session.addr")
is_private_bus "$session_bus" || fail "KWin's session bus has no unix: address: ${session_bus@Q}"
dbus-monitor --address "$session_bus" > "$out/session-bus.log" 2>&1 &
monitor_pid=$!
wait_for "$ready_deadline_s" has_line "bar-bridge next failed" "$out/kwin.log" || fail "the script never polled, see $out/kwin.log"
note "script loaded: $(kwin_call /Scripting org.kde.kwin.Scripting.isScriptLoaded "string:$script_id")"
note "loaded effects: $(kwin_call /Effects org.freedesktop.DBus.Properties.Get string:org.kde.kwin.Effects string:loadedEffects)"

note "== Step 1: window list and commands"
sleep "$startup_failures_s"
note "next failed before the stub existed: $(count_lines 'bar-bridge next failed' "$out/kwin.log") times in ${startup_failures_s} s after the first"
start_stub
wait_for "$ready_deadline_s" has_line "stub next called" "$stub_log" || fail "the script never called Next on the stub"
note "first Next reached the stub: $(grep -a -m 1 'stub next called' "$stub_log")"

client_env SPIKE_TITLE=bar-bridge-a SPIKE_COUNT=2 qs --no-color -p "$spike/windows.qml" > "$out/app-a.log" 2>&1 &
app_a_pid=$!
drive wait "$session_bus" 2
client_env "$qml_runtime" "$spike/plain.qml" -- bar-bridge-b 1 > "$out/app-b.log" 2>&1 &
app_b_pid=$!
drive wait "$session_bus" 3
drive order "$session_bus"
drive cycle "$session_bus" "$samples"
client_env "$qml_runtime" "$spike/plain.qml" -- bar-bridge-pool "$pool" > "$out/pool.log" 2>&1 &
pool_pid=$!
drive wait "$session_bus" $((3 + pool))
drive close "$session_bus" bar-bridge-pool
wait "$pool_pid" 2> /dev/null || true
pool_pid=

note "idle for ${idle_s} s"
idle_from=$(now_ms)
sleep "$idle_s"
idle_to=$(now_ms)

note "stopping the stub with a Next pending"
stub_stopped=$(now_ms)
stop stub_pid
sleep "$stub_gone_s"
start_stub
wait_for "$ready_deadline_s" has_line "stub next called" "$stub_log" || fail "the script never came back to the stub"
note "failed() lines from the stop on:"
awk -v from="$stub_stopped" '/bar-bridge next failed/ && $NF >= from { print "  " $NF - from " ms after the stop" }' \
    "$out/kwin.log" | tee -a "$results"
note "Next back on the new stub: $(( $(grep -a -m 1 'stub next called' "$stub_log" | awk '{ print $NF }') - stub_stopped )) ms after the stop"
wait_for "$ready_deadline_s" has_line "stub report " "$stub_log" || fail "the new stub never got the window list"
note "window list back on the new stub: $(( $(grep -a -m 1 'stub report ' "$stub_log" | awk '{ print $NF }') - stub_stopped )) ms after the stop"

note "== Step 2: Meta on its own"
mkfifo "$root/keys.in"
"${py[@]}" "$spike/fake_chord.py" "$wayland" < "$root/keys.in" > "$out/fake-keys.log" 2>&1 &
keys_pid=$!
exec 3> "$root/keys.in"
wait_for "$ready_deadline_s" has_line "fake-key ready" "$out/fake-keys.log" || fail "no fake keyboard, see $out/fake-keys.log"
keys_from=$(now_ms)
for chord in "$key_meta" "$key_meta" "$key_meta" "$key_meta" "$key_meta" "$key_meta $key_s" "$key_meta $key_k"; do
    echo "$chord" >&3
    sleep "$key_gap_s"
done
sleep 1
note "chords typed: $(grep -a 'fake-key typed' "$out/fake-keys.log" | awk '{ printf "%s ", $3 }')"
note "ToggleLauncher calls at the stub: $(count_lines 'stub ToggleLauncher' "$stub_log")"
note "calls to org.kde.krunner on the bus: $(awk -v from="$keys_from" '/^method call/ && /destination=org.kde.krunner/ {
    match($0, /time=[0-9.]+/); if (substr($0, RSTART + 5, RLENGTH - 5) * 1000 >= from) { sub(/.*path=/, ""); print }
}' "$out/session-bus.log" | sort | uniq -c | tr -s ' \n' ' ')"
note "globalShortcutPressed signals on the bus: $(count_lines 'member=globalShortcutPressed' "$out/session-bus.log")"
note "[ModifierOnlyShortcuts] in the copied kwinrc: $(grep -a -A1 '^\[ModifierOnlyShortcuts\]' "$out/kwinrc" | tail -n 1)"
note "plasmashell's KGlobalAccel component active (it holds Meta for the launcher): $(dbus-send --bus="$session_bus" \
    --print-reply --dest=org.kde.kglobalaccel /component/plasmashell org.kde.kglobalaccel.Component.isActive 2>&1 | tail -n 1)"
note "kglobalaccel components: $(dbus-send --bus="$session_bus" --print-reply --dest=org.kde.kglobalaccel /kglobalaccel \
    org.kde.KGlobalAccel.allComponents 2>&1 | grep -ao 'component/[a-zA-Z_0-9]*' | tr '\n' ' ')"

note "== Step 3: launch focus"
client_env SPIKE_COMMAND_FILE="$root/command" SPIKE_ENTRY="$entry_id" SPIKE_ENTRY_PATH="$entry_path" \
    qs --no-color -p "$spike/shell.qml" > "$out/shell.log" 2>&1 &
shell_pid=$!
wait_for "$ready_deadline_s" has_line "spike shell entry found" "$out/shell.log" || fail "shell never found the entry, see $out/shell.log"
for route in launch-execute launch-gtk launch-kio; do
    for ((trial = 1; trial <= launch_trials; trial++)); do
        drive focus "$session_bus" bar-bridge-a-1 > /dev/null
        send_command "$route"
        note "$route trial $trial: $(drive state "$session_bus" bar-bridge-launched-1 "$launch_wait_s" | tail -n 1)"
        drive close "$session_bus" bar-bridge-launched > /dev/null || true
    done
done

note "== Step 4: popup fades"
stop app_a_pid
stop app_b_pid
send_command show none
sleep "$fade_rest_s"
fade_trial() {
    local kind=$1 label=$2 watch=$out/watch-$2.tsv watch_pid
    "${py[@]}" "$spike/../greeting-handoff/grab.py" watch "$session_bus" "$watch" "$fade_watch_s" "${fade_area[@]}" > /dev/null &
    watch_pid=$!
    sleep "$fade_lead_s"
    send_command show "$kind"
    wait "$watch_pid"
    if [[ $label == *-1 ]]; then
        "${py[@]}" "$spike/../greeting-handoff/grab.py" shot "$session_bus" "$out/shot-$kind.png" > /dev/null
    fi
    send_command show none
    sleep "$fade_rest_s"
    note "$label: $(awk -F '\t' 'NR > 1 {
        n++; if (NR == 2) { first = $1 } last = $1
        if (!started && $2 > 6) { started = $1; from = $2 }
        if (started && !full && $2 >= 250) { full = $1 }
        if (started && !full && $2 > 6) { between++ }
    } END {
        printf "%d captures, %.0f/s; first non-black r=%.1f, %d captures before full red over %d ms",
            n, n * 1000 / (last - first), from, between, full - started
    }' "$watch")"
}
for kind in popup dock utility; do
    for ((rep = 1; rep <= fade_reps; rep++)); do
        fade_trial "$kind" "$kind-$rep"
    done
done
# The control: the same popup once more with the effects that fade popups unloaded, in this KWin only.
for effect in fadingpopups kinetic_fadingpopups; do
    note "unload $effect: $(kwin_call /Effects org.kde.kwin.Effects.unloadEffect "string:$effect")"
done
fade_trial popup popup-unloaded
stop shell_pid

note "== Figures"
drive latency "$out/stub-1.log" "$out/kwin.log"
drive traffic "$out/session-bus.log" "$idle_from" "$idle_to"
note "done"

#!/usr/bin/env bash
set -euo pipefail

# Lock plan A Task 5: the hand-off from the gate's lock screen to the greeting overlay, measured as
# spikes/greeting-handoff/run.sh measured the stock lock screen. A headless KWin with its screen locker on and
# PLASMA_DEFAULT_SHELL=org.ghostshell.lock in its own environment (the route --lock installs) locks over a red
# stand-in window while ghost-shell's real shell, on a fake ghostd, arms its greeting overlay. The gate is unlocked
# through a FakeAdapter ("ghost"), plays its pass and then waits; the fake logind's Unlock then ends the greeter, the
# way a password's Qt.quit() would but for the exit code. No password reaches PAM.
#
# Captures: the overlay's resting frame (armed while unlocked, before the lock), the gate's last frame (after the
# pass, under the lock), and the centre of the screen as fast as KWin answers around the unlock.
#   SPIKE_OUT           directory for logs, captures and results, kept afterwards (default: a new temp dir)
#   SPIKE_LIVE_EFFECTS  1 copies the user's KWin effect set into the sandbox, as the greeting hand-off spike does

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
handoff=$repo/spikes/greeting-handoff
name=lock-handoff
root=/tmp/gs-sbx-$name
out=${SPIKE_OUT:-$(mktemp -d -t lock-handoff.XXXXXX)}
results=$out/results.txt
package=org.ghostshell.lock
socket=$root/ghostd.sock

socket_deadline_s=10
ready_deadline_s=30
settle_s=2
watch_lead_s=1
watch_s=4
poll_s=0.1
watch_area=(1248 688 64 64)
# evdev: g h o s t, Enter.
fake_password_keys=(34 35 24 31 20 28)
# The gate's scanlines at 2560 px wide: Theme.d(3) and Theme.d(1) at 2 px per design px.
scan_period=6
scan_width=2

py=(uv run --no-project --with dbus-next python -I)
no_session_bus=unix:path=/nonexistent

pids=()
session_pid=

note() {
    echo "$*" | tee -a "$results"
}

fail() {
    note "FAILED: $*"
    exit 1
}

cleanup() {
    exec 3>&- 2> /dev/null || true
    local pid
    for pid in "${pids[@]}"; do
        kill "$pid" 2> /dev/null || true
    done
    if [[ -s $root/kwin.pid ]]; then
        kill "$(< "$root/kwin.pid")" 2> /dev/null || true
    fi
    if [[ -n $session_pid ]]; then
        wait "$session_pid" 2> /dev/null || true
    fi
    for pid in "${pids[@]}"; do
        wait "$pid" 2> /dev/null || true
    done
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

has_count() { # <pattern> <file> <count>
    (($(count_lines "$1" "$2") >= $3))
}

active_changed_ms() {
    awk -v want="boolean $1" '
        /member=ActiveChanged/ { match($0, /time=[0-9.]+/); at = substr($0, RSTART + 5, RLENGTH - 5); next }
        at != "" && index($0, want) { printf "%d\n", at * 1000; exit }
        { at = "" }' "$out/session-bus.log"
}

has_active_changed() {
    [[ -n $(active_changed_ms "$1") ]]
}

grab() {
    "${py[@]}" "$handoff/grab.py" shot "$session_bus" "$out/$1.png" | tee -a "$results"
}

# A session value in ghostd's wire form: locked $1, greeting_seq $2, greeting $3, as ghostd would publish on the lock
# and unlock it reads from KWin's ScreenSaver.
send_session() {
    XDG_RUNTIME_DIR="$root/run" qs ipc --pid "$shell_pid" call greeting session \
        "{\"id\":\"0412\",\"locked\":$1,\"sleeping\":false,\"greeting\":$3,\"greeting_seq\":$2}" > /dev/null \
        || fail "qs ipc call greeting session failed; see $out/shell.log"
}

if [[ -e $root ]]; then
    echo "$root exists; another run is using it or left it behind" >&2
    trap - EXIT
    exit 1
fi
mkdir -p "$root"/{run,config,data,cache,state} "$out"
chmod 700 "$root/run"
: > "$results"
live_effects=${SPIKE_LIVE_EFFECTS:-0}
note "out=$out live_effects=$live_effects"
if [[ $live_effects == 1 ]]; then
    awk '/^\[/ { keep = ($0 == "[Plugins]" || $0 ~ /^\[Effect-/) } keep' "$HOME/.config/kwinrc" > "$root/config/kwinrc"
    mkdir -p "$root/data/kwin"
    cp -rL -- "$HOME/.local/share/kwin/effects" "$HOME/.local/share/kwin/scripts" "$root/data/kwin/"
fi

# The package exactly as --lock installs it, then swapped onto the fake adapter, whose done only logs: the greeter
# stays up on the gate's last frame until the logind Unlock below ends it.
env -u DBUS_SESSION_BUS_ADDRESS HOME="$root" XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
    XDG_STATE_HOME="$root/state" XDG_RUNTIME_DIR="$root/run" "$repo/scripts/install.sh" --lock > "$out/install.log" 2>&1 \
    || fail "install.sh --lock failed; see $out/install.log"
lock_qml=$root/data/plasma/shells/$package/contents/lockscreen/LockScreen.qml
sed -i -e 's/adapter: LockAdapter {/adapter: FakeAdapter {/' -e '/driving: root.primary/d' \
    -e 's/onDone: adapter.finish()/onDone: console.info("gate: done at " + Date.now())/' "$lock_qml"
grep -q 'FakeAdapter {' "$lock_qml" && grep -q 'gate: done at' "$lock_qml" || fail "could not swap the adapter in $lock_qml"

dbus-daemon --config-file="$repo/scripts/private-bus.conf" --nofork --print-address=3 3> "$root/system.addr" &
pids+=($!)
wait_for "$socket_deadline_s" test -s "$root/system.addr" || fail "no private system bus"
system_bus=$(head -n 1 "$root/system.addr")
"${py[@]}" "$handoff/fake_logind.py" "$system_bus" > "$out/fake-logind.log" 2>&1 &
logind_pid=$!
pids+=("$logind_pid")
wait_for "$ready_deadline_s" has_line "fake-logind ready" "$out/fake-logind.log" || fail "fake logind never ready"

wayland=$root/run/ghost-sbx-$name
# The screen locker stays on; its greeter inherits KWin's environment, PLASMA_DEFAULT_SHELL and stderr logging
# included, so the gate's lines land in kwin.log.
env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    DBUS_SYSTEM_BUS_ADDRESS="$system_bus" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
    PLASMA_DEFAULT_SHELL="$package" QT_FORCE_STDERR_LOGGING=1 \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --socket "$1" --width 2560 --height 1440' \
    "$root" "ghost-sbx-$name" > "$out/kwin.log" 2>&1 &
session_pid=$!
wait_for "$socket_deadline_s" test -S "$wayland" || fail "KWin made no socket, see $out/kwin.log"
session_bus=$(< "$root/session.addr")
dbus-monitor --address "$session_bus" > "$out/session-bus.log" 2>&1 &
pids+=($!)

mkfifo "$root/keys.in"
"${py[@]}" "$handoff/fake_key.py" "$wayland" < "$root/keys.in" > "$out/fake-key.log" 2>&1 &
pids+=($!)
exec 3> "$root/keys.in"
wait_for "$ready_deadline_s" has_line "fake-key ready" "$out/fake-key.log" || fail "no fake keyboard, see $out/fake-key.log"

qs_env=(env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config"
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state"
    WAYLAND_DISPLAY="$wayland" QT_QPA_PLATFORM=wayland)
"${qs_env[@]}" DBUS_SESSION_BUS_ADDRESS="$session_bus" qs --no-color -p "$handoff/window.qml" > "$out/red.log" 2>&1 &
pids+=($!)
wait_for "$ready_deadline_s" has_line "spike red active true" "$out/red.log" || fail "red window never active"

cargo build -q -p ghostd --manifest-path "$repo/Cargo.toml"
"$repo/target/debug/ghostd" default-config > "$root/config.toml"
DBUS_SESSION_BUS_ADDRESS=$no_session_bus "$repo/target/debug/ghostd" run --fake --socket "$socket" \
    --config "$root/config.toml" --widgets-dir "$repo/widgets" > "$out/ghostd.log" 2>&1 &
pids+=($!)
# The login marker keeps the shell from greeting at its start; only the lock and unlock below drive it.
echo greeted > "$root/run/ghost-shell-greeted"
mkdir -p "$root/pipewire"
"${qs_env[@]}" DBUS_SESSION_BUS_ADDRESS="$no_session_bus" GHOST_GREETING=auto GHOST_DEBUG=1 GHOST_SOCKET="$socket" \
    QML_IMPORT_PATH="$repo/shell/kit" PIPEWIRE_REMOTE=/nonexistent PIPEWIRE_RUNTIME_DIR="$root/pipewire" \
    qs --no-color -p "$repo/shell" > "$out/shell.log" 2>&1 &
shell_pid=$!
pids+=("$shell_pid")
wait_for "$ready_deadline_s" has_line "qml: ghostd connected" "$out/shell.log" || fail "the shell never reached ghostd"
sleep "$settle_s"
send_session false 0 null

# The resting frame, over the red window, while nothing is locked.
send_session true 0 null
wait_for "$ready_deadline_s" has_count "qml: greeting first frame at" "$out/shell.log" 1 || fail "the overlay never armed"
sleep "$settle_s"
grab 1-overlay-resting
send_session false 0 null
wait_for "$ready_deadline_s" has_line "qml: greeting disarmed" "$out/shell.log" || fail "the overlay never disarmed"
sleep "$settle_s"
grab 2-before-lock

dbus-send --bus="$session_bus" --print-reply --dest=org.freedesktop.ScreenSaver /ScreenSaver \
    org.freedesktop.ScreenSaver.Lock > "$out/lock-reply.log" 2>&1 &
wait_for "$ready_deadline_s" has_active_changed true || fail "no ActiveChanged(true), see $out/kwin.log"
note "locked: ActiveChanged(true) at $(active_changed_ms true)"
send_session true 0 null
wait_for "$ready_deadline_s" has_count "qml: greeting first frame at" "$out/shell.log" 2 || fail "the overlay never armed under the lock"
wait_for "$ready_deadline_s" has_line "gate: prompt lock" "$out/kwin.log" || fail "the gate never loaded, see $out/kwin.log"
sleep "$settle_s"
grab 3-locked-prompt

for key in "${fake_password_keys[@]}"; do
    echo "$key" >&3
done
wait_for "$ready_deadline_s" has_line "gate: done at" "$out/kwin.log" || fail "the gate never finished its pass, see $out/kwin.log"
note "gate: $(grep -a 'gate: succeeded' "$out/kwin.log" | head -n 1); $(grep -a 'gate: done at' "$out/kwin.log" | head -n 1)"
sleep "$settle_s"
grab 4-gate-last

"${py[@]}" "$handoff/grab.py" watch "$session_bus" "$out/watch.tsv" "$watch_s" "${watch_area[@]}" >> "$results" 2>&1 &
watch_pid=$!
pids+=("$watch_pid")
sleep "$watch_lead_s"
kill -USR1 "$logind_pid"
wait_for "$ready_deadline_s" has_active_changed false || fail "no ActiveChanged(false), see $out/kwin.log"
unlocked_ms=$(active_changed_ms false)
note "unlocked: ActiveChanged(false) at $unlocked_ms"
send_session false 1 '"unlock"'
wait "$watch_pid" || true
grab 5-after-unlock

note "fake logind: $(grep -a 'Unlock' "$out/fake-logind.log" | tail -n 1)"
note "greeting: $(grep -aE 'qml: greeting (playing|first frame)' "$out/shell.log" | tail -n 2 | paste -sd ';')"
note "watch, class changes:"
awk 'NR > 1 && $5 != last { print "  " $1, $2, $3, $4, $5; last = $5 }' "$out/watch.tsv" | tee -a "$results"
# The gate's frame is the reference: the first capture that differs from it by more than a rounding step is the next
# scene, and any red above the ground's 10 is the window showing through.
note "watch, from the unlock on: $(awk -v after="$unlocked_ms" '
    NR == 2 { r0 = $2; g0 = $3; b0 = $4 }
    NR > 1 && $1 >= after {
        n++
        d = ($2 - r0)^2 + ($3 - g0)^2 + ($4 - b0)^2
        if (!changed && d > 1) { changed = 1; first = $1 - after " ms: " $2 "," $3 "," $4 }
        if ($2 > peak) { peak = $2 }
        if ($2 + $3 + $4 < dark || !dark) { dark = $2 + $3 + $4 }
    }
    END { print n " captures; reference " r0 "," g0 "," b0 "; first different at +" first "; peak red " peak "; darkest r+g+b " dark }' "$out/watch.tsv")"
note "compare, gate's last frame vs overlay's resting frame:"
"${py[@]}" "$spike/compare.py" "$out/4-gate-last.png" "$out/1-overlay-resting.png" "$scan_period" "$scan_width" | tee -a "$results"

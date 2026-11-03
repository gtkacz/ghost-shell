#!/usr/bin/env bash
set -euo pipefail

# Installs the switcher into a headless KWin's sandbox with `install.sh --switcher` and drives it with real key events, as
# Alt+Tab is on the desktop: a flick draws nothing, a hold opens it, and Tab, Shift+Tab, Esc and a window closing while it
# is open all leave the expected window active. Then it opens with no snapshot, with a garbled one and with a changed
# colour, and `--remove-switcher` must put [TabBox] back as it began. Everything runs under env -i with a KWin and bus of its
# own, so nothing reaches the user's screen. Headless says nothing about NVIDIA: the first opening there is the user's.

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
name=smokeswitcher
root=/tmp/gs-sbx-$name
wayland=ghost-sbx-$name
kwin_log=$root/kwin.log
windows_log=$root/windows.log
kwinrc=$root/config/kwinrc
snapshot=$root/state/ghost-shell/gate.ini
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)
fake_keys=$repo/scripts/fake-keys.py
grab=$repo/spikes/greeting-handoff/grab.py

# evdev codes.
alt=56
tab=15
shift_key=42
esc=1

# Four, so that three steps of the focus chain land on another window instead of wrapping back to the active one.
stand_ins=(a b c d)
flicks=10
budget_s=20
kwin_stop_s=5
poll_s=0.1
# Time for a key to have had its effect, when the check is that nothing changed.
settle_s=0.4
# Fewer pixels than the selected row's border draws at full strength.
min_signal_pixels=100
first_signal='#ff00ff'
second_signal='#00ffff'

opened_line='switcher: open'
warning_pattern='\.(qml|mjs):[0-9]+|ReferenceError|TypeError|Component failed|outdated|falling back|Failed to load'

session_pid=
keys_pid=
declare -A window_pids=()
# The focus chain KWin walks, the active window first: Alt+Tab's nth step selects its nth entry.
chain=()
# Set by a failure, so the logs it names outlive the run.
keep_root=

fail() {
    keep_root=1
    echo "smoke-switcher: FAIL ($1)" >&2
    exit 1
}

stop() {
    if [[ -n $1 ]]; then
        kill "$1" 2> /dev/null || true
        wait "$1" 2> /dev/null || true
    fi
}

cleanup() {
    local pid kwin_pid deadline
    exec 3>&- 2> /dev/null || true
    stop "$keys_pid"
    for pid in "${window_pids[@]}"; do
        stop "$pid"
    done
    if [[ -s $root/kwin.pid ]]; then
        # The KWin is not a child of this script, so it is polled; it writes to its sandbox until it has gone.
        kwin_pid=$(< "$root/kwin.pid")
        kill "$kwin_pid" 2> /dev/null || true
        deadline=$((SECONDS + kwin_stop_s))
        while kill -0 "$kwin_pid" 2> /dev/null && (( SECONDS < deadline )); do
            sleep "$poll_s"
        done
    fi
    stop "$session_pid"
    if [[ -n $keep_root ]]; then
        echo "smoke-switcher: $root is kept for its logs; remove it before the next run" >&2
        return
    fi
    rm -rf -- "$root"
}

wait_for() {
    local deadline=$((SECONDS + budget_s))
    until "$@"; do
        if (( SECONDS >= deadline )); then
            return 1
        fi
        sleep "$poll_s"
    done
}

has_line() { # <fixed string> <file>
    grep -aqF -- "$1" "$2" 2> /dev/null
}

# grep -c prints 0 and exits 1 when nothing matches, which is a valid count here.
openings() {
    grep -acF -- "$opened_line" "$kwin_log" || true
}

more_openings_than() {
    (( $(openings) > $1 ))
}

active_window() {
    sed -n 's/.*smoke-window \([a-z]\) active true.*/\1/p' "$windows_log" | tail -n 1
}

is_active() {
    [[ $(active_window) == "$1" ]]
}

keys() {
    printf '%s\n' "$@" >&3
}

# Holds Alt with one Tab and waits for the panel's first frame.
open_switcher() {
    local before
    before=$(openings)
    keys "+$alt" "$tab"
    wait_for more_openings_than "$before" || fail "$1 did not open: no \"$opened_line\" in $kwin_log"
    (( $(openings) == before + 1 )) || fail "$1 opened more than once"
}

# After a release: the chain's entry that many steps along must be active, and moves to the front.
expect_steps() { # <steps> <what ran>
    local index=$(($1 % ${#chain[@]})) wanted
    wanted=${chain[index]}
    sleep "$settle_s"
    wait_for is_active "$wanted" || fail "$2 left $(active_window) active, expected $wanted"
    chain=("$wanted" "${chain[@]:0:index}" "${chain[@]:index+1}")
}

signal_pixels() { # <colour>
    "${py[@]}" "$grab" shot "$session_bus" "$root/shot.png" > /dev/null
    magick "$root/shot.png" -alpha off -fill black +opaque "$1" -fill white -opaque "$1" -format '%[fx:int(mean*w*h)]' info:
}

shows_signal() {
    (( $(signal_pixels "$1") >= min_signal_pixels ))
}

write_signal() {
    printf '[colors]\nsignal="%s"\n' "$1" > "$snapshot"
}

# Only this group: the running KWin writes others to kwinrc itself, and the byte-for-byte check is in test_install_switcher.py.
tabbox_group() {
    awk '/^\[/ { inside = ($0 == "[TabBox]") } inside' "$1"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ -e $root ]]; then
    echo "smoke-switcher: FAIL ($root exists: another run is using it, or a stopped one left it behind)" >&2
    trap - EXIT
    exit 1
fi
mkdir -p "$root"/{run,config,data,cache,state/ghost-shell}
chmod 700 "$root/run"
: > "$windows_log"
sandbox=(env -i "HOME=$root" PATH=/usr/bin:/bin "XDG_RUNTIME_DIR=$root/run" "XDG_CONFIG_HOME=$root/config"
    "XDG_DATA_HOME=$root/data" "XDG_CACHE_HOME=$root/cache" "XDG_STATE_HOME=$root/state")

"${sandbox[@]}" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 QT_FORCE_STDERR_LOGGING=1 \
    QT_LOGGING_RULES='qml.debug=true' \
    dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
    sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width 2560 --height 1440' \
    "$root" "$wayland" > "$kwin_log" 2>&1 &
session_pid=$!
wait_for test -S "$root/run/$wayland" || fail "KWin made no socket; see $kwin_log"
wait_for test -s "$root/session.addr" || fail "KWin wrote no bus address"
session_bus=$(< "$root/session.addr")
in_session=("${sandbox[@]}" "WAYLAND_DISPLAY=$wayland" "DBUS_SESSION_BUS_ADDRESS=$session_bus")

mkfifo "$root/keys.in"
"${py[@]}" "$fake_keys" "$root/run/$wayland" < "$root/keys.in" > "$root/keys.log" 2>&1 &
keys_pid=$!
exec 3> "$root/keys.in"
wait_for has_line 'fake-key ready' "$root/keys.log" || fail "no fake keyboard; see $root/keys.log"

for window in "${stand_ins[@]}"; do
    "${in_session[@]}" QT_QPA_PLATFORM=wayland QT_LOGGING_RULES='qml.info=true' SMOKE_WINDOW="$window" SMOKE_COLOUR='#203040' \
        qs --no-color -p "$repo/scripts/smoke-window.qml" >> "$windows_log" 2>&1 &
    window_pids[$window]=$!
    wait_for is_active "$window" || fail "stand-in $window never became active; see $windows_log"
    chain=("$window" "${chain[@]}")
done

cp -- "$kwinrc" "$root/kwinrc.before"
write_signal "$first_signal"
"${in_session[@]}" "$repo/scripts/install.sh" --switcher > "$root/install.log" 2>&1 \
    || fail "install.sh --switcher failed; see $root/install.log"
grep -q '^warning:' "$root/install.log" && fail "install.sh --switcher could not reach the sandbox's KWin; see $root/install.log"
sleep "$settle_s"

for ((i = 0; i < flicks; i++)); do
    keys "+$alt" "$tab" "-$alt"
    expect_steps 1 "flick $i"
done
(( $(openings) == 0 )) || fail "$flicks flicks opened the switcher $(openings) times"
echo "smoke-switcher: flick"

open_switcher hold
keys "-$alt"
expect_steps 1 hold
echo "smoke-switcher: hold"

open_switcher step
keys "$tab" "$tab"
sleep "$settle_s"
keys "-$alt"
expect_steps 3 step
echo "smoke-switcher: step"

open_switcher back
keys "+$shift_key" "$tab" "-$shift_key"
sleep "$settle_s"
keys "-$alt"
expect_steps 0 back
echo "smoke-switcher: back"

open_switcher cancel
keys "$esc"
sleep "$settle_s"
keys "-$alt"
expect_steps 0 cancel
echo "smoke-switcher: cancel"

# The selected window goes, so its row and the preview's source both go while the panel is on screen.
doomed=${chain[1]}
open_switcher closing
stop "${window_pids[$doomed]}"
unset "window_pids[$doomed]"
sleep "$settle_s"
keys "-$alt"
sleep "$settle_s"
survivor=$(active_window)
[[ $survivor != "$doomed" ]] || fail "the closed window $doomed is still the active one"
[[ -n ${window_pids[$survivor]:-} ]] || fail "no live window is active after $doomed closed"
remaining=()
for window in "${chain[@]}"; do
    if [[ $window != "$doomed" && $window != "$survivor" ]]; then
        remaining+=("$window")
    fi
done
chain=("$survivor" "${remaining[@]}")
echo "smoke-switcher: closing"

rm -- "$snapshot"
open_switcher "no snapshot"
keys "-$alt"
expect_steps 1 "no snapshot"
echo "smoke-switcher: no snapshot"

head -c 512 /dev/urandom > "$snapshot"
open_switcher "garbled snapshot"
keys "-$alt"
expect_steps 1 "garbled snapshot"
echo "smoke-switcher: garbled snapshot"

write_signal "$first_signal"
open_switcher "first colour"
wait_for shows_signal "$first_signal" || fail "the panel never showed $first_signal; see $root/shot.png"
keys "-$alt"
expect_steps 1 "first colour"
write_signal "$second_signal"
open_switcher "new colour"
wait_for shows_signal "$second_signal" || fail "the panel never showed $second_signal; see $root/shot.png"
(( $(signal_pixels "$first_signal") == 0 )) || fail "the old colour $first_signal is still on screen; see $root/shot.png"
keys "-$alt"
expect_steps 1 "new colour"
echo "smoke-switcher: new colour"

# Nothing left to switch to. The checks below cover this too: KWin must survive it, with no warning.
for window in "${!window_pids[@]}"; do
    stop "${window_pids[$window]}"
    unset "window_pids[$window]"
done
sleep "$settle_s"
before_empty=$(openings)
keys "+$alt" "$tab"
sleep "$settle_s"
keys "-$alt"
sleep "$settle_s"
echo "smoke-switcher: no windows, opened $(($(openings) - before_empty)) times"

kill -0 "$(< "$root/kwin.pid")" 2> /dev/null || fail "KWin died; see $kwin_log"
warnings=$(grep -aE -- "$warning_pattern" "$kwin_log" || true)
[[ -z $warnings ]] || fail "warnings in $kwin_log:"$'\n'"$warnings"

"${in_session[@]}" "$repo/scripts/install.sh" --remove-switcher > "$root/remove.log" 2>&1 \
    || fail "install.sh --remove-switcher failed; see $root/remove.log"
[[ $(tabbox_group "$kwinrc") == "$(tabbox_group "$root/kwinrc.before")" ]] \
    || fail "--remove-switcher did not restore [TabBox] in $kwinrc; compare $root/kwinrc.before"
[[ ! -e $root/data/kwin/tabbox/org.ghostshell.switcher ]] || fail "--remove-switcher left the package behind"

echo "smoke-switcher: PASS"

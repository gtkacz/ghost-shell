#!/usr/bin/env bash
set -euo pipefail

# Plan 3c Task 1: the proofs in spec §6, in a headless KWin with the layout route the design rests on. Nothing runs on
# the user's display or reaches the user's buses.
#
# usage: run.sh proofs | effects | timing | broken-load | broken-open
#   proofs       proofs 1, 2, 4, 5, 6, 7 and 9
#   effects      proof 3: how long the panel takes to arrive and to go, with still stand-ins (SPIKE_STATIC=1)
#   timing       key press to first changed capture, first opening and later ones
#   broken-*     proof 8: a layout with an unknown type, or one that throws as its window completes
#   SPIKE_OUT           directory for logs and captures, kept afterwards (default: a new temp dir)
#   SPIKE_LIVE_EFFECTS  1 copies the user's KWin effects and scripts into the sandbox, as lock-handoff does
#   SPIKE_LAYOUT        a stock layout's id for `timing`, in place of the spike's

mode=${1:?usage: run.sh proofs|effects|timing|broken-load|broken-open}
spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
handoff=$repo/spikes/greeting-handoff
name=3c-t1
root=/tmp/gs-sbx-$name
out=${SPIKE_OUT:-$(mktemp -d -t switcher-spike.XXXXXX)}
results=$out/results.txt
layout_id=org.ghostshell.switcher-spike
layout=$root/data/kwin/tabbox/$layout_id
wayland=$root/run/ghost-sbx-$name
kwin_log=$out/kwin.log

alt=56
tab=15
ready_deadline_s=30
socket_deadline_s=10
poll_s=0.1
settle_s=2
hold_settle_s=0.6
between_flicks_s=0.4
first_flicks=5
more_flicks=15
later_openings=3
# Inside the panel, clear of its header and the thumbnail, so only the panel's own arrival changes it.
panel_spot=(500 700 64 64)
preview_spot=(1500 600 200 150)
bar_spot=(100 10 64 30)
centre_spot=(1248 688 64 64)
first_signal='#112233'
second_signal='#445566'
warning_pattern='\.(qml|mjs):[0-9]+|ReferenceError|TypeError|not installed|is not a type|Failed to|Could not'

py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)

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

keys() {
    printf '%s\n' "$@" >&3
}

flick() {
    keys "+$alt" "$tab" "-$alt"
}

kwin_alive() {
    [[ -S $wayland ]] && kill -0 "$(< "$root/kwin.pid")" 2> /dev/null
}

grab() {
    "${py[@]}" "$handoff/grab.py" shot "$session_bus" "$out/$1.png" | tee -a "$results"
}

watch_area() { # <table name> <seconds> <x y w h>
    "${py[@]}" "$handoff/grab.py" watch "$session_bus" "$out/$1.tsv" "$2" "${@:3}" >> "$results" 2>&1
}

active_lines() {
    cat "$out/window-a.log" "$out/window-b.log" | grep -ac 'active true' || true
}

start_session() {
    if [[ -e $root ]]; then
        echo "$root exists; another run is using it or left it behind" >&2
        trap - EXIT
        exit 1
    fi
    mkdir -p "$root"/{run,config,data,cache,state/ghost-shell} "$out"
    chmod 700 "$root/run"
    : > "$results"
    note "mode=$mode out=$out live_effects=${SPIKE_LIVE_EFFECTS:-0} layout=${SPIKE_LAYOUT:-$layout_id}"

    if [[ ${SPIKE_LIVE_EFFECTS:-0} == 1 ]]; then
        awk '/^\[/ { keep = ($0 == "[Plugins]" || $0 ~ /^\[Effect-/) } keep' "$HOME/.config/kwinrc" > "$root/config/kwinrc"
        mkdir -p "$root/data/kwin"
        cp -rL -- "$HOME/.local/share/kwin/effects" "$HOME/.local/share/kwin/scripts" "$root/data/kwin/"
    fi
    printf '\n[TabBox]\nHighlightWindows=false\nLayoutName=%s\n' "${SPIKE_LAYOUT:-$layout_id}" >> "$root/config/kwinrc"
    printf '[colors]\nsignal="%s"\n' "$first_signal" > "$root/state/ghost-shell/gate.ini"

    mkdir -p "$layout"
    cp -r -- "$spike/layout/." "$layout/"
    "$repo/scripts/assemble-gate.sh" "$layout/contents/ui"
    sed -i 's/SPIKE_MARKER/first/' "$layout/contents/ui/main.qml"
    case $mode in
        broken-load)
            sed -i 's/^    Instantiator {/    NoSuchType {}\n\n    Instantiator {/' "$layout/contents/ui/main.qml"
            ;;
        broken-open)
            sed -i 's/^                Theme.load(GateLib/                throw new Error("spike: broken on open");\n                Theme.load(GateLib/' "$layout/contents/ui/main.qml"
            ;;
    esac

    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
        KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 QT_FORCE_STDERR_LOGGING=1 \
        QT_LOGGING_RULES='kwin_tabbox.debug=true;js.info=true;qml.info=true' \
        dbus-run-session --config-file="$repo/scripts/private-bus.conf" -- \
        sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width 2560 --height 1440' \
        "$root" "ghost-sbx-$name" > "$kwin_log" 2>&1 &
    session_pid=$!
    wait_for "$socket_deadline_s" test -S "$wayland" || fail "KWin made no socket, see $kwin_log"
    wait_for "$socket_deadline_s" test -s "$root/session.addr" || fail "KWin wrote no bus address"
    session_bus=$(< "$root/session.addr")

    mkfifo "$root/keys.in"
    "${py[@]}" "$repo/scripts/fake-keys.py" "$wayland" < "$root/keys.in" > "$out/fake-keys.log" 2>&1 &
    pids+=($!)
    exec 3> "$root/keys.in"
    wait_for "$ready_deadline_s" has_line "fake-key ready" "$out/fake-keys.log" || fail "no fake keyboard, see $out/fake-keys.log"

    qs_env=(env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config"
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state"
        WAYLAND_DISPLAY="$(basename "$wayland")" QT_QPA_PLATFORM=wayland DBUS_SESSION_BUS_ADDRESS="$session_bus"
        QT_LOGGING_RULES='qml.info=true;js.info=true')
    "${qs_env[@]}" qs --no-color -p "$spike/bar.qml" > "$out/bar.log" 2>&1 &
    pids+=($!)
    wait_for "$ready_deadline_s" has_line "spike bar ready" "$out/bar.log" || fail "no stand-in bar, see $out/bar.log"
    "${qs_env[@]}" SPIKE_NAME=a SPIKE_COLOURS="${SPIKE_COLOURS_A:-#ff0000,#0000ff}" SPIKE_STATIC="${SPIKE_STATIC:-0}" \
        qs --no-color -p "$spike/window.qml" > "$out/window-a.log" 2>&1 &
    pids+=($!)
    wait_for "$ready_deadline_s" has_line "spike a active true" "$out/window-a.log" || fail "stand-in a never active, see $out/window-a.log"
    "${qs_env[@]}" SPIKE_NAME=b SPIKE_COLOURS="${SPIKE_COLOURS_B:-#ffff00,#ff00ff}" SPIKE_STATIC="${SPIKE_STATIC:-0}" \
        qs --no-color -p "$spike/window.qml" > "$out/window-b.log" 2>&1 &
    pids+=($!)
    wait_for "$ready_deadline_s" has_line "spike b active true" "$out/window-b.log" || fail "stand-in b never active, see $out/window-b.log"
    sleep "$settle_s"
}

# Prints the first capture's colour, how many captures followed, and the times of change after the baseline.
summarise_watch() { # <table name>
    awk -F'\t' 'NR == 2 { r0 = $2; g0 = $3; b0 = $4; t0 = $1; print "  first " r0 "," g0 "," b0 " at " t0; last = r0 "," g0 "," b0 }
        NR > 2 { now = $2 "," $3 "," $4; if (now != last) { print "  +" $1 - t0 " ms " now; last = now } }
        END { print "  " NR - 1 " captures" }' "$out/$1.tsv" | tee -a "$results"
}

proofs() {
    local before after opens
    note "--- proof 9: nothing loads before the first hold"
    has_line "spike-layout: root" "$kwin_log" && note "proof 9: FAIL, the layout loaded at KWin's start" || note "proof 9: not loaded after KWin's start"

    note "--- proof 2: a flick draws nothing"
    before=$(active_lines)
    for ((i = 0; i < first_flicks; i++)); do
        flick
        sleep "$between_flicks_s"
    done
    has_line "spike-layout: root" "$kwin_log" && note "proof 9: FAIL, a flick loaded the layout" || note "proof 9: not loaded after $first_flicks flicks"
    for ((i = 0; i < more_flicks; i++)); do
        flick
        sleep "$between_flicks_s"
    done
    after=$(active_lines)
    opens=$(count_lines "spike-layout: open" "$kwin_log")
    note "proof 2: $((first_flicks + more_flicks)) flicks, $opens openings, $((after - before)) activations"
    awk '/typed \+56/ { press = $NF } /typed -56/ && press { gap = $NF - press; if (gap > worst) worst = gap; press = 0 } END { print "proof 2: longest press to release " worst " ms" }' "$out/fake-keys.log" | tee -a "$results"

    note "--- proof 1: opens over the bar, live; proof 2's control; proof 4's first reading"
    watch_area bar-before 0.5 "${bar_spot[@]}"
    summarise_watch bar-before
    keys "+$alt" "$tab"
    wait_for "$ready_deadline_s" has_line "spike-layout: open" "$kwin_log" || fail "held 30 s and no opening, see $kwin_log"
    note "proof 9: $(grep -a 'spike-layout: root' "$kwin_log" | head -n 1)"
    note "proof 2 control: $(grep -a 'spike-layout: open' "$kwin_log" | head -n 1)"
    sleep "$hold_settle_s"
    grab 1-open
    watch_area bar-open 0.5 "${bar_spot[@]}"
    summarise_watch bar-open
    watch_area preview 3 "${preview_spot[@]}"
    summarise_watch preview
    grab 2-open-later
    keys "-$alt"
    sleep "$settle_s"
    note "proof 4 first: $(grep -a 'spike-layout: signal' "$kwin_log" | tail -n 1)"

    note "--- proof 6: rows"
    grep -a 'spike-layout: row' "$kwin_log" | tee -a "$results"

    note "--- proof 4: the snapshot read again on the next opening"
    printf '[colors]\nsignal="%s"\n' "$second_signal" > "$root/state/ghost-shell/gate.ini"
    opens=$(count_lines "spike-layout: open" "$kwin_log")
    keys "+$alt" "$tab"
    wait_for "$ready_deadline_s" has_count "spike-layout: open" "$kwin_log" "$((opens + 1))" || fail "second opening never came"
    sleep "$hold_settle_s"
    keys "-$alt"
    sleep "$settle_s"
    note "proof 4 second: $(grep -a 'spike-layout: signal' "$kwin_log" | tail -n 1)"

    note "--- proof 7: replacing the package"
    sed -i 's/readonly property string marker: "first"/readonly property string marker: "second"/' "$layout/contents/ui/main.qml"
    dbus-send --bus="$session_bus" --type=method_call --print-reply --dest=org.kde.KWin /KWin org.kde.KWin.reconfigure > /dev/null
    sleep "$settle_s"
    opens=$(count_lines "spike-layout: open" "$kwin_log")
    keys "+$alt" "$tab"
    wait_for "$ready_deadline_s" has_count "spike-layout: open" "$kwin_log" "$((opens + 1))" || fail "the opening after replacing never came"
    sleep "$hold_settle_s"
    keys "-$alt"
    sleep "$settle_s"
    note "proof 7: $(grep -a 'spike-layout: open' "$kwin_log" | tail -n 1)"

    note "--- proof 5: warnings in kwin.log"
    grep -aE -- "$warning_pattern" "$kwin_log" | sort | uniq -c | sort -rn | head -n 30 | cut -c1-220 | tee -a "$results" || true
    kwin_alive && note "KWin alive at the end" || note "KWin DEAD at the end"
}

effects() {
    local attempt pressed released
    note "--- proof 3: arrival and departure on the panel's spot"
    for ((attempt = 0; attempt < 3; attempt++)); do
        watch_area "effects-$attempt" 3.2 "${panel_spot[@]}" &
        local watcher=$!
        sleep 0.8
        pressed=$(date +%s%3N)
        keys "+$alt" "$tab"
        sleep 1
        released=$(date +%s%3N)
        keys "-$alt"
        wait "$watcher" || true
        awk -F'\t' -v press="$pressed" -v release="$released" -v n="$attempt" '
            function differs(a, b) { d = a - b; if (d < 0) { d = -d } return d > 3 }
            NR == 1 { next }
            { t[NR] = $1; s[NR] = $2 + $3 + $4; last = NR }
            END {
                for (i = 2; i <= last; i++) {
                    if (t[i] < press) { before = s[i] }
                    if (t[i] < release) { open_value = s[i] }
                }
                final = s[last]
                for (i = 2; i <= last; i++) {
                    if (t[i] >= press && t[i] < release) {
                        if (!opened && differs(s[i], before)) { opened = t[i] }
                        if (differs(s[i], open_value)) { opening_last = t[i] }
                    }
                    if (t[i] >= release) {
                        if (!closed && differs(s[i], open_value)) { closed = t[i] }
                        if (differs(s[i], final)) { closing_last = t[i] }
                    }
                }
                printf "opening %d: press to first change +%d ms, settled %d ms later | closing: release to first change +%d ms, gone %d ms later | levels %d, %d, %d (%d captures)\n", n, opened - press, opening_last - opened, closed - release, closing_last - closed, before, open_value, final, last - 1
            }' "$out/effects-$attempt.tsv" | tee -a "$results"
        sleep 0.5
    done
    grep -a 'spike-layout: open' "$kwin_log" | tee -a "$results" || true
    kwin_alive && note "KWin alive at the end" || note "KWin DEAD at the end"
}

timing() {
    local attempt pressed first_change
    for ((attempt = 0; attempt <= later_openings; attempt++)); do
        watch_area "timing-$attempt" 2 "${centre_spot[@]}" &
        local watcher=$!
        sleep 0.7
        keys "+$alt" "$tab"
        sleep 0.8
        keys "-$alt"
        wait "$watcher" || true
        # The press is the Tab key's roundtrip, the moment KWin starts its delay.
        pressed=$(awk -v n="$((attempt + 1))" '/typed 15 / { if (++seen == n) { print $NF; exit } }' "$out/fake-keys.log")
        first_change=$(awk -F'\t' -v after="$pressed" 'NR == 2 { base = $2 + $3 + $4 } NR > 2 && $1 >= after { d = $2 + $3 + $4 - base; if (d < 0) { d = -d } if (d > 6) { print $1; exit } }' "$out/timing-$attempt.tsv")
        note "opening $attempt: pressed $pressed, first changed capture ${first_change:-none}, +$((${first_change:-$pressed} - pressed)) ms; captures $(($(wc -l < "$out/timing-$attempt.tsv") - 1)) in 2 s"
        sleep 1
    done
    grep -a 'spike-layout: open' "$kwin_log" | tee -a "$results" || true
}

broken() {
    local before after
    note "--- proof 8: $mode"
    kwin_alive && note "alive before any Alt+Tab" || note "DEAD before any Alt+Tab"
    before=$(active_lines)
    flick
    sleep "$settle_s"
    after=$(active_lines)
    note "flick: activations $((after - before))"
    kwin_alive && note "alive after the flick" || fail "KWin died on a flick, see $kwin_log"
    before=$after
    keys "+$alt" "$tab"
    sleep "$settle_s"
    kwin_alive && note "alive while held" || fail "KWin died on a hold, see $kwin_log"
    grab held
    keys "-$alt"
    sleep "$settle_s"
    after=$(active_lines)
    note "hold: activations $((after - before))"
    before=$after
    keys "+$alt" "$tab"
    sleep "$settle_s"
    keys "-$alt"
    sleep "$settle_s"
    after=$(active_lines)
    note "second hold: activations $((after - before))"
    kwin_alive && note "alive at the end" || note "DEAD at the end"
    note "what KWin logged:"
    grep -aE -- "$warning_pattern|spike|tabbox" "$kwin_log" | sort | uniq -c | sort -rn | head -n 12 | cut -c1-220 | tee -a "$results" || true
}

start_session
case $mode in
    proofs) proofs ;;
    effects) effects ;;
    timing) timing ;;
    broken-*) broken ;;
    *) fail "unknown mode $mode" ;;
esac
note "done"

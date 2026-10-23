#!/usr/bin/env bash
set -euo pipefail

# Plan 3a Task 2 end to end, in sandbox.sh's headless KWin: a real plasmashell on copies of the user's Plasma
# configs, its panel retired through plasma-panel.sh, Plasma's popups opened through popup.sh, Meta and Meta+1…9
# handed to a stand-in ghost-shell, plasmashell's memory with and without the panel, and the restore.
#   SPIKE_OUT   directory for results and captures, kept afterwards (default: a new temp dir)

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=/tmp/gs-sbx-3a-t2
out=${SPIKE_OUT:-$(mktemp -d -t plasma-hidden-spike.XXXXXX)}
results=$out/results.txt

sbx=("$spike/sandbox.sh")
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)
settle_s=20
rss_settle_s=60
rss_samples=3
rss_gap_s=10
popup_open_s=1.5
popup_close_s=1
taps=3
popup_names=(network bluetooth volume kdeconnect clipboard tray calendar)
# evdev codes for sandbox.sh keys.
key_esc=1
key_meta=125
key_alt=56
key_f1=59
key_v=47
# Meta+1…9 and Meta+0; evdev puts 0 after 9.
digit_keys=(2 3 4 5 6 7 8 9 10 11)
# Qt key codes as KGlobalAccel stores them: bare Meta, and Meta plus a digit.
qt_meta=$((0x01000022))
qt_meta_digit=$((0x10000030))

monitor_pid=
ghost_pids=()

note() {
    echo "$*" | tee -a "$results"
}

stop_helpers() {
    local pid
    for pid in "${ghost_pids[@]}" $monitor_pid; do
        kill "$pid" 2> /dev/null || true
        wait "$pid" 2> /dev/null || true
    done
    ghost_pids=()
    monitor_pid=
}

cleanup() {
    stop_helpers
    if [[ -e $root/env ]]; then
        "${sbx[@]}" down || true
    fi
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

source_env() {
    # shellcheck source=/dev/null
    source "$root/env"
    [[ $session_bus == unix:* && $session_bus != *"/run/user/"* ]] || { echo "refusing ${session_bus@Q}" >&2; exit 1; }
}

popups_open() {
    "${sbx[@]}" windows | grep -vc '"desktopWindow":true' || true
}

window_lines() {
    "${sbx[@]}" windows | grep -v '"desktopWindow":true' \
        | sed -E 's/.*"resourceClass":"([^"]*)","caption":"([^"]*)","x":([0-9.]+),"y":([0-9.]+),"width":([0-9.]+),"height":([0-9.]+).*/\1 "\2" at \3,\4 \5x\6/' \
        | awk '{ for (i = 1; i <= NF; i++) { if ($i ~ /^[0-9.]+$|^[0-9.]+,[0-9.]+$|^[0-9.]+x[0-9.]+$/) { gsub(/\.[0-9]+/, "", $i) } } print }' \
        | paste -sd ';'
}

rss() {
    local i
    sleep "$rss_settle_s"
    for ((i = 1; i <= rss_samples; i++)); do
        note "$1: $("${sbx[@]}" rss)"
        sleep "$rss_gap_s"
    done
}

holders() {
    local key
    for key in "$qt_meta" $(seq $((qt_meta_digit + 1)) $((qt_meta_digit + 9))) "$qt_meta_digit"; do
        note "$1: $("${sbx[@]}" do kga bykey "$key" | tr '\n' ' ' | sed 's/meta-kga //; s/ at [0-9]* / /')"
    done
}

# Registers bare Meta and Meta+1…9 for a stand-in ghost-shell, as ghostd registers its view shortcuts.
register_ghost_shell() {
    local n
    XDG_RUNTIME_DIR="$root/run" "${py[@]}" "$spike/../bar-bridge/meta_kglobalaccel.py" register "$session_bus" \
        ghost-shell launcher "Open the launcher" "$qt_meta" > "$out/ghost-launcher.log" 2>&1 &
    ghost_pids+=($!)
    for n in 1 2 3 4 5 6 7 8 9; do
        XDG_RUNTIME_DIR="$root/run" "${py[@]}" "$spike/../bar-bridge/meta_kglobalaccel.py" register "$session_bus" \
            ghost-shell "workspace-$n" "Workspace $n" $((qt_meta_digit + n)) > "$out/ghost-workspace-$n.log" 2>&1 &
        ghost_pids+=($!)
    done
    sleep 6
    note "ghost-shell registered: $(cat "$out"/ghost-*.log | grep -c 'meta-kga registered') of 10"
}

unregister_ghost_shell() {
    local n
    stop_helpers
    "${sbx[@]}" do plasma_dbus unregister ghost-shell launcher > /dev/null
    for n in 1 2 3 4 5 6 7 8 9; do
        "${sbx[@]}" do plasma_dbus unregister ghost-shell "workspace-$n" > /dev/null
    done
}

press_round() {
    local label=$1 log=$out/pressed-$1.log chords=() i
    dbus-monitor --address "$session_bus" "type='signal',member='globalShortcutPressed'" > "$log" 2>&1 &
    monitor_pid=$!
    sleep 1
    for ((i = 0; i < taps; i++)); do
        chords+=("$key_meta")
    done
    for i in "${digit_keys[@]}"; do
        chords+=("$key_meta+$i")
    done
    chords+=("$key_alt+$key_f1")
    "${sbx[@]}" keys "${chords[@]}" > /dev/null
    sleep 1
    kill "$monitor_pid"
    wait "$monitor_pid" 2> /dev/null || true
    monitor_pid=
    note "$label, $taps Meta taps then Meta+1…9, Meta+0, Alt+F1: $(grep -a -A3 'member=globalShortcutPressed' "$log" \
        | grep -a 'string' | paste - - | awk -F '"' '{ print $2 "/" $4 }' | sort | uniq -c | sed -E 's/^ *([0-9]+) (.*)/\2 \1/' | paste -sd ',')"
}

: > "$results"
note "out=$out"
"${sbx[@]}" up | tee -a "$results"
source_env
sleep "$settle_s"

note "== step 1: plasmashell with the user's panel"
"${sbx[@]}" shot "$out/01-panel.png" > /dev/null
note "windows: $(window_lines)"
rss "panel, first start"
holders "before retire"

note "== step 2: retire"
"${sbx[@]}" retire | tee -a "$results"
source_env
sleep "$settle_s"
"${sbx[@]}" shot "$out/02-retired.png" > /dev/null
note "windows: $(window_lines)"
note "org.freedesktop.Notifications owner pid: $("${sbx[@]}" do plasma_dbus pid org.freedesktop.Notifications), plasmashell $plasma_pid"
note "notification: $("${sbx[@]}" do plasma_dbus notify "plasma-hidden spike")"
sleep "$popup_open_s"
note "toast: $(window_lines)"
"${sbx[@]}" shot "$out/03-toast.png" > /dev/null
client=(env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" WAYLAND_DISPLAY=ghost-sbx-3a-t2)
"${client[@]}" wl-copy "plasma-hidden spike 1"
sleep 1
"${client[@]}" wl-copy "plasma-hidden spike 2"
sleep 1
note "clipboard history entries: $(dbus-send --bus="$session_bus" --print-reply --dest=org.kde.klipper /klipper \
    org.kde.klipper.klipper.getClipboardHistoryMenu | grep -c 'string "plasma-hidden spike')"
"${sbx[@]}" keys "$key_meta+$key_v" > /dev/null
sleep "$popup_open_s"
note "Meta+V: $(window_lines)"
"${sbx[@]}" shot "$out/04-meta-v.png" > /dev/null
"${sbx[@]}" keys "$key_esc" > /dev/null
sleep "$popup_close_s"

note "== step 3: popups through popup.sh"
for name in "${popup_names[@]}"; do
    "${sbx[@]}" popup "$name"
    sleep "$popup_open_s"
    note "$name: $(window_lines)"
    "${sbx[@]}" shot "$out/05-popup-$name.png" > /dev/null
    "${sbx[@]}" popup "$name"
    sleep "$popup_close_s"
    note "$name after a second popup.sh: $(popups_open) open"
done
"${sbx[@]}" strip
for name in tray calendar; do
    "${sbx[@]}" popup "$name"
    sleep "$popup_open_s"
    note "$name over the strip: $(window_lines)"
    "${sbx[@]}" shot "$out/06-strip-$name.png" > /dev/null
    "${sbx[@]}" popup "$name"
    sleep "$popup_close_s"
done
"${sbx[@]}" strip stop

note "== step 4: Meta and Meta+1…0"
holders "after retire"
register_ghost_shell
press_round retired
"${sbx[@]}" plasma-restart | tee -a "$results"
source_env
sleep "$settle_s"
holders "after a plasmashell restart"
press_round "retired, plasmashell restarted"
rss "retired, after a restart"
unregister_ghost_shell

note "== step 5: restore"
"${sbx[@]}" restore | tee -a "$results"
source_env
sleep "$settle_s"
"${sbx[@]}" shot "$out/07-restored.png" > /dev/null
note "windows: $(window_lines)"
holders "after restore"
for file in plasma-org.kde.plasma.desktop-appletsrc plasmashellrc kglobalshortcutsrc; do
    note "$file: $(diff "$root/backup/$file" "$root/config/$file" | grep -c '^[<>]' || true) lines differ from the backup"
done
rss "panel, restored"
note "done"

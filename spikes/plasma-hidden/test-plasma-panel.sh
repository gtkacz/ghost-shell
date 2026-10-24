#!/usr/bin/env bash
set -euo pipefail

# Sandbox test for scripts/plasma-panel.sh: a real plasmashell and kglobalaccel (inside a headless KWin) on private
# buses, with XDG dirs under /tmp/gs-sbx-3a-t17 seeded with a fake Plasma layout (nothing is copied from the user).
# It runs retire, checks the screenshot regions, the popup map, the shortcuts and status, refuses a second retire,
# runs restore, and diffs every backed-up file against the original (expected: identical). Failure paths follow: a
# retire that prints no "ok", one killed after plasmashell was stopped, a truncated keys backup, a missing popup map,
# an older retire's Brightness and Color entry in the map,
# repeated retire/restore cycles and a preflight that fails. It also checks that
# KGlobalAccel's setInactive on never-registered ghost-shell task-N slots creates no entries (Task 5's carry-forward).
#   test-plasma-panel.sh                run the test; it builds and removes its own sandbox
#   test-plasma-panel.sh __plasma-stop|__plasma-start
#                                       what the test hands plasma-panel.sh to stop and start the sandbox's plasmashell
#   SANDBOX_ROOT  where the sandbox is built (default: /tmp/gs-sbx 3a-t17, with a space)
#   TEST_OUT   directory for results and captures, kept afterwards (default: a new temp dir)

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
script=$repo/scripts/plasma-panel.sh
name=3a-t17
# The space is deliberate: the script must cope with any path its user's XDG dirs hold.
root=${SANDBOX_ROOT:-/tmp/gs-sbx $name}
self=$spike/$(basename "${BASH_SOURCE[0]}")
wayland_name=ghost-sbx-$name

width=1920
height=1080
socket_deadline_s=15
ready_deadline_s=60
settle_s=20
poll_s=0.2
# The fake panel's thickness; the bar's strip is derived from the shell's own constants, as retire does.
panel_px=44
shell_lib=$repo/shell/kit/GhostKit/lib
top_right_px=720
# The fake desktop's solid wallpaper colour, and how far a region's mean may stray from a colour and still match.
wallpaper_rgb="30 60 90"
match_tolerance=6
# Mean colour distance that counts as a region having changed.
change_threshold=8

mjs_constant() {
    sed -n "s/^export const $2 = \([0-9][0-9]*\);\$/\1/p" "$shell_lib/$1"
}
strip_px=$(awk -v band="$(mjs_constant layout.mjs BAR_BAND_PX)" -v design="$(mjs_constant scale.mjs DESIGN_WIDTH)" \
    -v width="$width" 'BEGIN { printf "%d", band * width / design + 0.5 }')
ghost_config=$root/config/ghost-shell/config.toml
held_key=$((0x1E000000 + 0x01000030 + 2))
held_key_text=Meta+Ctrl+Alt+Shift+F3
# A widget id no sandbox layout uses, standing for the Brightness and Color applet an older retire gave F8.
older_brightness_widget=9999
# What KWin --virtual calls its one output, and so what Plasma's screenForConnector knows it by.
sandbox_connector=Virtual-0

layout_file=plasma-org.kde.plasma.desktop-appletsrc
backed_up_files=("$layout_file" plasmashellrc kglobalshortcutsrc)
activity=c0ffee00-0000-4000-8000-000000000017
launcher_action="activate application launcher"
task_entry="activate task manager entry"
task_entries=10
qt_meta=$((0x01000022))
qt_meta_zero=$((0x10000030))
services=(
    org.kde.kded6 org.kde.ActivityManager org.kde.plasma.Notifications org.kde.KScreen org.kde.kiod6
    org.kde.kpasswdserver6 org.kde.kssld6 ca.desrt.dconf
)

py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)
inherited_bus=${DBUS_SESSION_BUS_ADDRESS:-}
failures=0
launcher_before=

is_private_bus() {
    [[ $1 == unix:* && $1 != *"/run/user/"* && $1 != "$inherited_bus" ]]
}

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

note() {
    echo "$*" | tee -a "$out/results.txt"
}

check() {
    local label=$1
    shift
    if "$@"; then
        note "PASS $label"
    else
        note "FAIL $label"
        failures=$((failures + 1))
    fi
}

load_env() {
    # shellcheck source=/dev/null
    source "$root/env"
    is_private_bus "$session_bus" || { echo "refusing session bus ${session_bus@Q}" >&2; exit 1; }
    is_private_bus "$system_bus" || { echo "refusing system bus ${system_bus@Q}" >&2; exit 1; }
}

client_env() {
    client=(env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config"
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state"
        XDG_CURRENT_DESKTOP=KDE XDG_SESSION_TYPE=wayland KDE_FULL_SESSION=true KDE_SESSION_VERSION=6
        WAYLAND_DISPLAY="$wayland_name" QT_QPA_PLATFORM=wayland QT_FORCE_STDERR_LOGGING=1
        DBUS_SESSION_BUS_ADDRESS="$session_bus" DBUS_SYSTEM_BUS_ADDRESS="$system_bus")
}

# busctl on the sandbox's session bus only.
sbx_busctl() {
    env -i PATH=/usr/bin:/bin DBUS_SESSION_BUS_ADDRESS="$session_bus" busctl --user "$@"
}

name_owned() {
    [[ $(sbx_busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus NameHasOwner s "$1") == "b true" ]]
}

plasma_stop() {
    load_env
    kill "$plasma_pid"
    # A plasmashell stopped moments after it started can ignore TERM; systemd's stop ends it with KILL after a timeout.
    if ! wait_for 20 bash -c "! kill -0 $plasma_pid 2> /dev/null"; then
        kill -KILL "$plasma_pid"
        wait_for 20 bash -c "! kill -0 $plasma_pid 2> /dev/null" || { echo "plasmashell $plasma_pid did not stop" >&2; return 1; }
    fi
}

plasma_start() {
    load_env
    # systemctl start on a running unit does nothing; the script's recovery relies on that.
    if name_owned org.kde.plasmashell; then
        return 0
    fi
    client_env
    # setsid: plasmashell outlives this script; in a background job it execs in place, so $! is plasmashell.
    "${client[@]}" setsid plasmashell --no-respawn > "$root/logs/plasmashell-$(date +%s%N).log" 2>&1 &
    plasma_pid=$!
    sed -i "/^plasma_pid=/d" "$root/env"
    echo "plasma_pid=$plasma_pid" >> "$root/env"
    wait_for "$ready_deadline_s" name_owned org.kde.plasmashell || { echo "plasmashell never owned its name" >&2; return 1; }
}

# Faults are files in $root/faults that one stub consumes, so a failure is injected exactly where a test wants it.
fault_term_at_start() {
    if [[ -e $root/faults/term-at-start ]]; then
        rm -- "$root/faults/term-at-start"
        kill -TERM "$PPID"
        sleep 2
        exit 1
    fi
}

make_stubs() {
    local stub=$root/stub-bin
    # The script's live default stops and starts plasmashell through systemctl; here that must never run.
    printf '#!/bin/sh\necho "systemctl $*" >> "%s/live-path-used"\nexit 1\n' "$root" > "$stub/systemctl"
    printf '#!/bin/sh\nSANDBOX_ROOT="%s" exec "%s" __plasma-stop\n' "$root" "$self" > "$stub/plasma-stop"
    printf '#!/bin/sh\nSANDBOX_ROOT="%s" exec "%s" __plasma-start\n' "$root" "$self" > "$stub/plasma-start"
    printf '#!/bin/sh\n[ ! -e "%s/faults/check-fails" ]\n' "$root" > "$stub/plasma-check"
    # evaluateScript answers without "ok" while the fault is set; everything else reaches the sandbox bus.
    cat > "$stub/busctl" <<- EOF
	#!/bin/sh
	if [ -e "$root/faults/no-ok" ]; then
	    for arg; do
	        if [ "\$arg" = evaluateScript ]; then
	            echo '{"type":"s","data":["the script ran but printed nothing"]}'
	            exit 0
	        fi
	    done
	fi
	exec /usr/bin/busctl "\$@"
	EOF
    chmod +x "$stub"/*
}

seed_config() {
    local config=$root/config
    cat > "$config/$layout_file" <<- EOF
	[ActionPlugins][0]
	RightButton;NoModifier=org.kde.contextmenu

	[Containments][1]
	activityId=$activity
	formfactor=0
	immutability=1
	lastScreen=0
	location=0
	plugin=org.kde.plasma.folder
	wallpaperplugin=org.kde.color

	[Containments][1][Wallpaper][org.kde.color][General]
	Color=${wallpaper_rgb// /,}

	[Containments][2]
	activityId=
	formfactor=2
	immutability=1
	lastScreen=0
	location=4
	plugin=org.kde.panel
	wallpaperplugin=org.kde.image

	[Containments][2][Applets][3]
	immutability=1
	plugin=org.kde.plasma.kickoff

	[Containments][2][Applets][4]
	immutability=1
	plugin=org.kde.plasma.icontasks

	[Containments][2][Applets][4][Configuration][General]
	launchers=applications:org.kde.dolphin.desktop

	[Containments][2][Applets][5]
	immutability=1
	plugin=org.kde.plasma.systemtray

	[Containments][2][Applets][5][Configuration][General]
	extraItems=org.kde.plasma.networkmanagement,org.kde.plasma.bluetooth,org.kde.plasma.volume,org.kde.kdeconnect,org.kde.plasma.clipboard,org.kde.plasma.brightness,org.kde.plasma.notifications
	knownItems=org.kde.plasma.networkmanagement,org.kde.plasma.bluetooth,org.kde.plasma.volume,org.kde.kdeconnect,org.kde.plasma.clipboard,org.kde.plasma.brightness,org.kde.plasma.notifications
	hiddenItems=org.kde.kdeconnect

	[Containments][2][Applets][6]
	immutability=1
	plugin=org.kde.plasma.digitalclock

	[Containments][2][Applets][6][Configuration][Appearance]
	showSeconds=2
	enabledCalendarPlugins=holidaysevents
	showWeekNumbers=true

	[Containments][2][General]
	AppletOrder=3;4;5;6
	EOF
    cat > "$config/plasmashellrc" <<- EOF
	[PlasmaViews][Panel 2]
	floating=0

	[PlasmaViews][Panel 2][Defaults]
	thickness=$panel_px
	EOF
    # Keeps plasma-welcome's first-run window off the captures.
    cat > "$config/plasma-welcomerc" <<- EOF
	[General]
	LastSeenVersion=$(plasmashell --version | awk '{ print $2 }')
	ShowUpdatePage=false
	EOF
    cat > "$config/kactivitymanagerdrc" <<- EOF
	[activities]
	$activity=Default

	[main]
	currentActivity=$activity
	EOF
}

up() {
    if [[ -e $root ]]; then
        echo "$root exists; another run is using it or left it behind" >&2
        exit 1
    fi
    mkdir -p "$root"/{run,config,data,cache,state,services,logs,stub-bin,before,faults,drm}
    chmod 700 "$root/run"
    seed_config
    for service in "${services[@]}"; do
        ln -s -- "/usr/share/dbus-1/services/$service.service" "$root/services/"
    done
    sed "s|</busconfig>|  <servicedir>$root/services</servicedir>\n</busconfig>|" "$repo/scripts/private-bus.conf" \
        > "$root/session-bus.conf"
    make_stubs

    dbus-daemon --config-file="$repo/scripts/private-bus.conf" --nofork --print-address=3 \
        3> "$root/system.addr" > "$root/logs/system-bus.log" 2>&1 &
    local system_pid=$!
    wait_for 10 test -s "$root/system.addr" || { echo "no private system bus" >&2; exit 1; }
    system_bus=$(head -n 1 "$root/system.addr")
    is_private_bus "$system_bus" || { echo "refusing system bus ${system_bus@Q}" >&2; exit 1; }
    # The tray loads its Bluetooth applet only while org.bluez is on the system bus.
    XDG_RUNTIME_DIR="$root/run" "${py[@]}" "$spike/plasma_dbus.py" own "$system_bus" org.bluez \
        > "$root/logs/bluez.log" 2>&1 &
    local bluez_pid=$!
    wait_for 30 grep -q 'own org.bluez: 1' "$root/logs/bluez.log" || { echo "no org.bluez stand-in" >&2; exit 1; }

    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
        XDG_CURRENT_DESKTOP=KDE XDG_SESSION_TYPE=wayland KDE_FULL_SESSION=true KDE_SESSION_VERSION=6 \
        DBUS_SYSTEM_BUS_ADDRESS="$system_bus" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 QT_FORCE_STDERR_LOGGING=1 \
        setsid dbus-run-session --config-file="$root/session-bus.conf" -- \
        sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width "$2" --height "$3"' \
        "$root" "$wayland_name" "$width" "$height" > "$root/logs/kwin.log" 2>&1 &
    local session_pid=$!
    wait_for "$socket_deadline_s" test -S "$root/run/$wayland_name" || { echo "KWin made no socket" >&2; exit 1; }
    session_bus=$(< "$root/session.addr")
    is_private_bus "$session_bus" || { echo "refusing session bus ${session_bus@Q}" >&2; exit 1; }
    cat > "$root/env" <<- EOF
	session_bus=$session_bus
	system_bus=$system_bus
	system_pid=$system_pid
	bluez_pid=$bluez_pid
	session_pid=$session_pid
	kwin_pid=$(< "$root/kwin.pid")
	EOF
    note "isolation: inherited session bus ${inherited_bus@Q}"
    note "isolation: sandbox session bus (KWin, kglobalaccel, plasmashell, plasma-panel.sh) $session_bus"
    note "isolation: sandbox system bus $system_bus"

    "${py[@]}" "$spike/plasma_dbus.py" env "$session_bus" WAYLAND_DISPLAY="$wayland_name" QT_QPA_PLATFORM=wayland \
        QT_FORCE_STDERR_LOGGING=1
    wait_for "$ready_deadline_s" name_owned org.kde.kglobalaccel || { echo "KGlobalAccel never appeared" >&2; exit 1; }
    # KWin holds file capabilities, so its /proc environ is unreadable; the bus owner of kglobalaccel shows where it is.
    note "isolation: kglobalaccel owner pid on the sandbox bus $("${py[@]}" "$spike/plasma_dbus.py" pid "$session_bus" org.kde.kglobalaccel), sandbox KWin pid $(< "$root/kwin.pid")"
    "${py[@]}" "$spike/plasma_dbus.py" start "$session_bus" org.kde.kded6 > /dev/null
    plasma_start
    load_env
    note "isolation: plasmashell's own DBUS_SESSION_BUS_ADDRESS $(tr '\0' '\n' < "/proc/$plasma_pid/environ" | sed -n 's/^DBUS_SESSION_BUS_ADDRESS=//p')"
}

# Every process the sandbox started, however it was started, carries its runtime dir in its environment.
sandbox_pids() {
    local environ pid
    for environ in /proc/[0-9]*/environ; do
        pid=${environ#/proc/}
        pid=${pid%/environ}
        if [[ -O $environ && -r $environ ]] && tr '\0' '\n' 2> /dev/null < "$environ" | grep -qxF "XDG_RUNTIME_DIR=$root/run"; then
            echo "$pid"
        fi
    done
}

down() {
    [[ -s $root/env ]] || return 0
    load_env
    local pids=() pid
    mapfile -t pids < <(sandbox_pids | while read -r pid; do
        if [[ $pid != "$session_pid" && $pid != "$$" && $(ps -o ppid= -p "$pid" | tr -d ' ') != "$session_pid" ]]; then
            echo "$pid"
        fi
    done)
    for pid in "${pids[@]}"; do
        kill "$pid" 2> /dev/null || true
    done
    wait_for 15 bash -c 'for pid; do kill -0 "$pid" 2> /dev/null && exit 1; done; exit 0' _ "${pids[@]}" \
        || echo "clients still running after 15 s" >&2
    kill "$kwin_pid" 2> /dev/null || true
    wait_for 10 bash -c "! kill -0 $session_pid 2> /dev/null" || echo "session $session_pid still running" >&2
    kill "$system_pid" 2> /dev/null || true
    sleep 1
    local left
    left=$(sandbox_pids | { grep -vxF "$$" || true; } | tr '\n' ' ')
    if kill -0 "$system_pid" 2> /dev/null; then
        left+="$system_pid "
    fi
    if [[ -n ${left// /} ]]; then
        echo "still running, sandbox kept at $root: $left" >&2
        return 1
    fi
    rm -r -- "$root"
}

# plasma-panel.sh with nothing of this shell's environment: sandbox dirs, the sandbox bus, the systemctl stub first
# in PATH, and the sandbox's own plasmashell stop and start.
run_script() {
    env -i HOME="$root" PATH="$root/stub-bin:/usr/bin:/bin" XDG_RUNTIME_DIR="$root/run" \
        XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state" \
        DBUS_SESSION_BUS_ADDRESS="$session_bus" DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent \
        GHOST_PLASMA_STOP="$root/stub-bin/plasma-stop" GHOST_PLASMA_START="$root/stub-bin/plasma-start" \
        GHOST_PLASMA_CHECK="$root/stub-bin/plasma-check" GHOST_DRM_DIR="$root/drm" \
        "$script" "$@"
}

# Runs the script, keeps its output in $out/last.txt and its status in $script_rc; a failing script does not stop the test.
try() {
    script_rc=0
    # Backups are named by the second; a retire right after a restore would collide.
    if [[ $1 == retire ]]; then sleep 1; fi
    run_script "$@" > "$out/last.txt" 2>&1 || script_rc=$?
    tee -a "$out/results.txt" < "$out/last.txt"
    load_env
}

# The keys of one plasmashell action, as ints joined by spaces.
action_keys() {
    sbx_busctl --json=short call org.kde.kglobalaccel /component/plasmashell org.kde.kglobalaccel.Component \
        allShortcutInfos | jq -r --arg action "$1" '.data[0][] | select(.[0] == $action) | .[6] | map(tostring) | join(" ")'
}

action_names() {
    sbx_busctl --json=short call org.kde.kglobalaccel "/component/$1" org.kde.kglobalaccel.Component \
        allShortcutInfos | jq -r '.data[0][][0]'
}

holders_of() {
    sbx_busctl --json=short call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel globalShortcutsByKey \
        '(ai)(i)' 4 "$1" 0 0 0 0 | jq -r '.data[0][] | .[2] + "/" + .[0]' | paste -sd ' '
}

region_mean() {
    local tsv=$out/region-$1.tsv
    "${py[@]}" "$spike/../greeting-handoff/grab.py" watch "$session_bus" "$tsv" 0.1 "${@:2}" > /dev/null
    awk -F '\t' 'NR == 2 { print $2, $3, $4 }' "$tsv"
}

distance() {
    awk -v a="$1" -v b="$2" 'BEGIN { split(a, x, " "); split(b, y, " "); d = 0
        for (i = 1; i <= 3; i++) { v = x[i] - y[i]; d += v < 0 ? -v : v }; printf "%.1f", d / 3 }'
}

near() {
    awk -v d="$(distance "$1" "$2")" -v t="$3" 'BEGIN { exit !(d <= t) }'
}

far() {
    ! near "$@"
}

refuses() {
    ! "$@" >> "$out/results.txt" 2>&1
}

same_files() {
    local from=$1 to=$2 file ok=0
    for file in "${backed_up_files[@]}"; do
        if cmp -s -- "$from/$file" "$to/$file"; then
            note "  $file: identical"
        else
            note "  $file: DIFFERS"
            diff -- "$from/$file" "$to/$file" | head -n 20 | tee -a "$out/results.txt" || true
            ok=1
        fi
    done
    return "$ok"
}

popup_map_is_valid() {
    local map=$root/state/ghost-shell/plasma-popups accepted applet id lines=0
    accepted=$(sed -n '/POPUP_APPLETS: \[&str; [0-9]*\] = \[/,/\];/p' "$repo/crates/ghostd/src/feeds/actions.rs" \
        | grep -o '"[^"]*"' | tr -d '"')
    [[ -s $map ]] || return 1
    while IFS=$'\t' read -r applet id; do
        lines=$((lines + 1))
        grep -qxF "$applet" <<< "$accepted" || { note "  $applet is not in POPUP_APPLETS"; return 1; }
        [[ $id =~ ^[0-9]+$ ]] || { note "  $applet has id ${id@Q}"; return 1; }
        grep -q "^\[Containments\].*\[Applets\]\[$id\]\]*$" "$root/config/$layout_file" \
            || { note "  no applet $id in the layout"; return 1; }
        action_names plasmashell | grep -qxF "activate widget $id" || { note "  no 'activate widget $id' action"; return 1; }
    done < "$map"
    note "  $lines popups: $(tr '\t\n' '= ' < "$map")"
    ((lines == $(wc -l <<< "$accepted")))
}

# After a restore the desktop must be what it was before the first retire, whatever went wrong on the way.
assert_restored() {
    check "$1: restore succeeded" test "$script_rc" -eq 0
    check "$1: files byte-identical to before the first retire" same_files "$root/before" "$root/config"
    check "$1: status is back to in place" test "$(run_script status)" == "plasma panel in place"
    check "$1: launcher keys are back" test "$(action_keys "$launcher_action")" == "$launcher_before"
    check "$1: $task_entry 1 has Meta+1 back" test "$(action_keys "$task_entry 1")" == "$((qt_meta_zero + 1))"
    check "$1: plasmashell is up" name_owned org.kde.plasmashell
    check "$1: no popup map or part file left" bash -c '! ls "$1"/plasma-popups* > /dev/null 2>&1' _ "$root/state/ghost-shell"
    check "$1: the popups' actions are gone" bash -c '! grep -q "^activate widget" <<< "$1"' _ "$(action_names plasmashell)"
}

latest_backup() {
    local stamp
    stamp=$(run_script status | sed -n 's/^retired //p')
    echo "$root/state/ghost-shell/plasma-backup/$stamp"
}

hint_names_restore() {
    grep -qF "$(printf '%q' "$script") restore" "$out/last.txt"
}

# "x y width height" of an applet as the layout file saves it.
applet_geometry() {
    grep -o "Applet-$1:[0-9.]*,[0-9.]*,[0-9.]*,[0-9.]*" "$root/config/$layout_file" | head -n 1 | sed 's/^[^:]*://; s/,/ /g'
}

# A monitor whose EDID names it "FakeMon", on the sandbox KWin's only connector.
make_fake_edid() {
    mkdir -p "$root/drm/card9-$sandbox_connector"
    { head -c 72 /dev/zero; printf '\x00\x00\x00\xfc\x00FakeMon\n     '; head -c 38 /dev/zero; } > "$root/drm/card9-$sandbox_connector/edid"
}

# Where retire put the tray and the clock: along the bar's edge, the strip's height, the bar's side.
check_placement() {
    local at_bottom=$1 id geometry x y w h tray_id clock_id
    local want_y=0
    if [[ $at_bottom == true ]]; then
        want_y=$((height - strip_px))
    fi
    check "retire asked for a $strip_px px strip at y $want_y" grep -q "strip $strip_px px at y $want_y\$" "$out/last.txt"
    tray_id=$(awk '$1 == "tray" { print $2 }' "$out/last.txt")
    clock_id=$(awk '$1 == "clock" { print $2 }' "$out/last.txt")
    for id in "$tray_id" "$clock_id"; do
        geometry=$(applet_geometry "$id")
        read -r x y w h <<< "$geometry"
        note "  applet $id at $geometry (strip $strip_px px, screen ${width}x${height})"
        if [[ $at_bottom == true ]]; then
            # Plasma's desktop moves a widget by up to a few tens of px to fit its grid, so "along the bottom" is one strip.
            check "applet $id sits along the bottom edge" awk -v y="$y" -v h="$h" -v screen="$height" -v strip="$strip_px" \
                'BEGIN { exit !(y + h >= screen - strip && y + h <= screen + 1) }'
        else
            check "applet $id sits on the top edge" awk -v y="$y" 'BEGIN { exit !(y <= 1) }'
        fi
    done
}

failure_paths() {
    local backup map=$root/state/ghost-shell/plasma-popups backups_before

    note "== the sandbox path contains a space"
    check "the sandbox root has a space" bash -c '[[ $1 == *" "* ]]' _ "$root"

    note "== retire whose script prints no ok, then restore"
    touch "$root/faults/no-ok"
    try retire
    rm -- "$root/faults/no-ok"
    check "retire fails when the script prints no ok" test "$script_rc" -ne 0
    check "the failure names the restore command" hint_names_restore
    check "the panel is still marked retired" test "$(run_script status | cut -d' ' -f1)" == retired
    check "plasmashell is still up" name_owned org.kde.plasmashell
    try restore
    assert_restored "no ok"

    note "== retire killed while plasmashell is stopped, then restore"
    touch "$root/faults/term-at-start"
    try retire
    rm -f -- "$root/faults/term-at-start"
    check "retire dies on the signal" test "$script_rc" -ne 0
    check "the failure names the restore command" hint_names_restore
    check "the exit started plasmashell again" wait_for 60 name_owned org.kde.plasmashell
    check "the panel is marked retired" test "$(run_script status | cut -d' ' -f1)" == retired
    try restore
    assert_restored "killed"

    note "== a truncated keys backup is refused by restore (cycle 2)"
    try retire
    check "retire succeeds" test "$script_rc" -eq 0
    backup=$(latest_backup)
    check "the second backup is byte-identical to the first" same_files "$root/before" "$backup"
    cp -- "$backup/keys" "$out/keys.full"
    head -n 5 "$out/keys.full" > "$backup/keys"
    local pid_before=$plasma_pid
    try restore
    check "restore refuses a truncated keys backup" test "$script_rc" -ne 0
    check "plasmashell was not stopped by the refusal" kill -0 "$pid_before"
    check "the panel stays marked retired" test "$(run_script status | cut -d' ' -f1)" == retired
    cp -- "$out/keys.full" "$backup/keys"
    try restore
    assert_restored "truncated keys"

    note "== a bottom bar on a screen picked by model, and a missing popup map (cycle 3)"
    make_fake_edid
    mkdir -p "$root/config/ghost-shell"
    printf '[bar]\nposition = "bottom"\n\n[display]\nmodel = "fakemon"\n' > "$ghost_config"
    try retire
    check "retire succeeds" test "$script_rc" -eq 0
    sleep "$settle_s"
    check_placement true
    check "the third backup is byte-identical to the first" same_files "$root/before" "$(latest_backup)"
    mv -- "$map" "$map.part"
    try restore
    assert_restored "missing popup map"
    rm -- "$ghost_config"
    sleep "$settle_s"
    check "the files are still identical after plasmashell settled" same_files "$root/before" "$root/config"

    note "== a popup map an older retire wrote, with a Brightness and Color entry, then restore (cycle 4)"
    try retire
    check "retire succeeds" test "$script_rc" -eq 0
    sleep "$settle_s"
    check "retire left Brightness and Color out of the map" bash -c '! grep -q "^org.kde.plasma.brightness" "$1"' _ "$map"
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel doRegister as 4 plasmashell \
        "activate widget $older_brightness_widget" plasmashell "Activate Brightness and Color"
    printf 'org.kde.plasma.brightness\t%s\n' "$older_brightness_widget" >> "$map"
    check "plasmashell holds the older entry's action" bash -c 'grep -qxF "activate widget $1" <<< "$2"' _ \
        "$older_brightness_widget" "$(action_names plasmashell)"
    try restore
    assert_restored "older brightness entry"
    sleep "$settle_s"

    note "== a preflight that fails changes nothing"
    backups_before=$(ls "$root/state/ghost-shell/plasma-backup" | wc -l)
    touch "$root/faults/check-fails"
    try retire
    rm -- "$root/faults/check-fails"
    check "retire refuses when the unit does not own plasmashell" test "$script_rc" -ne 0
    check "no marker was written" test "$(run_script status)" == "plasma panel in place"
    check "no backup was made" test "$(ls "$root/state/ghost-shell/plasma-backup" | wc -l)" == "$backups_before"
    check "the files are untouched" same_files "$root/before" "$root/config"

    note "== a popup key another action holds is refused, with nothing changed"
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel doRegister as 4 ghost-shell-test held-key \
        ghost-shell-test "Held key" > /dev/null
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel setForeignShortcutKeys 'asa(ai)' 4 \
        ghost-shell-test held-key "" "" 1 4 "$held_key" 0 0 0
    backups_before=$(ls "$root/state/ghost-shell/plasma-backup" | wc -l)
    try retire
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel unregister ss ghost-shell-test held-key > /dev/null
    check "retire refuses a taken popup key" test "$script_rc" -ne 0
    check "the refusal names the key and its holder" bash -c 'grep -q "$1" "$3" && grep -q "$2" "$3"' _ "$held_key_text" "held-key" "$out/last.txt"
    check "no marker was written" test "$(run_script status)" == "plasma panel in place"
    check "no backup was made" test "$(ls "$root/state/ghost-shell/plasma-backup" | wc -l)" == "$backups_before"
    # kglobalshortcutsrc is left out: registering the holder above is what writes to it.
    check "the layout and plasmashellrc are untouched by the refusal" bash -c \
        'cmp -s "$1/$3" "$2/$3" && cmp -s "$1/plasmashellrc" "$2/plasmashellrc"' _ "$root/before" "$root/config" "$layout_file"
}

# Task 5's carry-forward: ghostd sends setInactive for every unconfigured task-N slot; it must not create them.
check_set_inactive() {
    local n
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel doRegister as 4 ghost-shell task-1 \
        ghost-shell "Activate pinned app 1"
    for ((n = 2; n <= task_entries; n++)); do
        sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel setInactive as 4 ghost-shell \
            "task-$n" ghost-shell "Activate pinned app $n"
    done
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel setInactive as 4 ghost-shell-never \
        task-1 ghost-shell-never "Activate pinned app 1"
    sleep 2
    note "setInactive: ghost-shell actions: $(action_names ghost_shell | paste -sd ' ')"
    note "setInactive: components: $(sbx_busctl --json=short call org.kde.kglobalaccel /kglobalaccel \
        org.kde.KGlobalAccel allComponents | jq -r '.data[0][]' | paste -sd ' ')"
    note "setInactive: kglobalshortcutsrc ghost-shell lines: $(grep -c '^task-\|ghost-shell' "$root/config/kglobalshortcutsrc" || true)"
    [[ $(action_names ghost_shell) == task-1 ]] \
        && ! sbx_busctl call org.kde.kglobalaccel /component/ghost_shell_never org.freedesktop.DBus.Peer Ping 2> /dev/null
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel unregister ss ghost-shell task-1 > /dev/null
}

run_test() {
    out=${TEST_OUT:-$(mktemp -d -t test-plasma-panel.XXXXXX)}
    : > "$out/results.txt"
    local file
    trap down EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    note "out=$out"
    up
    sleep "$settle_s"

    note "== Task 5: setInactive on unconfigured task-N slots"
    check "setInactive creates no ghost-shell task-N entries" check_set_inactive

    # A first start in a fresh config still adds to it; the user's Plasma has started many times, so the baseline is
    # taken after a restart, and what that restart alone changes is recorded.
    note "== control: a plain plasmashell restart"
    mkdir -p "$root/control"
    for file in "${backed_up_files[@]}"; do
        cp -- "$root/config/$file" "$root/control/"
    done
    plasma_stop
    plasma_start
    load_env
    sleep "$settle_s"
    for file in "${backed_up_files[@]}"; do
        note "  $file: $(diff -- "$root/control/$file" "$root/config/$file" | grep '^[<>]' | paste -sd ';' || true) changed by a plain restart"
    done

    note "== before retire"
    check "status before retire" test "$(run_script status)" == "plasma panel in place"
    local panel_before top_before key
    for file in "${backed_up_files[@]}"; do
        cp -- "$root/config/$file" "$root/before/"
    done
    panel_before=$(region_mean panel-before 0 $((height - panel_px)) "$width" "$panel_px")
    top_before=$(region_mean top-before $((width - top_right_px)) 0 "$top_right_px" "$strip_px")
    launcher_before=$(action_keys "$launcher_action")
    note "panel region $panel_before; top-right region $top_before; launcher keys $launcher_before"
    note "Meta+1 holders: $(holders_of $((qt_meta_zero + 1))); Meta+0 holders: $(holders_of "$qt_meta_zero")"
    check "the panel is drawn" far "$panel_before" "$wallpaper_rgb" "$match_tolerance"
    check "the launcher holds bare Meta" grep -qw "$qt_meta" <<< "$launcher_before"

    note "== retire"
    try retire
    check "retire succeeds" test "$script_rc" -eq 0
    sleep "$settle_s"
    note "== a top bar (no ghost-shell config): the applets sit on the top edge"
    check_placement false
    local stamp backup panel_after top_after launcher_after
    stamp=$(run_script status | sed -n 's/^retired //p')
    backup=$root/state/ghost-shell/plasma-backup/$stamp
    check "status after retire" test -n "$stamp"
    check "the backup has a manifest" test -s "$backup/manifest"
    note "manifest: $(paste -sd ';' "$backup/manifest" 2> /dev/null)"
    note "backup against the files before retire:"
    check "the backup is byte-identical to the originals" same_files "$root/before" "$backup"
    check "the popup map holds every POPUP_APPLETS applet the tray has, each with a live action" popup_map_is_valid
    panel_after=$(region_mean panel-after 0 $((height - panel_px)) "$width" "$panel_px")
    top_after=$(region_mean top-after $((width - top_right_px)) 0 "$top_right_px" "$strip_px")
    note "panel region $panel_after; top-right region $top_after"
    "${py[@]}" "$spike/../greeting-handoff/grab.py" shot "$session_bus" "$out/retired.png" > /dev/null
    cp -- "$root/config/$layout_file" "$out/retired-$layout_file"
    check "the panel is gone (its region shows the wallpaper)" near "$panel_after" "$wallpaper_rgb" "$match_tolerance"
    check "the tray and clock are drawn top right" far "$top_after" "$top_before" "$change_threshold"
    launcher_after=$(action_keys "$launcher_action")
    note "launcher keys $launcher_after"
    check "the launcher lost bare Meta" bash -c '! grep -qw "$1" <<< "$2"' _ "$qt_meta" "$launcher_after"
    check "the launcher kept its other keys" test "$launcher_after" == "$(tr ' ' '\n' <<< "$launcher_before" \
        | grep -vxF "$qt_meta" | paste -sd ' ')"
    for ((key = 1; key <= task_entries; key++)); do
        check "$task_entry $key has no keys" test -z "$(action_keys "$task_entry $key")"
    done
    note "Meta+1 holders: $(holders_of $((qt_meta_zero + 1))); Meta+0 holders: $(holders_of "$qt_meta_zero")"
    check "Meta+0 stays KWin's" test "$(holders_of "$qt_meta_zero")" == "kwin/view_actual_size"

    note "== a second retire"
    check "a second retire is refused" refuses run_script retire
    check "a second retire made no second backup" test "$(ls "$root/state/ghost-shell/plasma-backup" | wc -l)" == 1

    note "== restore"
    try restore
    check "restore succeeds" test "$script_rc" -eq 0
    note "restored files against the backup, at once:"
    check "the restore is byte-identical at once" same_files "$backup" "$root/config"
    sleep "$settle_s"
    note "restored files against the backup, ${settle_s} s later:"
    check "the restore is still byte-identical after plasmashell settled" same_files "$backup" "$root/config"
    check "status after restore" test "$(run_script status)" == "plasma panel in place"
    check "restore deleted the popup map" test ! -e "$root/state/ghost-shell/plasma-popups"
    check "the launcher keys are back" test "$(action_keys "$launcher_action")" == "$launcher_before"
    check "$task_entry 1 has Meta+1 back" test "$(action_keys "$task_entry 1")" == "$((qt_meta_zero + 1))"
    check "the popups' actions are gone" bash -c '! grep -q "^activate widget" <<< "$1"' _ "$(action_names plasmashell)"
    panel_after=$(region_mean panel-restored 0 $((height - panel_px)) "$width" "$panel_px")
    note "panel region $panel_after"
    check "the panel is back" near "$panel_after" "$panel_before" "$change_threshold"
    check "restore without a retire is refused" refuses run_script restore

    failure_paths

    check "the live systemctl path was never taken" test ! -e "$root/live-path-used"
    note "failures: $failures"
    ((failures == 0))
}

case ${1:-} in
    __plasma-stop) inherited_bus= ; plasma_stop ;;
    __plasma-start) inherited_bus= ; load_env; fault_term_at_start; plasma_start ;;
    "") run_test ;;
    *) awk '/^# /{p=1} p&&!/^#/{exit} p' "$0" >&2; exit 64 ;;
esac

#!/usr/bin/env bash
set -euo pipefail

# The plasma-hidden spike's sandbox: a real plasmashell in a headless KWin, with its own session bus (some of KDE's
# services activatable on it) and a second private bus as its system bus. The user's Plasma, KWin and KDE configs
# and their plasma and kactivitymanagerd data are copied into it, and only the copies are changed. run.sh drives it.
#   sandbox.sh up                      build the sandbox and start KWin, kded6 and plasmashell; leave them running
#   sandbox.sh down                    stop everything the sandbox started (by pid, never by name) and remove it
#   sandbox.sh shot <png>              capture the sandbox's workspace
#   sandbox.sh plasma-stop|plasma-start|plasma-restart
#                                      stop and/or start the sandbox's plasmashell
#   sandbox.sh retire|restore          run plasma-panel.sh against the sandbox
#   sandbox.sh popup <name>            run popup.sh against the sandbox
#   sandbox.sh windows                 every window KWin stacks, with its logical geometry
#   sandbox.sh keys <chord>…           type chords such as 125 (Meta) or 125+2 (Meta+1) into the sandbox
#   sandbox.sh strip [exclusive|stop]  show or stop strip.qml, a red stand-in for the bar
#   sandbox.sh rss                     plasmashell's Rss and Pss from smaps_rollup
#   sandbox.sh do <helper> <args…>     run plasma_dbus.py, meta_kglobalaccel.py or grab.py against the session bus
# `up` writes $root/env with the bus addresses and pids.

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
name=3a-t2
root=/tmp/gs-sbx-$name
wayland_name=ghost-sbx-$name

socket_deadline_s=15
ready_deadline_s=60
poll_s=0.2
# KWin takes the virtual output size in logical px: 2226x1252 at the user's 1.15 is their 2560x1440 screen.
width=2226
height=1252
scale=1.15

# kconf_updaterc keeps kconf_update from rerunning every update on the copies: its calendar migration, run on an
# already migrated clock, stores the plugin list as single characters.
config_files=(
    plasma-org.kde.plasma.desktop-appletsrc plasmashellrc plasmarc kdeglobals kwinrc kglobalshortcutsrc kded6rc
    kactivitymanagerdrc kactivitymanagerd-statsrc kcminputrc kxkbrc klipperrc plasmanotifyrc plasma-localerc
    ktimezonedrc plasmaparc bluedevilglobalrc kscreenlockerrc powerdevilrc plasma_calendar_holiday_regions
    kiorc kservicemenurc trashrc plasma-welcomerc kconf_updaterc libnotificationmanager.notifyrc plasma_workspace.notifyrc
)
config_dirs=(kdedefaults)
data_dirs=(plasma kactivitymanagerd)
# Only what plasmashell needs. Left out on purpose: kdeconnectd (it would announce itself on the user's LAN),
# Akonadi (a database server), the portals, and the logout and shutdown services.
services=(
    org.kde.kded6 org.kde.ActivityManager org.kde.plasma.Notifications org.kde.KScreen org.kde.kiod6
    org.kde.kpasswdserver6 org.kde.kssld6 ca.desrt.dconf
)

py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)

is_private_bus() {
    [[ $1 == unix:* && $1 != *"/run/user/"* ]]
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

load_env() {
    [[ -s $root/env ]] || { echo "no sandbox at $root" >&2; exit 1; }
    # shellcheck source=/dev/null
    source "$root/env"
    is_private_bus "$session_bus" || { echo "refusing session bus ${session_bus@Q}" >&2; exit 1; }
    is_private_bus "$system_bus" || { echo "refusing system bus ${system_bus@Q}" >&2; exit 1; }
}

helper() {
    local script=$1
    shift
    case $script in
        plasma_dbus) "${py[@]}" "$spike/plasma_dbus.py" "$1" "$session_bus" "${@:2}" ;;
        kga) "${py[@]}" "$spike/../bar-bridge/meta_kglobalaccel.py" "$1" "$session_bus" "${@:2}" ;;
        grab) "${py[@]}" "$spike/../greeting-handoff/grab.py" "$1" "$session_bus" "${@:2}" ;;
        *) echo "unknown helper $script" >&2; return 64 ;;
    esac
}

client_env() {
    client=(env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config"
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state"
        XDG_CURRENT_DESKTOP=KDE XDG_SESSION_TYPE=wayland KDE_FULL_SESSION=true KDE_SESSION_VERSION=6
        WAYLAND_DISPLAY="$wayland_name" QT_QPA_PLATFORM=wayland QT_FORCE_STDERR_LOGGING=1
        DBUS_SESSION_BUS_ADDRESS="$session_bus" DBUS_SYSTEM_BUS_ADDRESS="$system_bus")
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

start_plasmashell() {
    local log=$1
    client_env
    # setsid: plasmashell outlives this script; in a background job it execs in place, so $! is plasmashell.
    "${client[@]}" setsid plasmashell --no-respawn > "$log" 2>&1 &
    plasma_pid=$!
    sed -i "/^plasma_pid=/d" "$root/env"
    echo "plasma_pid=$plasma_pid" >> "$root/env"
    wait_for "$ready_deadline_s" bash -c "[[ \$(\"\$@\" 2> /dev/null) == $plasma_pid ]]" _ \
        "${py[@]}" "$spike/plasma_dbus.py" pid "$session_bus" org.kde.plasmashell \
        || { echo "plasmashell never owned org.kde.plasmashell, see $log" >&2; return 1; }
}

logical_size() {
    dbus-send --bus="$session_bus" --print-reply --dest=org.kde.KWin /KWin org.kde.KWin.supportInformation 2> /dev/null \
        | grep -m 1 -o 'Geometry: [0-9,]*x[0-9]*' | sed 's/.*,//'
}

# kded's kscreen module, finding no kscreen config for the virtual output, sets its scale back to 1.
set_scale() {
    client_env
    local deadline=$((SECONDS + ready_deadline_s))
    while [[ $(logical_size) != "${width}x$height" ]]; do
        if ((SECONDS >= deadline)); then
            echo "the output never took scale $scale: $(logical_size)" >&2
            return 1
        fi
        "${client[@]}" kscreen-doctor "output.Virtual-0.scale.$scale" > /dev/null 2>&1 || true
        sleep 1
    done
}

stop_plasmashell() {
    kill "$plasma_pid"
    wait_for 20 bash -c "! kill -0 $plasma_pid 2> /dev/null" || { echo "plasmashell $plasma_pid did not stop" >&2; return 1; }
}

up() {
    if [[ -e $root ]]; then
        echo "$root exists; another run is using it or left it behind" >&2
        exit 1
    fi
    mkdir -p "$root"/{run,config,data,cache,state,services,logs}
    chmod 700 "$root/run"
    for file in "${config_files[@]}"; do
        if [[ -e $HOME/.config/$file ]]; then
            cp -- "$HOME/.config/$file" "$root/config/"
        fi
    done
    for dir in "${config_dirs[@]}"; do
        cp -r -- "$HOME/.config/$dir" "$root/config/"
    done
    for dir in "${data_dirs[@]}"; do
        cp -r -- "$HOME/.local/share/$dir" "$root/data/"
    done
    for service in "${services[@]}"; do
        ln -s -- "/usr/share/dbus-1/services/$service.service" "$root/services/"
    done
    sed "s|</busconfig>|  <servicedir>$root/services</servicedir>\n</busconfig>|" "$repo/scripts/private-bus.conf" \
        > "$root/session-bus.conf"

    dbus-daemon --config-file="$repo/scripts/private-bus.conf" --nofork --print-address=3 \
        3> "$root/system.addr" > "$root/logs/system-bus.log" 2>&1 &
    local system_pid=$!
    wait_for 10 test -s "$root/system.addr" || { echo "no private system bus" >&2; exit 1; }
    system_bus=$(head -n 1 "$root/system.addr")
    is_private_bus "$system_bus" || { echo "refusing system bus ${system_bus@Q}" >&2; exit 1; }
    # The tray loads its Bluetooth applet only while org.bluez is on the system bus. The runtime dir marks the
    # stand-in as the sandbox's for `down`.
    XDG_RUNTIME_DIR="$root/run" "${py[@]}" "$spike/plasma_dbus.py" own "$system_bus" org.bluez \
        > "$root/logs/bluez.log" 2>&1 &
    wait_for 30 grep -q 'own org.bluez: 1' "$root/logs/bluez.log" || { echo "no org.bluez stand-in" >&2; exit 1; }

    # Both permission switches hold only inside this KWin: ScreenShot2 for grab.py, fake input for fake_key.py.
    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
        XDG_CURRENT_DESKTOP=KDE XDG_SESSION_TYPE=wayland KDE_FULL_SESSION=true KDE_SESSION_VERSION=6 \
        DBUS_SYSTEM_BUS_ADDRESS="$system_bus" KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 \
        QT_FORCE_STDERR_LOGGING=1 \
        setsid dbus-run-session --config-file="$root/session-bus.conf" -- \
        sh -c 'echo $$ > "$0/kwin.pid"; echo "$DBUS_SESSION_BUS_ADDRESS" > "$0/session.addr"; exec kwin_wayland --virtual --no-lockscreen --socket "$1" --width "$2" --height "$3" --scale "$4"' \
        "$root" "$wayland_name" "$width" "$height" "$scale" > "$root/logs/kwin.log" 2>&1 &
    local session_pid=$!
    wait_for "$socket_deadline_s" test -S "$root/run/$wayland_name" || { echo "KWin made no socket" >&2; exit 1; }
    session_bus=$(< "$root/session.addr")
    is_private_bus "$session_bus" || { echo "refusing session bus ${session_bus@Q}" >&2; exit 1; }
    cat > "$root/env" <<- EOF
	session_bus=$session_bus
	system_bus=$system_bus
	system_pid=$system_pid
	session_pid=$session_pid
	kwin_pid=$(< "$root/kwin.pid")
	EOF

    # Services activated on this bus inherit the bus daemon's environment, which has no display yet.
    helper plasma_dbus env WAYLAND_DISPLAY="$wayland_name" QT_QPA_PLATFORM=wayland QT_FORCE_STDERR_LOGGING=1
    wait_for "$ready_deadline_s" helper kga keys plasmashell "activate application launcher" > /dev/null 2>&1 \
        || { echo "KGlobalAccel never answered" >&2; exit 1; }
    helper plasma_dbus start org.kde.kded6
    set_scale
    start_plasmashell "$root/logs/plasmashell-1.log"
    echo "up: $(logical_size)"
    echo "up: $(cat "$root/env" | tr '\n' ' ')"
}

down() {
    load_env
    local pids=() pid
    # The session's bus daemon and KWin go last, with dbus-run-session.
    mapfile -t pids < <(sandbox_pids | while read -r pid; do
        if [[ $pid != "$session_pid" && $(ps -o ppid= -p "$pid" | tr -d ' ') != "$session_pid" ]]; then
            echo "$pid"
        fi
    done)
    echo "stopping clients: ${pids[*]}"
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
    left=$(sandbox_pids | { grep -vxF "$$" || true; } | tr "\n" " ")
    for pid in "$system_pid"; do
        if kill -0 "$pid" 2> /dev/null; then
            left+="$pid "
        fi
    done
    if [[ -n ${left// /} ]]; then
        echo "still running, sandbox kept: $left" >&2
        exit 1
    fi
    rm -r -- "$root"
    echo "down"
}

case ${1:-} in
    up) up ;;
    down) down ;;
    shot) load_env; helper grab shot "$2" ;;
    plasma-stop) load_env; stop_plasmashell ;;
    plasma-start) load_env; start_plasmashell "$root/logs/plasmashell-$(date +%s).log"; echo "plasma_pid=$plasma_pid" ;;
    plasma-restart)
        load_env
        stop_plasmashell
        start_plasmashell "$root/logs/plasmashell-$(date +%s).log"
        echo "plasma_pid=$plasma_pid"
        ;;
    retire | restore)
        load_env
        PLASMA_STOP="$0 plasma-stop" PLASMA_START="$0 plasma-start" \
            "$spike/plasma-panel.sh" "$1" "$session_bus" "$root/config" "$root/backup"
        ;;
    popup) load_env; "$spike/popup.sh" "$session_bus" "$root/backup" "$2" ;;
    windows)
        load_env
        script_id=$(dbus-send --bus="$session_bus" --print-reply --dest=org.kde.KWin /Scripting \
            org.kde.kwin.Scripting.loadScript string:"$spike/windows.js" string:gs-windows | awk '/int32/ { print $2 }')
        from=$(grep -c '' "$root/logs/kwin.log")
        dbus-send --bus="$session_bus" --print-reply --dest=org.kde.KWin "/Scripting/Script$script_id" org.kde.kwin.Script.run > /dev/null
        sleep 0.5
        dbus-send --bus="$session_bus" --print-reply --dest=org.kde.KWin /Scripting \
            org.kde.kwin.Scripting.unloadScript string:gs-windows > /dev/null
        tail -n +"$((from + 1))" "$root/logs/kwin.log" | grep -a 'gs-windows {' | sed 's/.*gs-windows //'
        ;;
    keys)
        load_env
        # One chord per argument, evdev codes joined by "+": pressed in order, released in reverse.
        printf '%s\n' "${@:2}" | tr '+' ' ' | "${py[@]}" "$spike/../bar-bridge/fake_chord.py" "$root/run/$wayland_name"
        ;;
    strip)
        load_env
        if [[ ${2:-} == stop ]]; then
            kill "$strip_pid"
            sed -i "/^strip_pid=/d" "$root/env"
        else
            client_env
            # No QT_SCALE_FACTOR: the output itself is at the user's 1.15, so the strip is 52 logical px as is.
            "${client[@]}" GHOST_GREETING=off SPIKE_EXCLUSIVE="$([[ ${2:-} == exclusive ]] && echo 1 || echo 0)" \
                setsid qs -p "$spike/strip.qml" > "$root/logs/strip.log" 2>&1 &
            echo "strip_pid=$!" >> "$root/env"
            sleep 2
        fi
        ;;
    rss) load_env; echo "plasmashell $plasma_pid $(grep -E '^(Rss|Pss):' "/proc/$plasma_pid/smaps_rollup" | tr -s ' \n' ' ')" ;;
    do) load_env; helper "${@:2}" ;;
    *) sed -n '4,19p' "$0" >&2; exit 64 ;;
esac

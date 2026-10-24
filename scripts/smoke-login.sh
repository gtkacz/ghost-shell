#!/usr/bin/env bash
set -euo pipefail

# Installs the login part with `install.sh --login` into a temp root (stub sudo, localectl and semanage) and runs the
# real SDDM greeter on the installed theme in test mode, which has no daemon and so never reaches PAM. Checks that the
# gate loads once with no QML warning. With --shot <png> it also saves one frame of the login screen there.

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
name=smokelogin
root=/tmp/gs-sbx-$name
greeter=/usr/bin/sddm-greeter-qt6
log=$root/greeter.log
shot=
if [[ ${1:-} == --shot ]]; then
    shot=${2:?usage: smoke-login.sh [--shot <png>]}
    shot=$(realpath -m -- "$shot")
fi
real_fonts=${XDG_DATA_HOME:-$HOME/.local/share}/fonts

load_budget_s=15
poll_s=0.1
kwin_stop_s=5

# Time for the first frames to be drawn, and any warning from them logged, before the log is judged or a frame grabbed.
frame_settle_s=3

loaded_line='gate: prompt login'
warning_pattern='\.(qml|mjs):[0-9]+|ReferenceError|TypeError|Failed to load|Fallback to embedded|not installed'

kwin_pid=
greeter_pid=
# Set by a failure, so the logs it names outlive the run.
keep_root=

fail() {
    keep_root=1
    echo "smoke-login: FAIL ($1)" >&2
    exit 1
}

cleanup() {
    if [[ -n $greeter_pid ]]; then
        kill "$greeter_pid" 2> /dev/null || true
        wait "$greeter_pid" 2> /dev/null || true
    fi
    if [[ -z $kwin_pid ]]; then
        [[ -n $keep_root ]] || rm -rf -- "$root"
        return
    fi
    kill "$kwin_pid" 2> /dev/null || true
    local deadline=$((SECONDS + kwin_stop_s))
    while kill -0 "$kwin_pid" 2> /dev/null && (( SECONDS < deadline )); do
        sleep "$poll_s"
    done
    if kill -0 "$kwin_pid" 2> /dev/null; then
        echo "smoke-login: KWin $kwin_pid did not stop; $root is left in place" >&2
        return
    fi
    if [[ -n $keep_root ]]; then
        echo "smoke-login: $root is kept for its logs; remove it before the next run" >&2
        return
    fi
    rm -r -- "$root"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ -e $root ]]; then
    echo "smoke-login: FAIL ($root exists: another run is using it, or a stopped one left it behind)" >&2
    trap - EXIT
    exit 1
fi

# KWin lets a client whose desktop file names the interface take screenshots, as it does for fake input. Written before
# KWin starts, because KWin overlooks one added just before the client asks.
mkdir -p "$root/data/applications"
printf '[Desktop Entry]\nType=Application\nName=smoke-login shot\nExec=/usr/bin/spectacle\nNoDisplay=true\nX-KDE-DBUS-Restricted-Interfaces=org.kde.KWin.ScreenShot2\n' \
    > "$root/data/applications/gs-smokelogin-shot.desktop"

kwin_out=$("$repo/scripts/headless-kwin.sh" "$name")
kwin_pid=$(sed -n 's/^pid=//p' <<< "$kwin_out")
wayland=$(sed -n 's/^WAYLAND_DISPLAY=//p' <<< "$kwin_out")
# KWin is not dumpable, so its environ is closed to us; its bus is the socket its sibling dbus-daemon listens on.
bus_daemon=$(pgrep -P "$(ps -o ppid= -p "$kwin_pid" | tr -d ' ')" -x dbus-daemon)
bus=unix:path=$(ss -xlpn | awk -v p="pid=$bus_daemon," 'index($0, p) && !found { print $5; found = 1 }')

# The sandbox's own home holds no fonts, so the greeter and the installer see the user's, as the real ones do.
ln -s -- "$real_fonts" "$root/data/fonts"
sysroot=$root/sysroot
# The installer's stdin answers its keymap question with no.
env -u DBUS_SESSION_BUS_ADDRESS HOME="$root" XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
    XDG_STATE_HOME="$root/state" XDG_RUNTIME_DIR="$root/run" GHOST_ROOT="$sysroot" GHOST_SUDO= \
    GHOST_LOCALECTL=true GHOST_SEMANAGE=true GHOST_RESTORECON=true GHOST_GETENFORCE='echo Enforcing' \
    "$repo/scripts/install.sh" --login <<< n > "$root/install.log" 2>&1 \
    || fail "install.sh --login failed; see $root/install.log"
theme=$sysroot/usr/share/sddm/themes/ghost-shell
[[ -f $theme/Main.qml ]] || fail "install.sh --login put no theme in $theme"

env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" WAYLAND_DISPLAY="$wayland" \
    QT_QPA_PLATFORM=wayland DBUS_SESSION_BUS_ADDRESS="$bus" QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES='qml.debug=true' \
    "$greeter" --test-mode --theme "$theme" > "$log" 2>&1 &
greeter_pid=$!

deadline=$((SECONDS + load_budget_s))
until grep -aqF -- "$loaded_line" "$log"; do
    if (( SECONDS >= deadline )) || ! kill -0 "$greeter_pid" 2> /dev/null; then
        fail "no \"$loaded_line\" in $log within $load_budget_s s"
    fi
    sleep "$poll_s"
done
sleep "$frame_settle_s"

if [[ -n $shot ]]; then
    env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
        XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" WAYLAND_DISPLAY="$wayland" QT_QPA_PLATFORM=wayland \
        DBUS_SESSION_BUS_ADDRESS="$bus" spectacle -b -n -f -o "$shot" > "$root/spectacle.log" 2>&1 \
        || fail "spectacle failed; see $root/spectacle.log"
fi

prompts=$(grep -acF -- "$loaded_line" "$log" || true)
(( prompts == 1 )) || fail "$prompts \"$loaded_line\" lines in $log, expected 1"
warnings=$(grep -aE -- "$warning_pattern" "$log" || true)
[[ -z $warnings ]] || fail "warnings in $log:"$'\n'"$warnings"

echo "smoke-login: PASS"

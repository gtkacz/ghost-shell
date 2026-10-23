#!/usr/bin/env bash
set -euo pipefail

# Lock plan A Task 1: runs kscreenlocker_greet and sddm-greeter-qt6 against the probes in this directory, each
# inside scripts/headless-kwin.sh's headless KWin and private bus, and prints every "SPIKE" line they log.
# Never starts PAM: the lock probe does not call startAuthenticating and SDDM's test mode has no daemon.

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$spike/../.." && pwd)
name=lockspike
root=/tmp/gs-sbx-$name
run_s=6
package=org.ghostshell.lockspike
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project python -I -B)
keys_after_s=3
# evdev: Caps Lock, A, Caps Lock, so Caps Lock ends off. Taps only: no greeter here has started PAM.
caps_a_caps="58 30 58"

mkdir -p "$root/config"
# KWin reads its layouts once at start, so they go in before it runs.
cp -- "$HOME/.config/kxkbrc" "$root/config/kxkbrc"
kwin_out=$("$repo/scripts/headless-kwin.sh" "$name")
wayland=$(sed -n 's/^WAYLAND_DISPLAY=//p' <<< "$kwin_out")
kwin_pid=$(sed -n 's/^pid=//p' <<< "$kwin_out")
trap 'kill "$kwin_pid" 2> /dev/null || true' EXIT
# KWin is not dumpable, so its environ is closed to us; its bus is the socket its sibling dbus-daemon listens on.
bus_daemon=$(pgrep -P "$(ps -o ppid= -p "$kwin_pid" | tr -d ' ')" -x dbus-daemon)
bus=unix:path=$(ss -xlpn | awk -v p="pid=$bus_daemon," 'index($0, p) && !found { print $5; found = 1 }')
echo "=== private bus: $bus"

broken_import='import org.ghostshell.nosuchmodule'
stage_package() { # <id> <ok|outdated|broken>
    local dir=$root/data/plasma/shells/$1
    rm -rf -- "$dir"
    mkdir -p "$dir"
    cp -r -- "$spike/lock/$package/." "$dir/"
    cp -r -- "$spike/GhostGate" "$dir/contents/lockscreen/GhostGate"
    sed -i "s/\"Id\": \"$package\"/\"Id\": \"$1\"/" "$dir/metadata.json"
    if [[ $2 == outdated ]]; then
        sed -i -e '/X-Plasma-APIVersion/d' -e 's/},$/}/' "$dir/metadata.json"
    elif [[ $2 == broken ]]; then
        sed -i "1i $broken_import" "$dir/contents/lockscreen/LockScreen.qml"
    fi
}
stage_package "$package" ok
stage_package "$package-old" outdated
stage_package "$package-broken" broken
mkdir -p "$root/state/ghost-shell" "$root/var-lib-ghost-shell"
printf '[gate]\nstyle=terminal\n' > "$root/state/ghost-shell/gate.ini"
printf '[gate]\nstyle=idcheck\n' > "$root/var-lib-ghost-shell/gate.ini"
rm -rf -- "$root/sddm-theme"
cp -r -- "$spike/sddm" "$root/sddm-theme"
cp -r -- "$spike/GhostGate" "$root/sddm-theme/GhostGate"
rm -rf -- "$root/sddm-theme-broken"
cp -r -- "$root/sddm-theme" "$root/sddm-theme-broken"
sed -i "1i $broken_import" "$root/sddm-theme-broken/Main.qml"
printf '[General]\nfromUserFile=yes\nstandInGate=%s\n' "$root/var-lib-ghost-shell/gate.ini" > "$root/sddm-theme/theme.conf.user"
before=$(sha256sum "$root/state/ghost-shell/gate.ini" "$root/var-lib-ghost-shell/gate.ini")

greeter_env=(env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config"
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" WAYLAND_DISPLAY="$wayland"
    QT_QPA_PLATFORM=wayland DBUS_SESSION_BUS_ADDRESS="$bus" QT_FORCE_STDERR_LOGGING=1
    QT_LOGGING_RULES='*.debug=false;qml.debug=true;js.debug=true;kscreenlocker_greet.debug=true')

# headless-kwin.sh starts KWin under env -i, so KWIN_WAYLAND_NO_PERMISSION_CHECKS cannot reach it. KWin instead
# grants fake input to an executable that a desktop file in its (sandboxed) data dir names with the interface.
python_exe=$("${py[@]}" -c 'import os, sys; print(os.path.realpath(sys.executable))')
mkdir -p "$root/data/applications"
printf '[Desktop Entry]\nType=Application\nName=lock spike fake keys\nExec=%s\nNoDisplay=true\nX-KDE-Wayland-Interfaces=org_kde_kwin_fake_input\n' \
    "$python_exe" > "$root/data/applications/gs-lockspike-fake-keys.desktop"

mkdir -p "$root/logs"
runs=0
run() { # <label> <command…>
    runs=$((runs + 1))
    local log=$root/logs/$runs.log
    echo "=== $1 (full log: $log)"
    shift
    timeout -k 2 "$run_s" "$@" > "$log" 2>&1 &
    local greeter=$!
    if [[ -n ${keys:-} ]]; then
        sleep "$keys_after_s"
        printf '%s\n' $keys | "${py[@]}" "$spike/../bar-bridge/fake_chord.py" "$wayland"
    fi
    wait "$greeter" || true
    grep -E 'SPIKE|outdated|Failed|fallback|Fallback|rror|Loading|not installed|not found' "$log" || true
}

keys=$caps_a_caps run "lock: --testing --shell $package, then keys $caps_a_caps" "${greeter_env[@]}" \
    /usr/libexec/kscreenlocker_greet --testing --shell "$package"
run "lock: --testing, PLASMA_DEFAULT_SHELL=$package" "${greeter_env[@]}" PLASMA_DEFAULT_SHELL="$package" \
    /usr/libexec/kscreenlocker_greet --testing
run "lock: --testing, PLASMA_DEFAULT_SHELL=<absolute path>" "${greeter_env[@]}" \
    PLASMA_DEFAULT_SHELL="$root/data/plasma/shells/$package" /usr/libexec/kscreenlocker_greet --testing
printf '[Shell]\nShellPackage=%s\n' "$package" > "$root/config/plasmashellrc"
run "lock: --testing, plasmashellrc [Shell] ShellPackage=$package, PLASMA_DEFAULT_SHELL=$package-old" \
    "${greeter_env[@]}" PLASMA_DEFAULT_SHELL="$package-old" /usr/libexec/kscreenlocker_greet --testing
rm -- "$root/config/plasmashellrc"
run "lock: --testing --shell $package-old (no X-Plasma-APIVersion)" "${greeter_env[@]}" \
    /usr/libexec/kscreenlocker_greet --testing --shell "$package-old"
run "lock: --testing, PLASMA_DEFAULT_SHELL=org.ghostshell.notinstalled" "${greeter_env[@]}" \
    PLASMA_DEFAULT_SHELL=org.ghostshell.notinstalled /usr/libexec/kscreenlocker_greet --testing
run "lock: --testing --shell $package-broken (QML fails to load)" "${greeter_env[@]}" \
    /usr/libexec/kscreenlocker_greet --testing --shell "$package-broken"
keys=$caps_a_caps run "sddm: --test-mode --theme $root/sddm-theme, then keys $caps_a_caps" "${greeter_env[@]}" \
    /usr/bin/sddm-greeter-qt6 --test-mode --theme "$root/sddm-theme"
run "sddm: --test-mode --theme $root/sddm-theme-broken (QML fails to load)" "${greeter_env[@]}" \
    /usr/bin/sddm-greeter-qt6 --test-mode --theme "$root/sddm-theme-broken"

after=$(sha256sum "$root/state/ghost-shell/gate.ini" "$root/var-lib-ghost-shell/gate.ini")
echo "=== gate.ini files unchanged by the readers: $([[ $before == "$after" ]] && echo yes || echo NO)"

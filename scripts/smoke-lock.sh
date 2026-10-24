#!/usr/bin/env bash
set -euo pipefail

# Installs the lock part into a headless KWin's sandbox with `install.sh --lock` and runs the real lock greeter on it
# in test mode on two screens. Checks that the gate loads once with no QML warning, that one screen alone starts PAM,
# and that the greeter takes typed keys, wherever KWin first sends them, by sending
# exactly one wrong password through the greeter's real PAM stack, which must answer with one failure. Never more
# than one: on a machine with pam_faillock repeated failures lock the account. The success path is left to the gate's
# fake-adapter test and to the user, since it would need the real password.

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
name=smokelock
root=/tmp/gs-sbx-$name
package=org.ghostshell.lock
greeter=/usr/libexec/kscreenlocker_greet
log=$root/greeter.log
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project python -I -B)
fake_keys=$repo/scripts/fake-keys.py
# Two screens, so the check covers what only one may do: show the prompt and drive the shared authenticator.
outputs=2

# evdev codes: a nonsense password, then Enter.
wrong_password_keys="16 44 45 36 16 44 45 36"
enter_key=28
load_budget_s=15
# pam_unix waits about 2 s before it reports a wrong password.
failure_budget_s=15
# The greeter asks KWin for the keyboard after its first frame, a moment after the gate loads.
focus_settle_s=1
poll_s=0.1
kwin_stop_s=5

loaded_line='gate: prompt lock'
failed_line='gate: failed 1'
# kscreenlocker's debug line for each password handed to PAM.
responded_line='PamAuthenticators: responding to interactive authenticator'
started_line='gate: start authenticating'
warning_pattern='\.(qml|mjs):[0-9]+|ReferenceError|TypeError|outdated|falling back|Failed to load|Unlocked'

kwin_pid=
greeter_pid=
# Set by a failure, so the logs it names outlive the run.
keep_root=

fail() {
    keep_root=1
    echo "smoke-lock: FAIL ($1)" >&2
    exit 1
}

stop_greeter() {
    if [[ -n $greeter_pid ]]; then
        kill "$greeter_pid" 2> /dev/null || true
        wait "$greeter_pid" 2> /dev/null || true
        greeter_pid=
    fi
}

cleanup() {
    stop_greeter
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
        echo "smoke-lock: KWin $kwin_pid did not stop; $root is left in place" >&2
        return
    fi
    if [[ -n $keep_root ]]; then
        echo "smoke-lock: $root is kept for its logs; remove it before the next run" >&2
        return
    fi
    rm -r -- "$root"
}

wait_for_line() { # <fixed string> <seconds>
    local deadline=$((SECONDS + $2))
    until grep -aqF -- "$1" "$log"; do
        if (( SECONDS >= deadline )) || ! kill -0 "$greeter_pid" 2> /dev/null; then
            return 1
        fi
        sleep "$poll_s"
    done
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if [[ -e $root ]]; then
    echo "smoke-lock: FAIL ($root exists: another run is using it, or a stopped one left it behind)" >&2
    trap - EXIT
    exit 1
fi

# KWin grants fake input to an executable that a desktop file in its (sandboxed) data dir names with the interface.
# Written before KWin starts, because KWin overlooks one added just before the client asks.
python_exe=$("${py[@]}" -c 'import os, sys; print(os.path.realpath(sys.executable))')
mkdir -p "$root/data/applications"
printf '[Desktop Entry]\nType=Application\nName=smoke-lock keys\nExec=%s\nNoDisplay=true\nX-KDE-Wayland-Interfaces=org_kde_kwin_fake_input\n' \
    "$python_exe" > "$root/data/applications/gs-smokelock-keys.desktop"

kwin_out=$("$repo/scripts/headless-kwin.sh" "$name" "$outputs")
kwin_pid=$(sed -n 's/^pid=//p' <<< "$kwin_out")
wayland=$(sed -n 's/^WAYLAND_DISPLAY=//p' <<< "$kwin_out")
# KWin is not dumpable, so its environ is closed to us; its bus is the socket its sibling dbus-daemon listens on.
bus_daemon=$(pgrep -P "$(ps -o ppid= -p "$kwin_pid" | tr -d ' ')" -x dbus-daemon)
bus=unix:path=$(ss -xlpn | awk -v p="pid=$bus_daemon," 'index($0, p) && !found { print $5; found = 1 }')

# The sandbox's runtime dir and no session bus keep the installer's systemctl away from the user's manager.
env -u DBUS_SESSION_BUS_ADDRESS HOME="$root" XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
    XDG_STATE_HOME="$root/state" XDG_RUNTIME_DIR="$root/run" "$repo/scripts/install.sh" --lock > "$root/install.log" 2>&1 \
    || fail "install.sh --lock failed; see $root/install.log"
[[ -f $root/data/plasma/shells/$package/contents/lockscreen/LockScreen.qml ]] \
    || fail "install.sh --lock put no package in $root/data/plasma/shells"

env -i HOME="$root" PATH=/usr/bin:/bin XDG_RUNTIME_DIR="$root/run" XDG_CONFIG_HOME="$root/config" \
    XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" WAYLAND_DISPLAY="$wayland" \
    QT_QPA_PLATFORM=wayland DBUS_SESSION_BUS_ADDRESS="$bus" QT_FORCE_STDERR_LOGGING=1 QT_LOGGING_RULES='kscreenlocker_greet.debug=true;qml.debug=true' \
    "$greeter" --testing --shell "$package" > "$log" 2>&1 &
greeter_pid=$!

wait_for_line "$loaded_line" "$load_budget_s" || fail "no \"$loaded_line\" in $log within $load_budget_s s"
sleep "$focus_settle_s"
printf '%s\n' $wrong_password_keys "$enter_key" | "${py[@]}" "$fake_keys" "$wayland" > "$root/keys.log" 2>&1 \
    || fail "typing failed; see $root/keys.log"
wait_for_line "$failed_line" "$failure_budget_s" || fail "no \"$failed_line\" in $log within $failure_budget_s s"
# Stopped before its retry starts a new PAM conversation, which the kill would end as one more failure in PAM's eyes.
stop_greeter

responses=$(grep -acF -- "$responded_line" "$log" || true)
(( responses == 1 )) || fail "$responses passwords reached PAM in $log, expected 1"
starts=$(grep -acF -- "$started_line" "$log" || true)
(( starts == 1 )) || fail "$starts \"$started_line\" lines in $log, expected 1"
prompts=$(grep -acF -- "$loaded_line" "$log" || true)
(( prompts == 1 )) || fail "$prompts \"$loaded_line\" lines in $log, expected 1 on $outputs screens"
failures=$(grep -acF -- 'gate: failed' "$log" || true)
(( failures == 1 )) || fail "$failures failure lines in $log, expected 1"
if grep -aqF -- 'gate: succeeded' "$log"; then
    fail "the wrong password unlocked; see $log"
fi
warnings=$(grep -aE -- "$warning_pattern" "$log" || true)
[[ -z $warnings ]] || fail "warnings in $log:"$'\n'"$warnings"

echo "smoke-lock: PASS"

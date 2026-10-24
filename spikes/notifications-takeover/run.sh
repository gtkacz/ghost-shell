#!/usr/bin/env bash
set -euo pipefail

# Plan 3b Task 1 end to end, in plasma-hidden's sandbox under its own name: a stand-in notification server (claim.py)
# queues for org.freedesktop.Notifications behind the sandbox plasmashell and inherits it when plasmashell restarts,
# Plasma's job popups are checked with a claimant in place, and units.sh checks the login order on unit copies.
# Everything runs on the sandbox's private buses; nothing is sent to the user's session.
#   SPIKE_OUT   directory for results and screenshots, kept afterwards (default: a new temp dir)

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export SANDBOX_NAME=3b-t1
root=/tmp/gs-sbx-$SANDBOX_NAME
out=${SPIKE_OUT:-$(mktemp -d -t notifications-takeover.XXXXXX)}
results=$out/results.txt

sbx=("$spike/../plasma-hidden/sandbox.sh")
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)
settle_s=20
# Plasma never retries a failed registration, so waiting longer than the brief's 2 minutes proves nothing more.
hold_s=120
release_wait_s=30
# REPLACE_EXISTING only: no ALLOW_REPLACEMENT, no DO_NOT_QUEUE.
claim_flags=2
# evdev codes: Up, Down, Enter.
key_up=108
key_down=103
key_enter=28

claim_pid=
job_pid=
session_bus=

note() {
    echo "$*" | tee -a "$results"
}

stop_pids() {
    local pid
    for pid in "$@"; do
        if [[ -n $pid ]]; then
            kill "$pid" 2> /dev/null || true
            wait "$pid" 2> /dev/null || true
        fi
    done
}

cleanup() {
    stop_pids "$claim_pid" "$job_pid"
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

bus_call() {
    busctl --address "$session_bus" call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus "$@" 2>&1
}

owner() {
    bus_call GetNameOwner s org.freedesktop.Notifications | sed 's/^s //'
}

owner_pid() {
    "${sbx[@]}" do plasma_dbus pid org.freedesktop.Notifications | tail -n 1
}

notify() {
    env -i PATH=/usr/bin DBUS_SESSION_BUS_ADDRESS="$session_bus" notify-send "$@" 2>&1 | tail -n 1 || true
}

start_claim() {
    "${py[@]}" "$spike/claim.py" "$session_bus" "$claim_flags" > "$1" 2>&1 &
    claim_pid=$!
    sleep 4
}

restart_plasma() {
    "${sbx[@]}" plasma-restart | tail -n 1 | tee -a "$results"
    sleep "$settle_s"
    source_env
}

popup_windows() {
    "${sbx[@]}" windows | grep -v '"desktopWindow":true' \
        | sed -E 's/.*"resourceClass":"([^"]*)","caption":"([^"]*)".*"x":([0-9.]+),"y":([0-9.]+),"width":([0-9.]+),"height":([0-9.]+).*/\1 "\2" at \3,\4 \5x\6/'
}

failed_registrations() {
    grep -ac 'Failed to register Notification service on DBus' "$root"/logs/plasmashell-*.log | sed "s|$root/logs/||" | paste -sd ' '
}

: > "$results"
note "out=$out"
"${sbx[@]}" up | tee -a "$results"
source_env
note "session bus $session_bus (the shell's own: ${DBUS_SESSION_BUS_ADDRESS:-unset})"
sleep "$settle_s"

note "== question 1: inheritance"
note "plasmashell $plasma_pid owns: Notifications pid $(owner_pid), JobViewServer pid $("${sbx[@]}" do plasma_dbus pid org.kde.JobViewServer | tail -n 1), kuiserver pid $("${sbx[@]}" do plasma_dbus pid org.kde.kuiserver | tail -n 1)"
start_claim "$out/claim-1.log"
note "claimant: $(cat "$out/claim-1.log")"
note "owner now $(owner) (plasmashell); queued: $(bus_call ListQueuedOwners s org.freedesktop.Notifications)"
restart_plasma
note "claimant after restart 1: $(paste -sd ';' "$out/claim-1.log")"
note "owner $(owner), plasmashell $plasma_pid, JobViewServer pid $("${sbx[@]}" do plasma_dbus pid org.kde.JobViewServer | tail -n 1)"
note "plasmashell registration failures per log: $(failed_registrations)"
acquired_at=$SECONDS
notify "to claimant" "after restart 1"
sleep 1
note "claimant log tail: $(tail -n 1 "$out/claim-1.log")"

"${sbx[@]}" retire | tail -n 3 | tee -a "$results"
source_env
sleep "$settle_s"
note "after retire (restart 2): owner $(owner)"
"${sbx[@]}" popup tray
sleep 2
"${sbx[@]}" keys "$key_up" "$key_down" "$key_enter" > /dev/null
sleep 2
"${sbx[@]}" shot "$out/01-tray-notifications-page.png" > /dev/null
"${sbx[@]}" popup tray
while ((SECONDS - acquired_at < hold_s)); do
    sleep 5
done
note "$((SECONDS - acquired_at)) s after the claimant took the name: owner $(owner)"
restart_plasma
note "after restart 3: owner $(owner), plasmashell $plasma_pid"
notify "to claimant" "after restart 3"
sleep 1
note "claimant log tail: $(tail -n 1 "$out/claim-1.log")"

note "== question 1: release"
kill -USR1 "$claim_pid"
sleep "$release_wait_s"
note "claimant: $(tail -n 1 "$out/claim-1.log"); owner $(owner)"
restart_plasma
note "after restart 4: owner pid $(owner_pid), plasmashell $plasma_pid"
notify "to plasma again" "served"
sleep 3
note "toast: $(popup_windows)"
"${sbx[@]}" shot "$out/02-plasma-serves-again.png" > /dev/null
stop_pids "$claim_pid"

note "== question 2: jobs"
start_claim "$out/claim-2.log"
restart_plasma
note "claimant: $(paste -sd ';' "$out/claim-2.log"); owner pid $(owner_pid), plasmashell $plasma_pid"
run_job() {
    "${py[@]}" "$spike/job.py" "$session_bus" > "$out/job-$1.log" 2>&1 &
    job_pid=$!
    sleep 5
    note "$1: $(paste -sd ';' "$out/job-$1.log")"
    "${sbx[@]}" shot "$out/$2" > /dev/null
    note "$1 popup: $(popup_windows)"
    stop_pids "$job_pid"
    job_pid=
}
run_job default 03-job-default.png
env -i HOME="$root" PATH=/usr/bin XDG_CONFIG_HOME="$root/config" \
    kwriteconfig6 --file plasmanotifyrc --group Notifications --key PopupPosition BottomRight
note "plasmanotifyrc: $(grep -h PopupPosition "$root/config/plasmanotifyrc")"
restart_plasma
note "after restart 5: owner pid $(owner_pid), plasmashell $plasma_pid"
run_job bottom-right 04-job-bottom-right.png

note "== question 3: login order"
"$spike/units.sh" 2>&1 | tee -a "$results"

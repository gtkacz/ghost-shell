#!/usr/bin/env bash
set -euo pipefail

# Sandbox test for the notification subcommands of scripts/plasma-panel.sh: the plasmashell of test-plasma-panel.sh's
# sandbox (a headless KWin on private buses) plus a dev ghostd on the same session bus, with every systemd action
# replaced by a sandbox-local stub, so nothing reaches the user's manager or session bus.
# Cycle 1 (top bar): retire-notifications, the name's owner, status, a notify-send landing in ghostd's topic, a second
# retire refused, restore-notifications, and plasmanotifyrc byte-identical. A Meta+<letter> key held by another
# component is refused before any change. Cycle 2 (bottom bar, no plasmanotifyrc, a Meta+Shift+N the check cannot
# parse): restore_with_ghostd_stopped stops ghostd after the retire and still gives the name back to plasmashell.
#   test-plasma-notifications.sh        run the test; it builds and removes its own sandbox
#   TEST_OUT   directory for results, kept afterwards (default: a new temp dir)

SANDBOX_NAME=3b-t8
# shellcheck source=test-plasma-panel.sh
source "$(dirname "${BASH_SOURCE[0]}")/test-plasma-panel.sh"
notifications_test=$spike/$(basename "${BASH_SOURCE[0]}")

ghostd_bin=$repo/target/debug/ghostd
ghostd_socket=$root/run/ghostd.sock
ghostd_config=$root/ghostd-config.toml
# What the script reads for bar.position and shortcuts.notifications; ghostd keeps its own copy of the defaults.
script_config=$root/config/ghost-shell/config.toml
drop_in_dir=$root/config/systemd/user/plasma-plasmashell.service.d
drop_in=$drop_in_dir/ghost-shell-notifications.conf
marker=$root/state/ghost-shell/notifications-server
notifyrc=$root/config/plasmanotifyrc
tap_s=6
notify_summary="t8 summary"
# Qt::MetaModifier + Qt::Key_J, for the key another component holds.
held_letter_key=$((0x10000000 + 0x4A))
ghostd_log_deadline_s=30

owner_pid() {
    sbx_busctl call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s \
        org.freedesktop.Notifications 2> /dev/null | awk '{ print $2 }'
}

ghostd_pid() {
    if ghostd_alive; then cat "$root/ghostd.pid"; else echo 0; fi
}

ghostd_alive() {
    [[ -s $root/ghostd.pid ]] && kill -0 "$(< "$root/ghostd.pid")" 2> /dev/null
}

ghostd_stop() {
    if ghostd_alive; then
        local pid
        pid=$(< "$root/ghostd.pid")
        kill "$pid"
        wait_for 20 bash -c "! kill -0 $pid 2> /dev/null"
    fi
}

# Returns once ghostd has chosen what to do with the name, as systemd's Type=notify start returns at READY=1.
ghostd_start() {
    local log
    load_env
    client_env
    log=$root/logs/ghostd-$(date +%s%N).log
    mkdir -p "$root/pipewire"
    "${client[@]}" PIPEWIRE_REMOTE=/nonexistent PIPEWIRE_RUNTIME_DIR="$root/pipewire" setsid "$ghostd_bin" run --socket "$ghostd_socket" --config "$ghostd_config" \
        --widgets-dir "$repo/widgets" > "$log" 2>&1 &
    echo $! > "$root/ghostd.pid"
    wait_for "$ghostd_log_deadline_s" grep -q 'notifications: ' "$log" || { echo "ghostd said nothing about notifications, see $log" >&2; return 1; }
}

latest_ghostd_log() {
    ls -t "$root"/logs/ghostd-*.log | head -n 1
}

make_notification_stubs() {
    local stub=$root/stub-bin
    printf '#!/bin/sh\necho reload >> "%s/reloads"\n' "$root" > "$stub/systemd-reload"
    printf '#!/bin/sh\nif [ -e "%s/faults/ghostd-type-old" ]; then echo simple; else echo notify; fi\n' "$root" > "$stub/ghostd-type"
    for mode in restart try-restart pid; do
        printf '#!/bin/sh\nSANDBOX_ROOT="%s" SANDBOX_NAME="%s" exec "%s" __ghostd-%s\n' \
            "$root" "$name" "$notifications_test" "$mode" > "$stub/ghostd-$mode"
    done
    chmod +x "$stub"/*
}

# The script with the sandbox's systemd stand-ins as well as the panel test's.
run_script() {
    env -i HOME="$root" PATH="$root/stub-bin:/usr/bin:/bin" XDG_RUNTIME_DIR="$root/run" \
        XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state" \
        DBUS_SESSION_BUS_ADDRESS="$session_bus" DBUS_SYSTEM_BUS_ADDRESS=unix:path=/nonexistent \
        GHOST_PLASMA_STOP="$root/stub-bin/plasma-stop" GHOST_PLASMA_START="$root/stub-bin/plasma-start" \
        GHOST_PLASMA_CHECK="$root/stub-bin/plasma-check" GHOST_DRM_DIR="$root/drm" \
        GHOST_SYSTEMD_RELOAD="$root/stub-bin/systemd-reload" GHOST_GHOSTD_RESTART="$root/stub-bin/ghostd-restart" \
        GHOST_GHOSTD_TRY_RESTART="$root/stub-bin/ghostd-try-restart" GHOST_GHOSTD_PID_CHECK="$root/stub-bin/ghostd-pid" GHOST_GHOSTD_TYPE_CHECK="$root/stub-bin/ghostd-type" \
        "$script" "$@"
}

status_lines() {
    run_script status | paste -sd '|'
}

reload_count() {
    if [[ -e $root/reloads ]]; then wc -l < "$root/reloads"; else echo 0; fi
}

# A notification sent to the sandbox bus reaches ghostd's topic, which a UI client on its socket reads.
notification_reaches_topic() {
    local tap=$out/tap.txt
    env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project python -I -B \
        "$repo/scripts/topic-tap.py" "$ghostd_socket" "$tap_s" notifications > "$tap" 2>&1 &
    local tap_pid=$!
    sleep 2
    env -i PATH=/usr/bin:/bin DBUS_SESSION_BUS_ADDRESS="$session_bus" timeout 40 notify-send -a gs-t8 "$notify_summary" "body"
    wait "$tap_pid" || true
    grep -q "$notify_summary" "$tap" && grep -q '"server":"ghost-shell"' "$tap"
}

name_belongs_to() {
    [[ $(owner_pid) == "$1" ]]
}

failure_free_cycle() {
    local ghostd_before plasma_now rc_before=$root/before/plasmanotifyrc

    note "== before: Plasma serves, ghostd is up without the marker"
    check "status before" test "$(status_lines)" == "plasma panel in place|plasma serving notifications"
    check "plasmashell owns the name" name_belongs_to "$plasma_pid"
    check "ghostd saw no marker" grep -q 'notifications: plasma serving (no marker)' "$(latest_ghostd_log)"
    check "ghostd holds Meta+N, so the key check must let ghost-shell through" bash -c \
        'grep -q "^ghost-shell/" <<< "$1"' _ "$(holders_of $((0x10000000 + 0x4E)))"

    note "== a Meta+<letter> another component holds is refused, with nothing changed"
    mkdir -p "$root/config/ghost-shell"
    printf '[shortcuts]\nnotifications = "Meta+J"\n' > "$script_config"
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel doRegister as 4 ghost-shell-test held-letter \
        ghost-shell-test "Held letter" > /dev/null
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel setForeignShortcutKeys 'asa(ai)' 4 \
        ghost-shell-test held-letter "" "" 1 4 "$held_letter_key" 0 0 0
    try retire-notifications
    sbx_busctl call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel unregister ss ghost-shell-test held-letter > /dev/null
    rm -- "$script_config"
    check "retire-notifications refuses a taken key" test "$script_rc" -ne 0
    check "the refusal names the key and its holder" bash -c 'grep -q Meta+J "$1" && grep -q held-letter "$1"' _ "$out/last.txt"
    check "no retired line" test "$(status_lines)" == "plasma panel in place|plasma serving notifications"
    check "no marker and no drop-in" bash -c '[[ ! -e $1 && ! -e $2 ]]' _ "$marker" "$drop_in"
    check "plasmanotifyrc is untouched" cmp -s "$rc_before" "$notifyrc"
    check "nothing was reloaded" test "$(reload_count)" -eq 0

    note "== an installed ghostd.service that is not Type=notify is refused, with nothing changed"
    touch "$root/faults/ghostd-type-old"
    try retire-notifications
    rm -- "$root/faults/ghostd-type-old"
    check "retire-notifications refuses an old unit" test "$script_rc" -ne 0
    check "the refusal says to reinstall" grep -q 'reinstall ghost-shell first (scripts/install.sh)' "$out/last.txt"
    check "no marker, drop-in or backup" bash -c '[[ ! -e $1 && ! -e $2 && ! -e $3 ]]' _ "$marker" "$drop_in" "$root/state/ghost-shell/plasma-backup"
    check "plasmanotifyrc is untouched and nothing was reloaded" bash -c 'cmp -s "$1" "$2" && [[ $3 -eq 0 ]]' _ "$rc_before" "$notifyrc" "$(reload_count)"

    note "== retire-notifications (top bar)"
    ghostd_before=$(ghostd_pid)
    try retire-notifications
    check "retire-notifications succeeds" test "$script_rc" -eq 0
    check "the name belongs to the sandbox ghostd" name_belongs_to "$(ghostd_pid)"
    check "ghostd was restarted" test "$(ghostd_pid)" != "$ghostd_before"
    check "the output prints the owner" grep -q "belongs to ghostd (pid $(ghostd_pid))" "$out/last.txt"
    check "ghostd owns the name by its own log" grep -q 'notifications: owning org.freedesktop.Notifications' "$(latest_ghostd_log)"
    check "status shows both lines" test "$(status_lines)" == "plasma panel in place|notifications retired $(ls "$root/state/ghost-shell/plasma-backup" | tail -n 1)"
    check "the marker exists" test -e "$marker"
    check "the drop-in is the packaged file" cmp -s "$repo/packaging/systemd/plasma-plasmashell-after-ghostd.conf" "$drop_in"
    check "the user manager was reloaded once" test "$(reload_count)" -eq 1
    check "Plasma's popups go to the bottom right under a top bar" test "$(kreadconfig6 --file "$notifyrc" --group Notifications --key PopupPosition)" == BottomRight
    check "plasmashell runs" name_owned org.kde.plasmashell
    check "a notify-send reaches ghostd's notifications topic" notification_reaches_topic

    note "== a second retire-notifications"
    check "a second retire is refused" refuses run_script retire-notifications
    check "the name still belongs to ghostd" name_belongs_to "$(ghostd_pid)"

    note "== restore-notifications"
    try restore-notifications
    plasma_now=$plasma_pid
    check "restore-notifications succeeds" test "$script_rc" -eq 0
    check "plasmashell owns the name again" name_belongs_to "$plasma_now"
    check "status is back to Plasma serving" test "$(status_lines)" == "plasma panel in place|plasma serving notifications"
    check "no marker" test ! -e "$marker"
    check "no drop-in and no empty drop-in directory" test ! -e "$drop_in_dir"
    check "plasmanotifyrc is byte-identical" cmp -s "$rc_before" "$notifyrc"
    check "ghostd was restarted and let go of the name" grep -q 'notifications: plasma serving (no marker)' "$(latest_ghostd_log)"
    check "the user manager was reloaded again" test "$(reload_count)" -eq 2
    check "restore-notifications without a retire is refused" refuses run_script restore-notifications
}

# A fault is a file in $root/faults that the stub consumes, so the failure lands exactly where the test wants it.
fault_ghostd_restart() {
    if [[ -e $root/faults/ghostd-restart-fails ]]; then
        rm -- "$root/faults/ghostd-restart-fails"
        exit 1
    fi
}

retire_that_fails_after_plasmashell_stopped() {
    note "== a retire whose ghostd restart fails: plasmashell comes back, the hint names the restore, the restore undoes it"
    touch "$root/faults/ghostd-restart-fails"
    try retire-notifications
    check "retire-notifications fails" test "$script_rc" -ne 0
    check "the failure names restore-notifications" grep -qF "$(printf '%q' "$script") restore-notifications" "$out/last.txt"
    check "plasmashell was started again" wait_for "$ready_deadline_s" name_owned org.kde.plasmashell
    check "status says retired" test "$(status_lines | cut -d'|' -f2 | cut -d' ' -f1-2)" == "notifications retired"
    try restore-notifications
    check "restore-notifications succeeds" test "$script_rc" -eq 0
    check "plasmashell owns the name" name_belongs_to "$plasma_pid"
    check "status is back to Plasma serving" test "$(status_lines)" == "plasma panel in place|plasma serving notifications"
    check "plasmanotifyrc is byte-identical" cmp -s "$root/before/plasmanotifyrc" "$notifyrc"
    check "no marker and no drop-in" bash -c '[[ ! -e $1 && ! -e $2 ]]' _ "$marker" "$drop_in_dir"
}

restore_with_ghostd_stopped() {
    note "== restore_with_ghostd_stopped (bottom bar, no plasmanotifyrc, a shortcut the check cannot parse)"
    rm -- "$notifyrc"
    printf '[bar]\nposition = "bottom"\n\n[shortcuts]\nnotifications = "Meta+Shift+N"\n' > "$script_config"
    try retire-notifications
    check "retire-notifications succeeds" test "$script_rc" -eq 0
    check "the check skipped the unparsable shortcut with a warning" grep -q 'not Meta+<letter>' "$out/last.txt"
    check "the name belongs to ghostd" name_belongs_to "$(ghostd_pid)"
    check "Plasma's popups go to the top right under a bottom bar" test "$(kreadconfig6 --file "$notifyrc" --group Notifications --key PopupPosition)" == TopRight
    ghostd_stop
    note "ghostd stopped; the name's owner now: '$(owner_pid)'"
    check "nobody owns the name" test -z "$(owner_pid)"
    try restore-notifications
    check "restore-notifications succeeds with ghostd stopped" test "$script_rc" -eq 0
    check "plasmashell owns the name" name_belongs_to "$plasma_pid"
    check "ghostd stays stopped" test "$(ghostd_pid)" -eq 0
    check "status is back to Plasma serving" test "$(status_lines)" == "plasma panel in place|plasma serving notifications"
    check "no marker" test ! -e "$marker"
    check "no drop-in and no empty drop-in directory" test ! -e "$drop_in_dir"
    check "plasmanotifyrc is gone again, as before the retire" test ! -e "$notifyrc"
    check "a notify-send now reaches Plasma, not nobody" env -i PATH=/usr/bin:/bin DBUS_SESSION_BUS_ADDRESS="$session_bus" timeout 20 notify-send -a gs-t8 "to plasma"
}

run_notifications_test() {
    out=${TEST_OUT:-$(mktemp -d -t test-plasma-notifications.XXXXXX)}
    : > "$out/results.txt"
    trap down EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    note "out=$out"
    up
    make_notification_stubs
    printf '[Notifications]\nPopupTimeout=7000\n' > "$notifyrc"
    "$ghostd_bin" default-config > "$ghostd_config"
    sleep "$settle_s"
    cp -p -- "$notifyrc" "$root/before/plasmanotifyrc"
    ghostd_start
    note "isolation: ghostd runs on the sandbox bus $session_bus (inherited: ${inherited_bus@Q})"
    check "ghostd cannot reach the user's PipeWire" bash -c 'tr "\0" "\n" < /proc/$1/environ | grep -qxF PIPEWIRE_REMOTE=/nonexistent && tr "\0" "\n" < /proc/$1/environ | grep -qxF "PIPEWIRE_RUNTIME_DIR=$2"' _ "$(ghostd_pid)" "$root/pipewire"

    failure_free_cycle
    retire_that_fails_after_plasmashell_stopped
    restore_with_ghostd_stopped

    check "the live systemctl path was never taken" test ! -e "$root/live-path-used"
    note "failures: $failures"
    ((failures == 0))
}

case ${1:-} in
    __ghostd-restart) inherited_bus= ; load_env; fault_ghostd_restart; ghostd_stop; ghostd_start ;;
    __ghostd-try-restart) inherited_bus= ; load_env; if ghostd_alive; then ghostd_stop; ghostd_start; fi ;;
    __ghostd-pid) inherited_bus= ; load_env; ghostd_pid ;;
    "") run_notifications_test ;;
    *) awk '/^# /{p=1} p&&!/^#/{exit} p' "$0" >&2; exit 64 ;;
esac

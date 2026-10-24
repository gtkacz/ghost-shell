# shellcheck shell=bash
# Hands org.freedesktop.Notifications from plasmashell to ghostd and back. Sourced by plasma-panel.sh, which dispatches
# retire-notifications and restore-notifications to it and prints notifications_status beside the panel's own line.
# GHOST_SYSTEMD_RELOAD, GHOST_GHOSTD_RESTART, GHOST_GHOSTD_TRY_RESTART, GHOST_GHOSTD_PID_CHECK and
# GHOST_GHOSTD_TYPE_CHECK each name one executable that replaces the systemctl call that reloads the user manager,
# restarts ghostd, restarts it only if it runs, prints its main pid, or prints its unit's Type (defaults: those
# systemctl --user calls).

# ghostd requests the name only while this file exists when it starts.
notifications_marker=$state_dir/notifications-server
notifications_retired=$state_dir/notifications-retired
notifications_name=org.freedesktop.Notifications
notifyrc_file=plasmanotifyrc
notifyrc_manifest=notifications-manifest
drop_in_source=$script_dir/../packaging/systemd/plasma-plasmashell-after-ghostd.conf
drop_in_dir=$config_home/systemd/user/$plasma_unit.d
drop_in=$drop_in_dir/ghost-shell-notifications.conf
default_notifications_shortcut=Meta+N
# Qt::MetaModifier; the letter's Qt key is its upper-case ASCII code.
qt_meta_modifier=$((0x10000000))
# Component ghostd registers its shortcuts under in KGlobalAccel.
ghost_component=ghost-shell

systemd_reload=(systemctl --user daemon-reload)
ghostd_restart=(systemctl --user restart ghostd)
ghostd_try_restart=(systemctl --user try-restart ghostd)
ghostd_pid=(systemctl --user show --property MainPID --value ghostd)
ghostd_type=(systemctl --user show --property Type --value ghostd)
if [[ -n ${GHOST_SYSTEMD_RELOAD:-} ]]; then systemd_reload=("$GHOST_SYSTEMD_RELOAD"); fi
if [[ -n ${GHOST_GHOSTD_RESTART:-} ]]; then ghostd_restart=("$GHOST_GHOSTD_RESTART"); fi
if [[ -n ${GHOST_GHOSTD_TRY_RESTART:-} ]]; then ghostd_try_restart=("$GHOST_GHOSTD_TRY_RESTART"); fi
if [[ -n ${GHOST_GHOSTD_PID_CHECK:-} ]]; then ghostd_pid=("$GHOST_GHOSTD_PID_CHECK"); fi
if [[ -n ${GHOST_GHOSTD_TYPE_CHECK:-} ]]; then ghostd_type=("$GHOST_GHOSTD_TYPE_CHECK"); fi

# Prints the pid of the process that owns bus name $1; fails while nobody does.
name_owner_pid() {
    local reply
    reply=$(session_call call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus \
        GetConnectionUnixProcessID s "$1" 2> /dev/null) || return 1
    [[ $reply == u\ * ]] || return 1
    echo "${reply#u }"
}

# Prints who holds the notification name now, for the error that says it is not who was expected.
describe_notifications_owner() {
    local pid
    if pid=$(name_owner_pid "$notifications_name"); then
        echo "pid $pid ($(cat "/proc/$pid/comm" 2> /dev/null || echo unknown))"
    else
        echo nobody
    fi
}

# Polls until the notification name belongs to the pid the rest of the arguments print. $1 names the expected owner.
wait_for_notifications_owner() {
    local who=$1 owner want deadline=$((SECONDS + plasma_deadline_s))
    shift
    while ((SECONDS < deadline)); do
        if owner=$(name_owner_pid "$notifications_name") && want=$("$@") && [[ $want != 0 && $owner == "$want" ]]; then
            step "$notifications_name belongs to $who (pid $owner)"
            return 0
        fi
        sleep "$poll_s"
    done
    warn "$notifications_name does not belong to $who after ${plasma_deadline_s} s; it belongs to $(describe_notifications_owner)"
    return 1
}

# Refuses before any change if a component other than ghost-shell holds the shortcut that opens the centre; ghostd
# registers that one itself.
check_notifications_key_free() {
    local shortcut holders others
    shortcut=$(config_value shortcuts notifications)
    shortcut=${shortcut:-$default_notifications_shortcut}
    if [[ ! $shortcut =~ ^Meta\+([A-Za-z])$ ]]; then
        warn "shortcuts.notifications is ${shortcut@Q}, not Meta+<letter>; not checking who holds it"
        return 0
    fi
    holders=$(key_holders $((qt_meta_modifier + $(printf '%d' "'${BASH_REMATCH[1]^^}")))) \
        || die "could not read who holds $shortcut"
    others=$(grep -v "^$ghost_component/" <<< "$holders" || true)
    if [[ -n $others ]]; then
        die "$shortcut, meant for ghost-shell's notification centre, is already held by ${others//$'\n'/, }; free it first"
    fi
}

back_up_notifyrc() {
    local backup=$1
    step "backing up $notifyrc_file into $backup"
    mkdir -p -- "$backup"
    if [[ -e $config_home/$notifyrc_file ]]; then
        cp -p -- "$config_home/$notifyrc_file" "$backup/$notifyrc_file"
        printf 'file\t%s\n' "$notifyrc_file" > "$backup/$notifyrc_manifest"
    else
        printf 'absent\t%s\n' "$notifyrc_file" > "$backup/$notifyrc_manifest"
    fi
}

# Refuses a backup whose manifest is missing, lists anything but the one file, or names a file the backup lacks.
validate_notifyrc_backup() {
    local backup=$1 manifest=() kind name
    [[ -s $backup/$notifyrc_manifest ]] || die "the backup in $backup has no $notifyrc_manifest"
    mapfile -t manifest < "$backup/$notifyrc_manifest"
    ((${#manifest[@]} == 1)) || die "the manifest in $backup lists ${#manifest[@]} entries, not 1"
    IFS=$'\t' read -r kind name <<< "${manifest[0]}"
    [[ $name == "$notifyrc_file" ]] || die "the manifest in $backup does not list $notifyrc_file"
    case $kind in
        file) [[ -f $backup/$name ]] || die "the backup in $backup lacks $name" ;;
        absent) ;;
        *) die "the manifest in $backup has an unknown entry ${manifest[0]@Q}" ;;
    esac
}

# With an older unit, restart returns before ghostd has settled the name, and plasmashell could win it.
check_ghostd_waits_for_ready() {
    local type
    type=$("${ghostd_type[@]}") || die "could not read the Type of ghostd.service"
    if [[ $type != notify ]]; then
        die "ghostd.service is Type=${type:-unset}, not notify; reinstall ghost-shell first (scripts/install.sh)"
    fi
}

retire_notifications() {
    local stamp backup bar_position popup_position
    recovery_marker=$notifications_retired
    recovery_command=restore-notifications
    if [[ -e $notifications_retired ]]; then
        die "notifications are already retired ($(< "$notifications_retired")); run restore-notifications before retiring again"
    fi
    [[ -f $drop_in_source ]] || die "$drop_in_source is missing"
    if ! name_owned org.kde.plasmashell; then
        die "plasmashell is not on the session bus"
    fi
    if ! "${plasma_check[@]}"; then
        die "$plasma_unit is not active or does not own the running plasmashell; restarting it would not restart plasmashell"
    fi
    check_ghostd_waits_for_ready
    check_notifications_key_free
    bar_position=$(config_value bar position)
    bar_position=${bar_position:-top}
    case $bar_position in
        top) popup_position=BottomRight ;;
        bottom) popup_position=TopRight ;;
        *) die "bar.position in $ghost_config is ${bar_position@Q}, not \"top\" or \"bottom\"" ;;
    esac
    stamp=$(date -u +%Y%m%dT%H%M%SZ)
    backup=$backup_root/$stamp
    if [[ -e $backup/$notifyrc_manifest ]]; then
        die "$backup already holds a notifications backup"
    fi

    back_up_notifyrc "$backup"
    validate_notifyrc_backup "$backup"
    step "marking notifications retired in $notifications_retired"
    mkdir -p -- "$state_dir"
    recovery_armed=1
    echo "$stamp" > "$notifications_retired"
    step "telling ghostd to own the name: $notifications_marker"
    echo "$stamp" > "$notifications_marker"
    step "installing the drop-in that starts plasmashell after ghostd: $drop_in"
    install -D -m 644 -- "$drop_in_source" "$drop_in"
    step "reloading the user manager: ${systemd_reload[*]@Q}"
    "${systemd_reload[@]}"

    # Plasma rewrites its rc files as it quits, so the corner is written while it is down.
    stop_plasmashell
    step "setting Plasma's own popups (jobs and the like) to $popup_position in $config_home/$notifyrc_file"
    kwriteconfig6 --file "$config_home/$notifyrc_file" --group Notifications --key PopupPosition "$popup_position"
    step "restarting ghostd, which takes the name: ${ghostd_restart[*]@Q}"
    "${ghostd_restart[@]}"
    start_plasmashell
    wait_for_notifications_owner ghostd "${ghostd_pid[@]}"
    step "retired; undo with: $(printf '%q' "$0") restore-notifications"
}

restore_notifications() {
    local stamp backup kind file
    recovery_marker=$notifications_retired
    recovery_command=restore-notifications
    if [[ ! -e $notifications_retired ]]; then
        die "notifications are not retired; nothing to restore"
    fi
    stamp=$(< "$notifications_retired")
    backup=$backup_root/$stamp
    validate_notifyrc_backup "$backup"

    recovery_armed=1
    step "removing ghostd's marker $notifications_marker"
    attempt rm -f -- "$notifications_marker"
    step "removing the drop-in $drop_in"
    attempt rm -f -- "$drop_in"
    if [[ -d $drop_in_dir ]]; then
        attempt rmdir --ignore-fail-on-non-empty -- "$drop_in_dir"
    fi
    step "reloading the user manager: ${systemd_reload[*]@Q}"
    attempt "${systemd_reload[@]}"
    attempt stop_plasmashell
    IFS=$'\t' read -r kind file < "$backup/$notifyrc_manifest"
    case $kind in
        file)
            step "putting back $config_home/$file"
            attempt cp -p -- "$backup/$file" "$config_home/$file"
            ;;
        absent)
            if [[ -e $config_home/$file ]]; then
                step "removing $config_home/$file, which did not exist before the retire"
                attempt rm -- "$config_home/$file"
            fi
            ;;
    esac
    # A ghostd that is stopped stays stopped: this only makes a running one let go of the name.
    step "restarting ghostd if it runs, so it lets go of the name: ${ghostd_try_restart[*]@Q}"
    attempt "${ghostd_try_restart[@]}"
    attempt start_plasmashell
    attempt wait_for_notifications_owner plasmashell name_owner_pid org.kde.plasmashell
    if ((${#failures[@]} > 0)); then
        printf 'error: restore-notifications did not finish; %d step(s) failed:\n' "${#failures[@]}" >&2
        printf '  %s\n' "${failures[@]}" >&2
        exit 1
    fi
    step "marking notifications as served by Plasma (the backup stays in $backup)"
    rm -- "$notifications_retired"
}

notifications_status() {
    if [[ -e $notifications_retired ]]; then
        echo "notifications retired $(< "$notifications_retired")"
    else
        echo "plasma serving notifications"
    fi
}

#!/usr/bin/env bash
set -Eeuo pipefail

# Hands Plasma's panel and Kickoff over to ghost-shell's bar and launcher, and gives them back.
#   plasma-panel.sh retire    back up Plasma's layout and shortcuts, remove the panel, put the System Tray and the
#                             Digital Clock on the desktop under the bar's strip, give the popups ghostd opens a global
#                             shortcut each, take bare Meta off Plasma's launcher and clear its Meta+1…9 task entries
#   plasma-panel.sh restore   put the latest backup back and give Plasma its shortcuts back
#   plasma-panel.sh status    print "retired <stamp>" or "plasma panel in place"
# It runs against $DBUS_SESSION_BUS_ADDRESS, $XDG_CONFIG_HOME and $XDG_STATE_HOME. GHOST_PLASMA_STOP, GHOST_PLASMA_START
# and GHOST_PLASMA_CHECK each name one executable that replaces the command that stops plasmashell, starts it, or
# succeeds when plasmashell runs under its systemd user unit (defaults: that unit's systemctl calls).
# If retire or restore fails after it changed something, it starts plasmashell again if it had stopped it, and prints
# the command that recovers.

: "${HOME:?HOME must be set}"

config_home=${XDG_CONFIG_HOME:-$HOME/.config}
state_home=${XDG_STATE_HOME:-$HOME/.local/state}
state_dir=$state_home/ghost-shell
backup_root=$state_dir/plasma-backup
retired_marker=$state_dir/plasma-retired
# Read by ghostd's plasma.popup call: one "<applet plugin><TAB><widget id>" line per popup.
popup_map=$state_dir/plasma-popups

plasma_unit=plasma-plasmashell.service
plasma_stop=(systemctl --user stop "$plasma_unit")
plasma_start=(systemctl --user start "$plasma_unit")
plasma_check=(unit_owns_plasmashell)
# One word each, so a path with spaces reaches the command whole.
if [[ -n ${GHOST_PLASMA_STOP:-} ]]; then plasma_stop=("$GHOST_PLASMA_STOP"); fi
if [[ -n ${GHOST_PLASMA_START:-} ]]; then plasma_start=("$GHOST_PLASMA_START"); fi
if [[ -n ${GHOST_PLASMA_CHECK:-} ]]; then plasma_check=("$GHOST_PLASMA_CHECK"); fi
plasma_deadline_s=60
poll_s=0.2

layout_file=plasma-org.kde.plasma.desktop-appletsrc
backed_up_files=("$layout_file" plasmashellrc kglobalshortcutsrc)
keys_file=keys
manifest_file=manifest
required_tools=(busctl jq kwriteconfig6)

launcher_action="activate application launcher"
task_entry="activate task manager entry"
# plasmashell registers entries 1…10 on every start, panel or not. Meta+0 is KWin's zoom reset, not one of these.
task_entries=10
actions=("$launcher_action")
for ((entry = 1; entry <= task_entries; entry++)); do
    actions+=("$task_entry $entry")
done
# Qt::Key_Meta with no modifier bit, as KGlobalAccel stores bare Meta.
key_meta=$((0x01000022))
# A Qt key sequence holds four keys; KGlobalAccel's own client always sends all four.
keys_per_sequence=4
# KGlobalAccel's actionId is component, action, component friendly name, action friendly name.
action_id_length=4
# Positions in a KGlobalShortcutInfo as allShortcutInfos returns it.
info_action_name_at=0
info_keys_at=6
tray_plugin=org.kde.plasma.systemtray
clock_plugin=org.kde.plasma.digitalclock
# The applets ghostd's plasma.popup accepts (POPUP_APPLETS in crates/ghostd/src/feeds/actions.rs), each with the key
# Plasma registers its "activate widget <id>" action under. ghostd never presses the key, but Plasma registers the
# action only for a widget that has one. Meta with Ctrl, Alt and Shift is held by nothing in a stock KDE.
popups=(
    "org.kde.plasma.networkmanagement Meta+Ctrl+Alt+Shift+F1"
    "org.kde.plasma.bluetooth Meta+Ctrl+Alt+Shift+F2"
    "org.kde.plasma.volume Meta+Ctrl+Alt+Shift+F3"
    "org.kde.kdeconnect Meta+Ctrl+Alt+Shift+F4"
    "org.kde.plasma.clipboard Meta+Ctrl+Alt+Shift+F5"
    "$tray_plugin Meta+Ctrl+Alt+Shift+F6"
    "$clock_plugin Meta+Ctrl+Alt+Shift+F7"
)

# Run through org.kde.PlasmaShell.evaluateScript. Prints "tray <id>", "clock <id>" and "ok" last.
read -r -d '' retire_js << 'EOF' || true
const TRAY = "org.kde.plasma.systemtray";
const CLOCK = "org.kde.plasma.digitalclock";
// The config groups the old panel's tray and clock keep their settings in.
const TRAY_CONFIG_GROUP = "General";
const CLOCK_CONFIG_GROUP = "Appearance";
// The bar's strip at the desktop's 1.15 scale, in logical px: both widgets sit under it.
const STRIP_HEIGHT = 52;
const TRAY_WIDTH = 480;
const CLOCK_WIDTH = 240;
// readConfig hands a list back as one comma-joined string, and writeConfig would store that string's characters.
const LIST_KEYS = ["extraItems", "hiddenItems", "shownItems", "knownItems", "disabledStatusNotifiers",
    "enabledCalendarPlugins", "selectedTimeZones"];
const lines = [];

// A walk over configGroups that changes currentConfigGroup visits only one group, so each group is named.
function copyConfig(from, to, group) {
    from.currentConfigGroup = [group];
    to.currentConfigGroup = [group];
    const keys = from.configKeys;
    for (let i = 0; i < keys.length; i++) {
        const value = from.readConfig(keys[i]);
        const isList = LIST_KEYS.indexOf(keys[i]) >= 0;
        to.writeConfig(keys[i], isList ? (value === "" ? [] : value.split(",")) : value);
    }
    to.reloadConfig();
    return group + ": " + keys.join(",");
}

const desktop = desktops().filter(function (d) { return d.screen === 0; })[0];
if (!desktop) {
    throw new Error("no desktop on screen 0");
}
const area = screenGeometry(0);
lines.push("desktop " + desktop.id + " on a " + area.width + "x" + area.height + " screen");

let oldTray = null;
let oldClock = null;
panels().forEach(function (panel) {
    oldTray = oldTray || panel.widgets(TRAY)[0] || null;
    oldClock = oldClock || panel.widgets(CLOCK)[0] || null;
});

const tray = desktop.addWidget(TRAY, area.width - CLOCK_WIDTH - TRAY_WIDTH, 0, TRAY_WIDTH, STRIP_HEIGHT);
const clock = desktop.addWidget(CLOCK, area.width - CLOCK_WIDTH, 0, CLOCK_WIDTH, STRIP_HEIGHT);
if (oldTray) {
    lines.push("copied the panel tray's " + copyConfig(oldTray, tray, TRAY_CONFIG_GROUP));
}
if (oldClock) {
    lines.push("copied the panel clock's " + copyConfig(oldClock, clock, CLOCK_CONFIG_GROUP));
}
panels().forEach(function (panel) {
    lines.push("removed panel " + panel.id);
    panel.remove();
});
lines.push("tray " + tray.id);
lines.push("clock " + clock.id);
lines.push("ok");
print(lines.join("\n"));
EOF

# Set once a change has been made that only restore can undo, and while this script holds plasmashell stopped.
recovery_armed=0
plasma_stopped=0
failures=()

step() {
    printf '==> %s\n' "$*"
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

on_error() {
    local status=$?
    printf 'error: "%s" failed with status %d\n' "$BASH_COMMAND" "$status" >&2
    return "$status"
}

# The one place every way out passes (ERR and die exit, and INT, HUP and TERM are turned into exits), so a failure after
# the first change never leaves plasmashell stopped, and always says how to recover.
recover() {
    local status=$?
    trap - EXIT ERR
    if ((status == 0 || ! recovery_armed)); then
        exit "$status"
    fi
    if [[ -e $retired_marker ]]; then
        printf 'error: the backup is in %s; recover with: %q restore\n' \
            "$backup_root/$(< "$retired_marker")" "$0" >&2
    fi
    if ((plasma_stopped)); then
        printf 'error: plasmashell was stopped; starting it again so the desktop is not left blank\n' >&2
        start_plasmashell || warn "plasmashell did not start; start it with: ${plasma_start[*]@Q}"
    fi
    exit "$status"
}
trap recover EXIT
trap 'exit 130' INT
trap 'exit 129' HUP
trap 'exit 143' TERM
trap on_error ERR

check_paths() {
    # Relative dirs would put the backup and the popup map wherever the script happens to run.
    if [[ $config_home != /* || $state_home != /* ]]; then
        die "XDG_CONFIG_HOME ($config_home) and XDG_STATE_HOME ($state_home) must be absolute"
    fi
}

check_prerequisites() {
    local tool
    : "${DBUS_SESSION_BUS_ADDRESS:?DBUS_SESSION_BUS_ADDRESS must be set to the session bus Plasma runs on}"
    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" > /dev/null 2>&1; then
            die "$tool not found in PATH"
        fi
    done
}

session_call() {
    busctl --user "$@"
}

name_owned() {
    [[ $(session_call call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus NameHasOwner s "$1") == "b true" ]]
}

# The script restarts plasmashell through its unit; if the unit is not what runs it, stopping the unit stops nothing.
unit_owns_plasmashell() {
    local cgroup owner
    systemctl --user is-active --quiet "$plasma_unit" || return 1
    cgroup=$(systemctl --user show --property ControlGroup --value "$plasma_unit") || return 1
    owner=$(session_call call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus \
        GetConnectionUnixProcessID s org.kde.plasmashell) || return 1
    [[ -n $cgroup && $owner == u\ * && -r /proc/${owner#u }/cgroup ]] || return 1
    [[ $(< "/proc/${owner#u }/cgroup") == *"$cgroup"* ]]
}

wait_for_plasmashell() {
    local want=$1 deadline=$((SECONDS + plasma_deadline_s))
    while ((SECONDS < deadline)); do
        if { [[ $want == up ]] && name_owned org.kde.plasmashell; } || { [[ $want == down ]] && ! name_owned org.kde.plasmashell; }; then
            return 0
        fi
        sleep "$poll_s"
    done
    warn "plasmashell is not $want after ${plasma_deadline_s} s"
    return 1
}

stop_plasmashell() {
    step "stopping plasmashell: ${plasma_stop[*]@Q}"
    # Set first: a stop that fails halfway may still have taken plasmashell down.
    plasma_stopped=1
    "${plasma_stop[@]}" && wait_for_plasmashell down
}

start_plasmashell() {
    step "starting plasmashell: ${plasma_start[*]@Q}"
    "${plasma_start[@]}" && wait_for_plasmashell up && plasma_stopped=0
}

reconfigure_kwin() {
    step "reconfiguring KWin"
    session_call call org.kde.KWin /KWin org.kde.KWin reconfigure
}

shortcut_infos() {
    session_call --json=short call org.kde.kglobalaccel /component/plasmashell org.kde.kglobalaccel.Component \
        allShortcutInfos
}

# Prints the keys of one of plasmashell's actions as ints joined by spaces, as KGlobalAccel holds them. It fails when
# the read fails or the action is missing, so no caller can take a failed read for "no keys".
action_keys() {
    local infos
    infos=$(shortcut_infos) || return 1
    jq -er --arg action "$1" --argjson name_at "$info_action_name_at" --argjson keys_at "$info_keys_at" \
        'first(.data[0][] | select(.[$name_at] == $action)) | .[$keys_at] | map(tostring) | join(" ")' <<< "$infos"
}

# Sets $launcher_keys. A launcher that reads as keyless is a failed read, and clearing it would take Alt+F1 with it.
read_launcher_keys() {
    launcher_keys=$(action_keys "$launcher_action") || die "could not read the keys of \"$launcher_action\""
    if [[ -z $launcher_keys ]]; then
        die "\"$launcher_action\" has no keys; refusing to clear them"
    fi
}

# KGlobalAccel keeps its registry in memory and rewrites kglobalshortcutsrc from it, so keys change through it, as
# the shortcuts KCM changes them; an edit to the file would be overwritten while KWin runs.
set_keys() {
    local action=$1 key sequences=()
    shift
    for key in "$@"; do
        sequences+=("$keys_per_sequence" "$key" 0 0 0)
    done
    session_call call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel setForeignShortcutKeys 'asa(ai)' \
        "$action_id_length" plasmashell "$action" "" "" "$#" "${sequences[@]}"
}

# plasmashell may still be registering when its name appears, so a set that did not take is polled for.
wait_for_keys() {
    local action=$1 want=$2 got deadline=$((SECONDS + plasma_deadline_s))
    while ((SECONDS < deadline)); do
        if got=$(action_keys "$action") && [[ $got == "$want" ]]; then
            return 0
        fi
        sleep "$poll_s"
    done
    return 1
}

# The desktop saves where its widgets sit a while after they move, not when plasmashell quits; stopped before that, it
# lays the new ones out again from its top left corner.
wait_for_geometries() {
    local id deadline=$((SECONDS + plasma_deadline_s))
    step "waiting for Plasma to save where the tray ($1) and the clock ($2) sit"
    for id in "$@"; do
        while ! grep -q "^ItemGeometries-.*Applet-$id:" "$config_home/$layout_file"; do
            if ((SECONDS >= deadline)); then
                die "Plasma did not save the position of widget $id within ${plasma_deadline_s} s"
            fi
            sleep "$poll_s"
        done
    done
}

# Prints "<group path> <plugin>" for every applet in the layout file, the path as kwriteconfig6 --group arguments
# joined by "/", such as Containments/56/Applets/95/Applets/98.
applet_groups() {
    awk '
        /^\[Containments\]/ {
            group = $0
            gsub(/^\[|\]$/, "", group)
            gsub(/\]\[/, "/", group)
            is_applet = group ~ /\/Applets\/[0-9]+$/
            next
        }
        /^\[/ { is_applet = 0 }
        is_applet && /^plugin=/ { print group, substr($0, 8) }' "$config_home/$layout_file"
}

# Prints the group path of applet <id>, or nothing.
applet_group() {
    applet_groups | awk -v id="$1" '{ n = split($1, part, "/") } part[n] == id { print $1; exit }'
}

# The tray's own applets live inside its group; the scripting API can't reach them, so their shortcuts go into the
# layout file while plasmashell is down (it writes the file on the way out).
assign_popup_shortcuts() {
    local tray_id=$1 clock_id=$2 tray_group clock_group entry plugin key group part groups=() group_args=()
    tray_group=$(applet_group "$tray_id")
    clock_group=$(applet_group "$clock_id")
    if [[ -z $tray_group || -z $clock_group ]]; then
        die "the new tray ($tray_id) or clock ($clock_id) is not in $config_home/$layout_file"
    fi
    : > "$popup_map.part"
    for entry in "${popups[@]}"; do
        read -r plugin key <<< "$entry"
        case $plugin in
            "$tray_plugin") group=$tray_group ;;
            "$clock_plugin") group=$clock_group ;;
            *) group=$(applet_groups | awk -v plugin="$plugin" -v prefix="$tray_group/Applets/" \
                '$2 == plugin && index($1, prefix) == 1 { print $1; exit }') ;;
        esac
        if [[ -z $group ]]; then
            warn "the tray has no $plugin applet, so ghost-shell can't open its popup"
            continue
        fi
        IFS=/ read -r -a groups <<< "$group"
        group_args=()
        for part in "${groups[@]}" Shortcuts; do
            group_args+=(--group "$part")
        done
        step "giving $plugin (widget ${groups[-1]}) the global shortcut $key"
        kwriteconfig6 --file "$config_home/$layout_file" "${group_args[@]}" --key global "$key"
        printf '%s\t%s\n' "$plugin" "${groups[-1]}" >> "$popup_map.part"
    done
    step "writing the popup map for ghostd to $popup_map"
    mv -- "$popup_map.part" "$popup_map"
}

# Refuses a backup that lacks any of the files or the 11 actions' keys, or holds anything unparsable, so a truncated
# backup is never restored as "no keys".
validate_backup() {
    local backup=$1 manifest=() key_lines=() i kind name keys
    [[ -s $backup/$manifest_file && -f $backup/$keys_file ]] || die "the backup in $backup is incomplete"
    mapfile -t manifest < "$backup/$manifest_file"
    ((${#manifest[@]} == ${#backed_up_files[@]} + 1)) || die "the manifest in $backup lists ${#manifest[@]} entries"
    for ((i = 0; i < ${#backed_up_files[@]}; i++)); do
        IFS=$'\t' read -r kind name <<< "${manifest[i]}"
        [[ $name == "${backed_up_files[i]}" ]] || die "the manifest in $backup does not list ${backed_up_files[i]}"
        case $kind in
            file) [[ -f $backup/$name ]] || die "the backup in $backup lacks $name" ;;
            absent) ;;
            *) die "the manifest in $backup has an unknown entry ${manifest[i]@Q}" ;;
        esac
    done
    [[ ${manifest[-1]} == "keys"$'\t'"$keys_file" ]] || die "the manifest in $backup does not list the keys"
    mapfile -t key_lines < "$backup/$keys_file"
    ((${#key_lines[@]} == ${#actions[@]})) || die "the keys in $backup hold ${#key_lines[@]} of ${#actions[@]} actions"
    for ((i = 0; i < ${#actions[@]}; i++)); do
        IFS=$'\t' read -r name keys <<< "${key_lines[i]}"
        [[ $name == "${actions[i]}" && $keys =~ ^([0-9]+( [0-9]+)*)?$ ]] \
            || die "the keys in $backup have a bad entry for \"${actions[i]}\": ${key_lines[i]@Q}"
    done
}

back_up() {
    local backup=$1 file action keys live name want
    step "backing up ${backed_up_files[*]} and the keys of ${#actions[@]} plasmashell shortcuts into $backup"
    mkdir -p -- "$backup"
    : > "$backup/$manifest_file"
    for file in "${backed_up_files[@]}"; do
        if [[ -e $config_home/$file ]]; then
            cp -p -- "$config_home/$file" "$backup/$file"
            printf 'file\t%s\n' "$file" >> "$backup/$manifest_file"
        else
            printf 'absent\t%s\n' "$file" >> "$backup/$manifest_file"
        fi
    done
    : > "$backup/$keys_file"
    for action in "${actions[@]}"; do
        keys=$(action_keys "$action") || die "could not read the keys of \"$action\""
        printf '%s\t%s\n' "$action" "$keys" >> "$backup/$keys_file"
    done
    printf 'keys\t%s\n' "$keys_file" >> "$backup/$manifest_file"
    validate_backup "$backup"
    # Read again so a backup that disagrees with the live keys never reaches the marker.
    while IFS=$'\t' read -r name want; do
        live=$(action_keys "$name") || die "could not read the keys of \"$name\" again"
        [[ $live == "$want" ]] || die "the keys of \"$name\" changed while they were backed up"
    done < "$backup/$keys_file"
}

retire() {
    local stamp backup result tray_id clock_id action key kept=() launcher_keys
    if [[ -e $retired_marker ]]; then
        die "the panel is already retired ($(< "$retired_marker")); run restore before retiring again"
    fi
    if [[ ! -f $config_home/$layout_file ]]; then
        die "$config_home/$layout_file is missing; is this a Plasma session?"
    fi
    if ! name_owned org.kde.plasmashell; then
        die "plasmashell is not on the session bus"
    fi
    if ! "${plasma_check[@]}"; then
        die "$plasma_unit is not active or does not own the running plasmashell; restarting it would not restart plasmashell"
    fi
    for action in "${actions[@]}"; do
        action_keys "$action" > /dev/null || die "could not read plasmashell's \"$action\" shortcut; is this Plasma 6?"
    done
    read_launcher_keys
    stamp=$(date -u +%Y%m%dT%H%M%SZ)
    backup=$backup_root/$stamp
    if [[ -e $backup ]]; then
        die "$backup already exists"
    fi

    back_up "$backup"
    step "marking the panel retired in $retired_marker"
    mkdir -p -- "$state_dir"
    recovery_armed=1
    echo "$stamp" > "$retired_marker"

    step "removing the panel and putting the System Tray and the Digital Clock on the desktop"
    result=$(session_call --json=short call org.kde.plasmashell /PlasmaShell org.kde.PlasmaShell evaluateScript \
        s "$retire_js" | jq -r '.data[0]')
    printf '    %s\n' "${result//$'\n'/$'\n    '}"
    if [[ $(tail -n 1 <<< "$result") != ok ]]; then
        die "Plasma's script did not finish"
    fi
    tray_id=$(awk '$1 == "tray" { print $2 }' <<< "$result")
    clock_id=$(awk '$1 == "clock" { print $2 }' <<< "$result")
    if [[ ! $tray_id =~ ^[0-9]+$ || ! $clock_id =~ ^[0-9]+$ ]]; then
        die "Plasma's script gave the tray id ${tray_id@Q} and the clock id ${clock_id@Q}"
    fi

    wait_for_geometries "$tray_id" "$clock_id"
    stop_plasmashell
    assign_popup_shortcuts "$tray_id" "$clock_id"
    start_plasmashell

    read_launcher_keys
    for key in $launcher_keys; do
        if ((key != key_meta)); then
            kept+=("$key")
        fi
    done
    step "taking bare Meta off plasmashell's \"$launcher_action\", keeping ${kept[*]:-no keys}"
    set_keys "$launcher_action" "${kept[@]}"
    wait_for_keys "$launcher_action" "${kept[*]}" || die "\"$launcher_action\" did not keep only ${kept[*]:-no keys}"
    # Cleared rather than unregistered: plasmashell registers these on every start, and an unregistered action
    # comes back with its default Meta+N.
    for ((entry = 1; entry <= task_entries; entry++)); do
        step "clearing the keys of plasmashell's \"$task_entry $entry\""
        set_keys "$task_entry $entry"
        wait_for_keys "$task_entry $entry" "" || die "\"$task_entry $entry\" still has keys"
    done
    reconfigure_kwin
    step "retired; undo with: $(printf '%q' "$0") restore"
}

unregister_widget_action() {
    session_call call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel unregister ss plasmashell \
        "activate widget $1" > /dev/null
}

# Runs one command and keeps going if it fails, so restore does everything it can and reports what it could not.
attempt() {
    if ! "$@"; then
        failures+=("$*")
        warn "failed: $*"
    fi
}

restore() {
    local stamp backup kind file action keys plugin id map
    if [[ ! -e $retired_marker ]]; then
        die "the panel is not retired; nothing to restore"
    fi
    stamp=$(< "$retired_marker")
    backup=$backup_root/$stamp
    validate_backup "$backup"

    recovery_armed=1
    # plasmashell writes its layout when it quits, so it must be gone before the files go back. A stop that fails must
    # not keep the files and keys from going back.
    attempt stop_plasmashell
    # A retire that died before it moved the map into place leaves the .part, and its widgets are registered too.
    for map in "$popup_map" "$popup_map.part"; do
        if [[ -f $map ]]; then
            while IFS=$'\t' read -r plugin id; do
                step "removing plasmashell's \"activate widget $id\" ($plugin)"
                attempt unregister_widget_action "$id"
            done < "$map"
        fi
    done
    while IFS=$'\t' read -r action keys; do
        step "giving plasmashell's \"$action\" its keys back: ${keys:-none}"
        # shellcheck disable=SC2086
        attempt set_keys "$action" $keys
    done < "$backup/$keys_file"
    # Last, so nothing written above can land over them.
    while IFS=$'\t' read -r kind file; do
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
    done < "$backup/$manifest_file"
    for map in "$popup_map" "$popup_map.part"; do
        if [[ -f $map ]]; then
            step "removing the popup map $map"
            attempt rm -- "$map"
        fi
    done
    attempt start_plasmashell
    if ((${#failures[@]} > 0)); then
        printf 'error: restore did not finish; %d step(s) failed:\n' "${#failures[@]}" >&2
        printf '  %s\n' "${failures[@]}" >&2
        exit 1
    fi
    # Before the reconfigure: after a complete restore a failing reconfigure must not leave the panel marked retired.
    step "marking the panel in place (the backup stays in $backup)"
    rm -- "$retired_marker"
    reconfigure_kwin || warn "KWin did not reconfigure; its shortcuts update on its next start"
}

status() {
    if [[ -e $retired_marker ]]; then
        echo "retired $(< "$retired_marker")"
    else
        echo "plasma panel in place"
    fi
}

usage() {
    awk '/^# /{ printing = 1 } printing && !/^#/ { exit } printing' "$0" >&2
    exit 64
}

check_paths
case ${1:-} in
    retire) check_prerequisites; retire ;;
    restore) check_prerequisites; restore ;;
    status) status ;;
    *) usage ;;
esac

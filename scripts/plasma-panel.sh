#!/usr/bin/env bash
set -Eeuo pipefail

# Hands Plasma's panel and Kickoff over to ghost-shell's bar and launcher, and gives them back.
#   plasma-panel.sh retire    back up Plasma's layout and shortcuts, remove the panel, put the System Tray and the
#                             Digital Clock on the desktop under the bar's strip, give the popups ghostd opens a global
#                             shortcut each, take bare Meta off Plasma's launcher and clear its Meta+1…9 task entries
#   plasma-panel.sh restore   put the latest backup back and give Plasma its shortcuts back
#   plasma-panel.sh status    print "retired <stamp>" or "plasma panel in place", then "notifications retired <stamp>"
#                             or "plasma serving notifications"
#   plasma-panel.sh retire-notifications
#                             hand org.freedesktop.Notifications from plasmashell to ghostd (plasma-notifications.sh)
#   plasma-panel.sh restore-notifications
#                             give it back to plasmashell
# It runs against $DBUS_SESSION_BUS_ADDRESS, $XDG_CONFIG_HOME and $XDG_STATE_HOME. GHOST_PLASMA_STOP, GHOST_PLASMA_START
# and GHOST_PLASMA_CHECK each name one executable that replaces the command that stops plasmashell, starts it, or
# succeeds when plasmashell runs under its systemd user unit (defaults: that unit's systemctl calls).
# If a retire or restore fails after it changed something, it starts plasmashell again if it had stopped it, and prints
# the command that recovers.

: "${HOME:?HOME must be set}"

script_dir=$(dirname "${BASH_SOURCE[0]}")
# shellcheck source=plasma-lib.sh
source "$script_dir/plasma-lib.sh"
# shellcheck source=plasma-notifications.sh
source "$script_dir/plasma-notifications.sh"

# Read by ghostd's plasma.popup call: one "<applet plugin><TAB><widget id>" line per popup.
popup_map=$state_dir/plasma-popups

layout_file=plasma-org.kde.plasma.desktop-appletsrc
backed_up_files=("$layout_file" plasmashellrc kglobalshortcutsrc)
keys_file=keys
manifest_file=manifest

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
# KGlobalAccel's actionId is component, action, component friendly name, action friendly name.
action_id_length=4
default_bar_position=top
default_display_model=
drm_dir=${GHOST_DRM_DIR:-/sys/class/drm}
# The shell's own sources hold the bar's height and the design width, so they are read rather than copied.
shell_lib=$script_dir/../shell/kit/GhostKit/lib
# An EDID holds four 18-byte descriptors from byte 54; a monitor-name one starts 00 00 00 FC 00 and ends in 0x0A.
edid_descriptor_offsets=(54 72 90 108)
edid_descriptor_length=18
edid_name_tag=000000fc
edid_name_skip=5
tray_plugin=org.kde.plasma.systemtray
clock_plugin=org.kde.plasma.digitalclock
# Qt::MetaModifier | ControlModifier | AltModifier | ShiftModifier, and Qt::Key_F1, which F2…F7 follow by one.
popup_modifiers=$((0x1E000000))
key_f1=$((0x01000030))
# The applets ghostd's plasma.popup accepts (POPUP_APPLETS in crates/ghostd/src/feeds/actions.rs), each with the key
# Plasma registers its "activate widget <id>" action under. ghostd never presses the key, but Plasma registers the
# action only for a widget that has one. Meta with Ctrl, Alt and Shift is held by nothing in a stock KDE; retire checks.
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
// BAR_BAND_PX, DESIGN_WIDTH, BAR_AT_BOTTOM and BAR_CONNECTOR come first, from retire: the bar's own figures and where it is.
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

const screen = BAR_CONNECTOR === "" ? 0 : screenForConnector(BAR_CONNECTOR);
if (screen < 0) {
    throw new Error("Plasma has no screen " + BAR_CONNECTOR);
}
const desktop = desktops().filter(function (d) { return d.screen === screen; })[0];
if (!desktop) {
    throw new Error("no desktop on screen " + screen);
}
const area = screenGeometry(screen);
// The bar's height is its design band scaled by the screen's width over the design width, as the shell does.
const STRIP_HEIGHT = Math.round(BAR_BAND_PX * area.width / DESIGN_WIDTH);
const STRIP_Y = BAR_AT_BOTTOM ? area.height - STRIP_HEIGHT : 0;
lines.push("desktop " + desktop.id + " on a " + area.width + "x" + area.height + " screen, strip " + STRIP_HEIGHT + " px at y " + STRIP_Y);

let oldTray = null;
let oldClock = null;
panels().forEach(function (panel) {
    oldTray = oldTray || panel.widgets(TRAY)[0] || null;
    oldClock = oldClock || panel.widgets(CLOCK)[0] || null;
});

const tray = desktop.addWidget(TRAY, area.width - CLOCK_WIDTH - TRAY_WIDTH, STRIP_Y, TRAY_WIDTH, STRIP_HEIGHT);
const clock = desktop.addWidget(CLOCK, area.width - CLOCK_WIDTH, STRIP_Y, CLOCK_WIDTH, STRIP_HEIGHT);
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

# Refuses before any change if another action already holds one of the popups' keys.
check_popup_keys_free() {
    local entry plugin key holders
    for entry in "${popups[@]}"; do
        read -r plugin key <<< "$entry"
        holders=$(key_holders $((popup_modifiers + key_f1 + ${key##*+F} - 1))) || die "could not read who holds $key"
        if [[ -n $holders ]]; then
            die "$key, meant for the $plugin popup, is already held by ${holders//$'\n'/, }; free it first"
        fi
    done
}

# Prints the value of an "export const <name> = <integer>;" line in one of the shell's lib modules.
shell_constant() {
    local value
    value=$(sed -n "s/^export const $2 = \([0-9][0-9]*\);\$/\1/p" "$shell_lib/$1") || return 1
    [[ -n $value ]] || die "$shell_lib/$1 has no integer constant $2; plasma-panel.sh needs it to size the strip"
    echo "$value"
}

# Prints the monitor name an EDID file carries, as the shell's screen model holds it.
edid_name() {
    local bytes=() offset i name=
    mapfile -t bytes < <(od -An -v -tx1 -w1 -N128 -- "$1" | tr -d ' ')
    for offset in "${edid_descriptor_offsets[@]}"; do
        if [[ ${bytes[offset]}${bytes[offset + 1]}${bytes[offset + 2]}${bytes[offset + 3]} == "$edid_name_tag" ]]; then
            for ((i = offset + edid_name_skip; i < offset + edid_descriptor_length; i++)); do
                if [[ ${bytes[i]} == 0a ]]; then
                    break
                fi
                name+=$(printf "\\x${bytes[i]}")
            done
            echo "$name"
            return 0
        fi
    done
}

# Prints the connector (DP-1) of the first screen whose model contains $1 as the shell's pickScreen matches it, or
# nothing: no model, or none matching, is screen 0 there too.
connector_for_model() {
    local wanted=${1,,} edid name dir
    if [[ -z $wanted ]]; then
        return 0
    fi
    for edid in "$drm_dir"/card*-*/edid; do
        [[ -r $edid ]] || continue
        name=$(edid_name "$edid")
        if [[ ${name,,} == *"$wanted"* ]]; then
            dir=${edid%/edid}
            dir=${dir##*/}
            echo "${dir#card*-}"
            return 0
        fi
    done
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
    local stamp backup result tray_id clock_id action key kept=() launcher_keys bar_position model connector at_bottom band_px design_px
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
    check_popup_keys_free
    bar_position=$(config_value bar position)
    bar_position=${bar_position:-$default_bar_position}
    if [[ $bar_position != top && $bar_position != bottom ]]; then
        die "bar.position in $ghost_config is ${bar_position@Q}, not \"top\" or \"bottom\""
    fi
    model=$(config_value display model)
    connector=$(connector_for_model "${model:-$default_display_model}")
    [[ $bar_position == bottom ]] && at_bottom=true || at_bottom=false
    band_px=$(shell_constant layout.mjs BAR_BAND_PX)
    design_px=$(shell_constant scale.mjs DESIGN_WIDTH)
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
        s "const BAR_BAND_PX = $band_px; const DESIGN_WIDTH = $design_px; const BAR_AT_BOTTOM = $at_bottom; const BAR_CONNECTOR = \"$connector\";
$retire_js" | jq -r '.data[0]')
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
    notifications_status
}

usage() {
    awk '/^# /{ printing = 1 } printing && !/^#/ { exit } printing' "$0" >&2
    exit 64
}

check_paths
case ${1:-} in
    retire) check_prerequisites; retire ;;
    restore) check_prerequisites; restore ;;
    retire-notifications) check_prerequisites; retire_notifications ;;
    restore-notifications) check_prerequisites; restore_notifications ;;
    status) status ;;
    *) usage ;;
esac

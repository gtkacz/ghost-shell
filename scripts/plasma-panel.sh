#!/usr/bin/env bash
set -Eeuo pipefail

# Hands Plasma's panel and Kickoff over to ghost-shell's bar and launcher, and gives them back.
#   plasma-panel.sh retire    back up Plasma's layout and shortcuts, remove the panel, put the System Tray and the
#                             Digital Clock on the desktop under the bar's strip, give the popups ghostd opens a global
#                             shortcut each, take bare Meta off Plasma's launcher and clear its Meta+1…9 task entries
#   plasma-panel.sh restore   put the latest backup back and give Plasma its shortcuts back
#   plasma-panel.sh status    print "retired <stamp>" or "plasma panel in place"
# It runs against $DBUS_SESSION_BUS_ADDRESS, $XDG_CONFIG_HOME and $XDG_STATE_HOME. GHOST_PLASMA_STOP and
# GHOST_PLASMA_START replace the commands that stop and start plasmashell (default: its systemd user unit).

: "${HOME:?HOME must be set}"
: "${DBUS_SESSION_BUS_ADDRESS:?DBUS_SESSION_BUS_ADDRESS must be set to the session bus Plasma runs on}"

config_home=${XDG_CONFIG_HOME:-$HOME/.config}
state_home=${XDG_STATE_HOME:-$HOME/.local/state}
state_dir=$state_home/ghost-shell
backup_root=$state_dir/plasma-backup
retired_marker=$state_dir/plasma-retired
# Read by ghostd's plasma.popup call: one "<applet plugin><TAB><widget id>" line per popup.
popup_map=$state_dir/plasma-popups

plasma_unit=plasma-plasmashell.service
plasma_stop=${GHOST_PLASMA_STOP:-systemctl --user stop $plasma_unit}
plasma_start=${GHOST_PLASMA_START:-systemctl --user start $plasma_unit}
plasma_deadline_s=60
poll_s=0.2

layout_file=plasma-org.kde.plasma.desktop-appletsrc
backed_up_files=("$layout_file" plasmashellrc kglobalshortcutsrc)
keys_file=keys
manifest_file=manifest
required_tools=(busctl jq kwriteconfig6 kreadconfig6)

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
    lines.push("copied the panel tray's " + copyConfig(oldTray, tray, "General"));
}
if (oldClock) {
    lines.push("copied the panel clock's " + copyConfig(oldClock, clock, "Appearance"));
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

# Every failure stops the script here; once retire has backed up, restore can undo whatever it already changed.
on_error() {
    local status=$?
    trap - ERR
    printf 'error: "%s" failed with status %d\n' "$BASH_COMMAND" "$status" >&2
    if [[ -e $retired_marker ]]; then
        printf 'error: the backup is in %s; run "%s restore" to undo what was already changed\n' \
            "$backup_root/$(< "$retired_marker")" "$0" >&2
    fi
    exit "$status"
}
trap on_error ERR

check_prerequisites() {
    local tool
    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" > /dev/null 2>&1; then
            die "$tool not found in PATH"
        fi
    done
    # Relative dirs would put the backup and the popup map wherever the script happens to run.
    if [[ $config_home != /* || $state_home != /* ]]; then
        die "XDG_CONFIG_HOME ($config_home) and XDG_STATE_HOME ($state_home) must be absolute"
    fi
}

session_call() {
    busctl --user "$@"
}

name_owned() {
    [[ $(session_call call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus NameHasOwner s "$1") == "b true" ]]
}

wait_for_plasmashell() {
    local want=$1 deadline=$((SECONDS + plasma_deadline_s))
    while ((SECONDS < deadline)); do
        if { [[ $want == up ]] && name_owned org.kde.plasmashell; } || { [[ $want == down ]] && ! name_owned org.kde.plasmashell; }; then
            return 0
        fi
        sleep "$poll_s"
    done
    die "plasmashell is not $want after ${plasma_deadline_s} s"
}

stop_plasmashell() {
    step "stopping plasmashell: $plasma_stop"
    $plasma_stop
    wait_for_plasmashell down
}

start_plasmashell() {
    step "starting plasmashell: $plasma_start"
    $plasma_start
    wait_for_plasmashell up
}

reconfigure_kwin() {
    step "reconfiguring KWin"
    session_call call org.kde.KWin /KWin org.kde.KWin reconfigure
}

# Prints the keys of one of plasmashell's actions as ints joined by spaces, as KGlobalAccel holds them.
action_keys() {
    session_call --json=short call org.kde.kglobalaccel /component/plasmashell org.kde.kglobalaccel.Component \
        allShortcutInfos | jq -r --arg action "$1" '.data[0][] | select(.[0] == $action) | .[6] | map(tostring) | join(" ")'
}

has_action() {
    session_call --json=short call org.kde.kglobalaccel /component/plasmashell org.kde.kglobalaccel.Component \
        allShortcutInfos | jq -e --arg action "$1" 'any(.data[0][]; .[0] == $action)' > /dev/null
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
        4 plasmashell "$action" "" "" "$#" "${sequences[@]}"
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

back_up() {
    local backup=$1 file action
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
        printf '%s\t%s\n' "$action" "$(action_keys "$action")" >> "$backup/$keys_file"
    done
    printf 'keys\t%s\n' "$keys_file" >> "$backup/$manifest_file"
}

retire() {
    local stamp backup result tray_id clock_id action key kept=()
    if [[ -e $retired_marker ]]; then
        die "the panel is already retired ($(< "$retired_marker")); run restore before retiring again"
    fi
    if [[ ! -f $config_home/$layout_file ]]; then
        die "$config_home/$layout_file is missing; is this a Plasma session?"
    fi
    if ! name_owned org.kde.plasmashell; then
        die "plasmashell is not on the session bus"
    fi
    for action in "${actions[@]}"; do
        if ! has_action "$action"; then
            die "plasmashell has no \"$action\" shortcut; is this Plasma 6?"
        fi
    done
    stamp=$(date -u +%Y%m%dT%H%M%SZ)
    backup=$backup_root/$stamp
    if [[ -e $backup ]]; then
        die "$backup already exists"
    fi

    back_up "$backup"
    step "marking the panel retired in $retired_marker"
    mkdir -p -- "$state_dir"
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

    wait_for_geometries "$tray_id" "$clock_id"
    stop_plasmashell
    assign_popup_shortcuts "$tray_id" "$clock_id"
    start_plasmashell

    for key in $(action_keys "$launcher_action"); do
        if ((key != key_meta)); then
            kept+=("$key")
        fi
    done
    step "taking bare Meta off plasmashell's \"$launcher_action\", keeping ${kept[*]:-no keys}"
    set_keys "$launcher_action" "${kept[@]}"
    # Cleared rather than unregistered: plasmashell registers these on every start, and an unregistered action
    # comes back with its default Meta+N.
    for ((entry = 1; entry <= task_entries; entry++)); do
        step "clearing the keys of plasmashell's \"$task_entry $entry\""
        set_keys "$task_entry $entry"
    done
    reconfigure_kwin
    step "retired; undo with: $0 restore"
}

restore() {
    local stamp backup kind file action keys plugin id
    if [[ ! -e $retired_marker ]]; then
        die "the panel is not retired; nothing to restore"
    fi
    stamp=$(< "$retired_marker")
    backup=$backup_root/$stamp
    if [[ ! -s $backup/$manifest_file || ! -f $backup/$keys_file ]]; then
        die "the backup in $backup is incomplete"
    fi
    while IFS=$'\t' read -r kind file; do
        if [[ $kind == file && ! -f $backup/$file ]]; then
            die "the backup in $backup lacks $file"
        fi
    done < "$backup/$manifest_file"

    # plasmashell writes its layout when it quits, so it must be gone before the files go back.
    stop_plasmashell
    if [[ -f $popup_map ]]; then
        while IFS=$'\t' read -r plugin id; do
            step "removing plasmashell's \"activate widget $id\" ($plugin)"
            session_call call org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel unregister ss plasmashell \
                "activate widget $id" > /dev/null
        done < "$popup_map"
    fi
    while IFS=$'\t' read -r action keys; do
        step "giving plasmashell's \"$action\" its keys back: ${keys:-none}"
        # shellcheck disable=SC2086
        set_keys "$action" $keys
    done < "$backup/$keys_file"
    # Last, so nothing written above can land over them.
    while IFS=$'\t' read -r kind file; do
        case $kind in
            file)
                step "putting back $config_home/$file"
                cp -p -- "$backup/$file" "$config_home/$file"
                ;;
            absent)
                if [[ -e $config_home/$file ]]; then
                    step "removing $config_home/$file, which did not exist before the retire"
                    rm -- "$config_home/$file"
                fi
                ;;
        esac
    done < "$backup/$manifest_file"
    if [[ -f $popup_map ]]; then
        step "removing the popup map $popup_map"
        rm -- "$popup_map"
    fi
    start_plasmashell
    reconfigure_kwin
    step "marking the panel in place (the backup stays in $backup)"
    rm -- "$retired_marker"
}

status() {
    if [[ -e $retired_marker ]]; then
        echo "retired $(< "$retired_marker")"
    else
        echo "plasma panel in place"
    fi
}

check_prerequisites
case ${1:-} in
    retire) retire ;;
    restore) restore ;;
    status) status ;;
    *) sed -n '4,11p' "$0" >&2; exit 64 ;;
esac

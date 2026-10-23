#!/usr/bin/env bash
set -euo pipefail

# Prototype of scripts/plasma-panel.sh: hands Plasma's panel over to ghost-shell's bar and gives it back.
#   plasma-panel.sh retire <session bus> <config dir> <backup dir>
#       back up Plasma's layout and shortcut files (once), remove every panel, put the System Tray and the Digital
#       Clock on the desktop under the bar's strip, give the popups popup.sh opens a global shortcut each (which
#       restarts plasmashell once), take bare Meta off Plasma's launcher action and clear the keys of its
#       "activate task manager entry N" actions
#   plasma-panel.sh restore <session bus> <config dir> <backup dir>
#       stop plasmashell, put the backed-up files back, drop the popups' shortcuts, give Plasma's actions their
#       keys back, start plasmashell
# PLASMA_STOP and PLASMA_START are the commands that stop and start plasmashell; on the desktop they would be
# `systemctl --user stop|start plasma-plasmashell.service`. The spike passes its own.
# <backup dir>/popups maps each popup's name to its widget id, for popup.sh.

spike=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
py=(env -u DBUS_SESSION_BUS_ADDRESS -u DBUS_SYSTEM_BUS_ADDRESS uv run --no-project --with dbus-next python -I -B)

layout_file=plasma-org.kde.plasma.desktop-appletsrc
layout_files=("$layout_file" plasmashellrc kglobalshortcutsrc)
launcher_action="activate application launcher"
task_entry="activate task manager entry"
task_entries=10
actions=("$launcher_action")
for ((n = 1; n <= task_entries; n++)); do
    actions+=("$task_entry $n")
done
# Qt::Key_Meta with no modifier bit, as KGlobalAccel stores bare Meta.
key_meta=$((0x01000022))
# name, plugin, and the key Plasma registers its "activate widget <id>" action under. popup.sh never presses the
# key, but Plasma registers the action only for a widget that has one. Meta with Ctrl, Alt and Shift is held by
# nothing in the user's shortcuts.
popups=(
    "network org.kde.plasma.networkmanagement Meta+Ctrl+Alt+Shift+F1"
    "bluetooth org.kde.plasma.bluetooth Meta+Ctrl+Alt+Shift+F2"
    "volume org.kde.plasma.volume Meta+Ctrl+Alt+Shift+F3"
    "kdeconnect org.kde.kdeconnect Meta+Ctrl+Alt+Shift+F4"
    "clipboard org.kde.plasma.clipboard Meta+Ctrl+Alt+Shift+F5"
    "tray org.kde.plasma.systemtray Meta+Ctrl+Alt+Shift+F6"
    "calendar org.kde.plasma.digitalclock Meta+Ctrl+Alt+Shift+F7"
)

usage() {
    sed -n '4,15p' "$0" >&2
    exit 64
}

is_private_bus() {
    [[ $1 == unix:* && $1 != *"/run/user/"* ]]
}

dbus_helper() {
    "${py[@]}" "$spike/plasma_dbus.py" "$1" "$bus" "${@:2}"
}

action_keys() {
    dbus_helper actions plasmashell | awk -F '\t' -v action="$1" '$1 == action { print $2 }' \
        | sed 's/keys=\[\(.*\)\]/\1/' | tr -d ' ' | tr ',' ' '
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
        is_applet && /^plugin=/ { print group, substr($0, 8) }' "$config/$layout_file"
}

assign_popup_shortcuts() {
    local desktop=$1 entry name plugin key group groups=()
    : > "$backup/popups"
    for entry in "${popups[@]}"; do
        read -r name plugin key <<< "$entry"
        # Only the widgets retire.js put on the desktop, and the tray's own children.
        group=$(applet_groups | awk -v plugin="$plugin" -v prefix="Containments/$desktop/Applets/" \
            '$2 == plugin && index($1, prefix) == 1 { print $1; exit }')
        if [[ -z $group ]]; then
            echo "no $plugin on desktop $desktop" >&2
            continue
        fi
        IFS=/ read -r -a groups <<< "$group"
        kwriteconfig6 --file "$config/$layout_file" $(printf -- '--group %s ' "${groups[@]}") --group Shortcuts \
            --key global "$key"
        printf '%s\t%s\n' "$name" "${groups[-1]}" >> "$backup/popups"
    done
    echo "popups: $(tr '\t\n' '= ' < "$backup/popups")"
}

retire() {
    mkdir -p "$backup"
    if [[ -e $backup/done ]]; then
        echo "a backup exists in $backup; restore first" >&2
        exit 1
    fi
    for file in "${layout_files[@]}"; do
        cp -- "$config/$file" "$backup/"
    done
    local action entry key
    : > "$backup/keys"
    for action in "${actions[@]}"; do
        printf '%s\t%s\n' "$action" "$(action_keys "$action")" >> "$backup/keys"
    done
    touch "$backup/done"
    echo "backed up: ${layout_files[*]} and the keys of ${#actions[@]} plasmashell actions"

    local result desktop
    result=$(dbus_helper eval "$spike/retire.js")
    echo "$result"
    desktop=$(echo "$result" | awk '/^desktop / { print $2 }')

    # The tray's children are out of the scripting API's reach, so their shortcuts go into the layout file while
    # plasmashell is down; it writes the file on the way out.
    $PLASMA_STOP
    assign_popup_shortcuts "$desktop"
    $PLASMA_START

    local kept=()
    for key in $(action_keys "$launcher_action"); do
        if ((key != key_meta)); then
            kept+=("$key")
        fi
    done
    dbus_helper setkeys plasmashell "$launcher_action" "${kept[@]}"
    # Cleared rather than unregistered: plasmashell registers these on every start, panel or not, and an
    # unregistered action comes back with its default Meta+N ahead of ghost-shell's.
    for ((entry = 1; entry <= task_entries; entry++)); do
        dbus_helper setkeys plasmashell "$task_entry $entry"
    done
    echo "launcher keys now: $(action_keys "$launcher_action"); $task_entry 1 keys now: $(action_keys "$task_entry 1")"
}

restore() {
    if [[ ! -e $backup/done ]]; then
        echo "no backup in $backup" >&2
        exit 1
    fi
    # plasmashell writes its layout back when it quits, so it must be gone before the files go back.
    $PLASMA_STOP
    for file in "${layout_files[@]}"; do
        cp -- "$backup/$file" "$config/"
    done
    # KGlobalAccel keeps its registry in memory and rewrites kglobalshortcutsrc from it, so the keys go back,
    # and the popups' actions go, through it.
    local name id action keys
    while IFS=$'\t' read -r name id; do
        dbus_helper unregister plasmashell "activate widget $id" > /dev/null
    done < "$backup/popups"
    while IFS=$'\t' read -r action keys; do
        # shellcheck disable=SC2086
        dbus_helper setkeys plasmashell "$action" $keys
    done < "$backup/keys"
    $PLASMA_START
    echo "launcher keys now: $(action_keys "$launcher_action"); $task_entry 1 keys now: $(action_keys "$task_entry 1")"
    rm -- "$backup/done"
}

if (($# != 4)); then
    usage
fi
command=$1
bus=$2
config=$3
backup=$4
# Spike only: the real script runs on the user's session bus.
is_private_bus "$bus" || { echo "refusing bus address ${bus@Q}" >&2; exit 1; }
: "${PLASMA_STOP:?set PLASMA_STOP}" "${PLASMA_START:?set PLASMA_START}"
case $command in
    retire) retire ;;
    restore) restore ;;
    *) usage ;;
esac

#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
package_src=$repo/kwin/ghost-shell-desktop
package_type=KWin/Script
plugin_id=ghost-shell-desktop
installed_dir=${XDG_DATA_HOME:-$HOME/.local/share}/kwin/scripts/$plugin_id
main_qml=contents/ui/main.qml
unload_polls=20
unload_poll_interval_s=0.1

usage() {
    echo "usage: ${0##*/} install|enable|disable|enabled|reload" >&2
    exit 64
}

set_enabled() {
    kwriteconfig6 --file kwinrc --group Plugins --key "${plugin_id}Enabled" "$1"
    # KWin loads and unloads scripts when it rereads its config.
    qdbus-qt6 org.kde.KWin /KWin reconfigure
}

scripting() {
    local method=$1
    shift
    qdbus-qt6 org.kde.KWin /Scripting "org.kde.kwin.Scripting.$method" "$@"
}

# KWin keeps running the copy it loaded through an upgrade, and its QML engine reuses the code it
# compiled for a path even after the script is unloaded, so the installed package is loaded again
# from a fresh copy. kwinrc is never written: a disable and an enable in a row merge into nothing,
# since KWin debounces reconfigure, and a failure between the two would leave the script disabled.
reload() {
    local fresh id poll

    # Everything that can fail without KWin comes first, so a failure never leaves it unloaded.
    scripting isScriptLoaded "$plugin_id" > /dev/null
    fresh=$(mktemp -d "${XDG_RUNTIME_DIR:?}/$plugin_id.XXXXXX")
    cp -r "$installed_dir/." "$fresh"
    scripting unloadScript "$plugin_id" > /dev/null
    # The unload is deferred, and loading under a name still in use is refused.
    for ((poll = 0; poll < unload_polls; poll++)); do
        if [[ $(scripting isScriptLoaded "$plugin_id") == false ]]; then
            break
        fi
        sleep "$unload_poll_interval_s"
    done
    id=$(scripting loadDeclarativeScript "$fresh/$main_qml" "$plugin_id")
    # Runs what was just loaded, or else starts the installed copy again, so the script keeps running.
    scripting start
    if [[ $id == -1 ]]; then
        echo "${0##*/}: KWin refused to load $fresh/$main_qml" >&2
        return 1
    fi
}

if [[ $# -ne 1 ]]; then
    usage
fi

case $1 in
    install)
        if kpackagetool6 --type "$package_type" --show "$plugin_id" > /dev/null 2>&1; then
            kpackagetool6 --type "$package_type" --upgrade "$package_src"
        else
            kpackagetool6 --type "$package_type" --install "$package_src"
        fi
        ;;
    enable)
        set_enabled true
        ;;
    disable)
        set_enabled false
        ;;
    enabled)
        # A bool is answered through the exit code alone, and a missing key reads as false, which
        # matches the package: it does not enable itself by default.
        kreadconfig6 --file kwinrc --group Plugins --key "${plugin_id}Enabled" --type bool
        ;;
    reload)
        reload
        ;;
    *)
        usage
        ;;
esac

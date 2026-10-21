#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
package_src=$repo/kwin/ghost-shell-desktop
package_type=KWin/Script
plugin_id=ghost-shell-desktop

usage() {
    echo "usage: ${0##*/} install|enable|disable|enabled" >&2
    exit 64
}

set_enabled() {
    kwriteconfig6 --file kwinrc --group Plugins --key "${plugin_id}Enabled" "$1"
    # KWin loads and unloads scripts when it rereads its config.
    qdbus-qt6 org.kde.KWin /KWin reconfigure
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
        # Exits 0 only when kwinrc enables the script; the package does not enable itself by default.
        [[ $(kreadconfig6 --file kwinrc --group Plugins --key "${plugin_id}Enabled") == true ]]
        ;;
    *)
        usage
        ;;
esac

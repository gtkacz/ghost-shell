#!/usr/bin/env bash
# Keep Konsole's shipped shortcut from competing with Ghostty at each login.
set -euo pipefail

data_home=${XDG_DATA_HOME:-$HOME/.local/share}
state_home=${XDG_STATE_HOME:-$HOME/.local/state}
IFS=: read -r -a data_dirs <<< "${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
data_dirs=("$data_home" "${data_dirs[@]}")

desktop_file() {
    local dir
    for dir in "${data_dirs[@]}"; do
        if [[ -f $dir/$1 ]]; then
            echo "$dir/$1"
            return 0
        fi
    done
    return 1
}

desktop_file applications/com.mitchellh.ghostty.desktop >/dev/null || exit 0
changed=false
for location in applications kglobalaccel; do
    konsole=$(desktop_file "$location/org.kde.konsole.desktop") || continue
    target=$data_home/$location/org.kde.konsole.desktop
    mkdir -p "${target%/*}"
    tmp=$(mktemp "${target}.XXXXXX")
    trap 'rm -f -- "$tmp"' EXIT
    # kglobalaccel entries are loaded before applications at login, so override both.
    awk '
        /^\[/ {
            entry = ($0 == "[Desktop Entry]")
            print
            if (entry) print "X-KDE-Shortcuts="
            next
        }
        entry && /^X-KDE-Shortcuts=/ { next }
        { print }
    ' "$konsole" >"$tmp"
    if ! cmp -s "$tmp" "$target"; then
        if [[ -e $target ]]; then
            mkdir -p "$state_home/ghost-shell/terminal-shortcut"
            backup=$(mktemp "$state_home/ghost-shell/terminal-shortcut/konsole.$location.XXXXXX.desktop")
            cp -p -- "$target" "$backup"
            echo "Previous Konsole launcher: $backup"
        fi
        chmod 644 "$tmp"
        mv -f -- "$tmp" "$target"
        changed=true
        echo "Disabled Konsole's default terminal shortcut in $target"
    else
        rm -f -- "$tmp"
    fi
done
if [[ $changed == true ]] && command -v kbuildsycoca6 >/dev/null; then
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
fi

# Apply immediately as well; desktop entries supply the defaults next login.
components=$(gdbus call --session --dest org.kde.kglobalaccel --object-path /kglobalaccel \
    --method org.kde.KGlobalAccel.allComponents 2>/dev/null) || exit 0
[[ $components == *org_kde_konsole_desktop* && $components == *com_mitchellh_ghostty_desktop* ]] || exit 0
gdbus call --session --dest org.kde.kglobalaccel --object-path /kglobalaccel \
    --method org.kde.KGlobalAccel.setForeignShortcutKeys \
    "['org.kde.konsole.desktop','_launch','Konsole','Konsole']" "@a(ai) []" >/dev/null
gdbus call --session --dest org.kde.kglobalaccel --object-path /kglobalaccel \
    --method org.kde.KGlobalAccel.setForeignShortcutKeys \
    "['com.mitchellh.ghostty.desktop','_launch','Ghostty','Ghostty']" "[( [201326676], )]" >/dev/null
echo "Ctrl+Alt+T assigned to Ghostty"

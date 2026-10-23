#!/usr/bin/env bash
# Installs the generated terminal themes and points zsh at Starship, after a verified backup.
# usage: scripts/install-term.sh [--dry-run]
set -euo pipefail

: "${HOME:?HOME must be set}"

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
config_home=${XDG_CONFIG_HOME:-$HOME/.config}
out=$repo/term/out
marker_begin="# >>> ghost-shell terminal >>>"
marker_end="# <<< ghost-shell terminal <<<"
dry_run=false

if [[ ${1:-} == --dry-run ]]; then
    dry_run=true
fi

# A symlinked ~/.zshrc is edited through the link so the link itself survives.
zshrc=$(readlink -f "$HOME/.zshrc")
plugin_dir=$HOME/.oh-my-zsh/custom/plugins/fast-syntax-highlighting

uv run --no-project python -I "$repo/scripts/term-theme.py" --out "$out"

targets=(
    "ghostty-config:$config_home/ghostty/config"
    "ghostty.css:$config_home/ghostty/ghost.css"
    "env.zsh:$config_home/ghost-shell/term.zsh"
    "starship.toml:$config_home/starship.toml"
    "eza-theme.yml:$config_home/eza/theme.yml"
    "fsh-ghost.ini:$config_home/fsh/ghost.ini"
)

if [[ $dry_run == true ]]; then
    echo "would back up .zshrc and the existing terminal configs"
    for pair in "${targets[@]}"; do
        echo "would write ${pair#*:}"
    done
    echo "would set ZSH_THEME=\"\" and add 'eval \"\$(starship init zsh)\"' before the zoxide init line in $zshrc"
    exit 0
fi

# Nothing below runs unless the backup was written and its checksum verified.
backup=$(bash "$repo/scripts/term-backup.sh" | tail -n 1)

for pair in "${targets[@]}"; do
    mkdir -p "$(dirname "${pair#*:}")"
    cp "$out/${pair%%:*}" "${pair#*:}"
done

tmp=$(mktemp)
awk -v begin="$marker_begin" -v end="$marker_end" '
    $0 == begin { skipping = 1; next }
    $0 == end { skipping = 0; next }
    skipping { next }
    /^ZSH_THEME=/ { print "ZSH_THEME=\"\""; next }
    /zoxide init zsh/ && !placed {
        print begin
        print "eval \"$(starship init zsh)\""
        print "source \"${XDG_CONFIG_HOME:-$HOME/.config}/ghost-shell/term.zsh\""
        print end
        placed = 1
    }
    { print }
    END {
        if (!placed) {
            print begin
            print "eval \"$(starship init zsh)\""
            print "source \"${XDG_CONFIG_HOME:-$HOME/.config}/ghost-shell/term.zsh\""
            print end
        }
    }
' "$zshrc" >"$tmp"
cat "$tmp" >"$zshrc"
rm -f "$tmp"

echo "backup: $backup"
echo "restore: tar -C \"$HOME\" -xzf \"$backup\""

# Last, so a failure here cannot hide the restore line; </dev/null keeps an interactive prompt from hanging it.
if [[ -d $plugin_dir ]]; then
    zsh -ic 'fast-theme XDG:ghost' </dev/null >/dev/null 2>&1 || echo "fast-theme failed; run: fast-theme XDG:ghost" >&2
fi

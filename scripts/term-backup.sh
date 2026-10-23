#!/usr/bin/env bash
# Backs up the dotfiles install-term.sh edits and prints the archive path on its last line.
# usage: scripts/term-backup.sh [dest_dir]    (default: $XDG_CONFIG_HOME, matching desktop-backup-*)
set -euo pipefail

: "${HOME:?HOME must be set}"

config_home=${XDG_CONFIG_HOME:-$HOME/.config}
dest=${1:-$config_home}
stamp=$(date +%Y%m%d-%H%M%S)
archive=$dest/term-backup-$stamp.tar.gz

# Relative to $HOME so the archive restores in place with `tar -C "$HOME" -xzf`.
candidates=(
    .zshrc
    "${config_home#"$HOME"/}/ghostty"
    "${config_home#"$HOME"/}/starship.toml"
    "${config_home#"$HOME"/}/eza"
    "${config_home#"$HOME"/}/fsh"
    "${config_home#"$HOME"/}/ghost-shell"
)

sources=()
for path in "${candidates[@]}"; do
    if [[ -e $HOME/$path ]]; then
        sources+=("$path")
    fi
done

if [[ ${#sources[@]} -eq 0 ]]; then
    echo "nothing to back up" >&2
    exit 1
fi

# -h stores a symlinked ~/.zshrc as its content, so a restore does not depend on the link target.
tar -C "$HOME" -czhf "$archive" "${sources[@]}"
(cd "$dest" && sha256sum "$(basename "$archive")" >"$(basename "$archive").sha256")
(cd "$dest" && sha256sum -c "$(basename "$archive").sha256" >/dev/null)

listing=$(tar -tzf "$archive")
for path in "${sources[@]}"; do
    if ! grep -q -- "^$path" <<<"$listing"; then
        echo "backup is missing $path" >&2
        exit 1
    fi
done

echo "$archive"

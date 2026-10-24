#!/usr/bin/env bash
# Wires the Claude status-line tap and the Codex hooks to ghostd, after verified backups.
# usage: scripts/install-agents.sh [--dry-run|--uninstall]
set -euo pipefail

: "${HOME:?HOME must be set}"

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ghostd=$HOME/.local/bin/ghostd
claude_settings=$HOME/.claude/settings.json
codex_hooks=$HOME/.codex/hooks.json
inner_file=${XDG_CONFIG_HOME:-$HOME/.config}/ghost-shell/claude-statusline-inner
tap_command="$ghostd claude-statusline-tap"
hook_command="$ghostd codex-hook"
stamp=$(date +%Y%m%d-%H%M%S)
mode=install

case ${1:-} in
    "") ;;
    --dry-run) mode=dry-run ;;
    --uninstall) mode=uninstall ;;
    *)
        echo "usage: $0 [--dry-run|--uninstall]" >&2
        exit 2
        ;;
esac

hooks() {
    uv run --no-project python -I "$repo/scripts/agents-hooks.py" "$@"
}

if [[ $mode == dry-run ]]; then
    [[ -x $ghostd ]] || echo "note: $ghostd is missing; run scripts/install.sh before the real run"
    for target in "$claude_settings" "$codex_hooks"; do
        if [[ -e $target ]]; then
            echo "would back up $target to $target.ghost-shell.bak.$stamp"
        else
            echo "would create $target (nothing to back up)"
        fi
    done
    echo "would point statusLine in $claude_settings at: $tap_command"
    echo "would save the previous statusLine command in $inner_file"
    echo "would add $hook_command to $codex_hooks for:"
    printf '  %s\n' SessionStart UserPromptSubmit PreToolUse PermissionRequest PostToolUse Stop Interrupt SessionEnd
    exit 0
fi

if [[ $mode == install && ! -x $ghostd ]]; then
    echo "$ghostd is missing; run scripts/install.sh first" >&2
    exit 1
fi

# Nothing is edited unless every backup was copied and compared byte for byte.
backups=()
for target in "$claude_settings" "$codex_hooks"; do
    [[ -e $target ]] || continue
    backup=$target.ghost-shell.bak.$stamp
    cp -p "$target" "$backup"
    if ! cmp -s "$target" "$backup"; then
        echo "backup of $target did not verify; nothing was changed" >&2
        exit 1
    fi
    backups+=("$target:$backup")
done

if [[ $mode == uninstall ]]; then
    hooks claude-unwrap "$claude_settings" "$inner_file"
    hooks codex-remove "$codex_hooks" "$hook_command"
else
    hooks claude-wrap "$claude_settings" "$inner_file" "$tap_command"
    mapfile -t events < <(hooks codex-merge "$codex_hooks" "$hook_command")
fi

for pair in "${backups[@]}"; do
    echo "backup: ${pair#*:}"
    echo "restore: cp ${pair#*:} ${pair%%:*}"
done
if [[ $mode == install ]]; then
    echo "codex hooks:"
    printf '  %s\n' "${events[@]}"
fi

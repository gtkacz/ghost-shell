"""Edits Claude's settings.json and Codex's hooks.json so ghostd can see the agents' sessions and usage.

usage:
    uv run --no-project python -I scripts/agents-hooks.py claude-wrap SETTINGS INNER_FILE TAP
    uv run --no-project python -I scripts/agents-hooks.py claude-unwrap SETTINGS INNER_FILE
    uv run --no-project python -I scripts/agents-hooks.py codex-merge HOOKS CMD
    uv run --no-project python -I scripts/agents-hooks.py codex-remove HOOKS CMD

claude-wrap points statusLine at the ghostd tap and saves the command it replaces in INNER_FILE, which the tap
runs afterwards. codex-merge adds one ghostd hook per event. Both are idempotent.
"""

import copy
import json
import os
import sys
import tempfile
from pathlib import Path

TAP_SUFFIX = "ghostd claude-statusline-tap"
HOOK_EVENTS = (
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "Stop",
    "Interrupt",
    "SessionEnd",
)
HOOK_TIMEOUT_S = 2


class HooksError(Exception):
    pass


def _with_command(status_line: dict, command: str) -> dict:
    return {**status_line, "type": "command", "command": command}


def wrap_claude(settings: dict, tap_command: str) -> tuple[dict, str | None]:
    wrapped = copy.deepcopy(settings)
    status_line = wrapped.get("statusLine") or {}
    current = status_line.get("command")
    if current is not None and current.endswith(TAP_SUFFIX):
        return wrapped, None
    wrapped["statusLine"] = _with_command(status_line, tap_command)
    return wrapped, current


def unwrap_claude(settings: dict, inner_command: str | None) -> dict:
    restored = copy.deepcopy(settings)
    status_line = restored.get("statusLine") or {}
    if inner_command is not None:
        restored["statusLine"] = _with_command(status_line, inner_command)
        return restored
    # Keys another tool added (padding, ...) are not ours to delete along with the tap.
    others = {key: value for key, value in status_line.items() if key not in ("type", "command")}
    if others:
        restored["statusLine"] = others
    else:
        restored.pop("statusLine", None)
    return restored


def _holds(group: dict, command: str) -> bool:
    return any(entry.get("command") == command for entry in group.get("hooks", []))


def merge_codex(hooks: dict, command: str) -> dict:
    merged = copy.deepcopy(hooks)
    events = merged.setdefault("hooks", {})
    for event in HOOK_EVENTS:
        groups = events.setdefault(event, [])
        if not any(_holds(group, command) for group in groups):
            groups.append({"hooks": [{"type": "command", "command": command, "timeout": HOOK_TIMEOUT_S}]})
    return merged


def remove_codex(hooks: dict, command: str) -> dict:
    cleaned = copy.deepcopy(hooks)
    events = {}
    for event, groups in cleaned.get("hooks", {}).items():
        kept = []
        for group in groups:
            entries = [entry for entry in group.get("hooks", []) if entry.get("command") != command]
            if entries:
                kept.append({**group, "hooks": entries})
        if kept:
            events[event] = kept
    if events:
        cleaned["hooks"] = events
    else:
        cleaned.pop("hooks", None)
    return cleaned


def _read(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        data = json.loads(path.read_text())
    except json.JSONDecodeError as error:
        raise HooksError(f"{path}: not valid JSON ({error})") from error
    if not isinstance(data, dict):
        raise HooksError(f"{path}: expected a JSON object")
    return data


def _write(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    # Same directory so os.replace is atomic and a crash never leaves a half-written config.
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False) as handle:
        handle.write(json.dumps(data, indent=2) + "\n")
    os.replace(handle.name, path)


def _run(argv: list[str]) -> None:
    command, *args = argv
    if command == "claude-wrap" and len(args) == 3:
        settings, inner_file, tap = Path(args[0]), Path(args[1]), args[2]
        wrapped, inner = wrap_claude(_read(settings), tap)
        if inner is not None:
            inner_file.parent.mkdir(parents=True, exist_ok=True)
            inner_file.write_text(inner)
        _write(settings, wrapped)
    elif command == "claude-unwrap" and len(args) == 2:
        settings, inner_file = Path(args[0]), Path(args[1])
        inner = inner_file.read_text().strip() or None if inner_file.exists() else None
        _write(settings, unwrap_claude(_read(settings), inner))
        inner_file.unlink(missing_ok=True)
    elif command == "codex-merge" and len(args) == 2:
        _write(Path(args[0]), merge_codex(_read(Path(args[0])), args[1]))
        print("\n".join(HOOK_EVENTS))
    elif command == "codex-remove" and len(args) == 2:
        _write(Path(args[0]), remove_codex(_read(Path(args[0])), args[1]))
    else:
        raise HooksError(__doc__)


def main(argv: list[str]) -> int:
    try:
        _run(argv)
    except HooksError as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

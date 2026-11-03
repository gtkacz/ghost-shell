import importlib.util
import json
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def _load():
    spec = importlib.util.spec_from_file_location("agents_hooks", REPO / "scripts/agents-hooks.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ah = _load()


def test_wrap_saves_the_inner_command_and_points_at_the_tap():
    settings = {"statusLine": {"type": "command", "command": "bun hud.ts"}, "model": "x"}
    wrapped, inner = ah.wrap_claude(settings, "/h/.local/bin/ghostd claude-statusline-tap")
    assert inner == "bun hud.ts"
    assert wrapped["statusLine"] == {"type": "command", "command": "/h/.local/bin/ghostd claude-statusline-tap"}
    assert wrapped["model"] == "x" and settings["statusLine"]["command"] == "bun hud.ts"


def test_rewrap_keeps_the_inner_command():
    wrapped, _ = ah.wrap_claude({"statusLine": {"type": "command", "command": "bun hud.ts"}}, "/h/ghostd claude-statusline-tap")
    again, inner = ah.wrap_claude(wrapped, "/h/ghostd claude-statusline-tap")
    assert inner is None and again == wrapped


def test_unwrap_restores_or_removes():
    tapped = {"statusLine": {"type": "command", "command": "/h/ghostd claude-statusline-tap"}}
    assert ah.unwrap_claude(tapped, "bun hud.ts")["statusLine"]["command"] == "bun hud.ts"
    assert "statusLine" not in ah.unwrap_claude(tapped, None)


def test_wrap_then_unwrap_keeps_the_status_lines_other_keys():
    original = {"statusLine": {"type": "command", "command": "bun hud.ts", "padding": 1}}
    wrapped, inner = ah.wrap_claude(original, "/h/ghostd claude-statusline-tap")
    assert wrapped["statusLine"]["padding"] == 1
    assert ah.unwrap_claude(wrapped, inner) == original
    tapped = {"statusLine": {"type": "command", "command": "/h/ghostd claude-statusline-tap", "padding": 1}}
    assert ah.unwrap_claude(tapped, None) == {"statusLine": {"padding": 1}}


def test_merge_adds_one_group_per_event_once():
    existing = {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "other"}]}]}}
    merged = ah.merge_codex(existing, "/h/ghostd codex-hook")
    assert set(merged["hooks"]) == set(ah.HOOK_EVENTS)
    assert len(merged["hooks"]["Stop"]) == 2
    assert merged["hooks"]["PermissionRequest"] == [{"hooks": [{"type": "command", "command": "/h/ghostd codex-hook", "timeout": 2}]}]
    assert ah.merge_codex(merged, "/h/ghostd codex-hook") == merged
    assert len(existing["hooks"]["Stop"]) == 1


def test_remove_leaves_other_hooks():
    merged = ah.merge_codex({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "other"}]}]}}, "/h/ghostd codex-hook")
    removed = ah.remove_codex(merged, "/h/ghostd codex-hook")
    assert removed == {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "other"}]}]}}
    assert ah.remove_codex(ah.merge_codex({}, "c"), "c") == {}


def test_cli_round_trips_files(tmp_path):
    settings = tmp_path / "settings.json"
    inner = tmp_path / "inner"
    settings.write_text('{"statusLine": {"type": "command", "command": "bun hud.ts"}}\n')
    ah.main(["claude-wrap", str(settings), str(inner), "/h/ghostd claude-statusline-tap"])
    assert inner.read_text() == "bun hud.ts"
    assert json.loads(settings.read_text())["statusLine"]["command"].endswith(ah.TAP_SUFFIX)
    ah.main(["claude-unwrap", str(settings), str(inner)])
    assert json.loads(settings.read_text())["statusLine"]["command"] == "bun hud.ts"
    assert not inner.exists()


def test_cli_codex_merge_prints_events_and_creates_the_file(tmp_path, capsys):
    hooks = tmp_path / "hooks.json"
    assert ah.main(["codex-merge", str(hooks), "c"]) == 0
    assert capsys.readouterr().out.split() == list(ah.HOOK_EVENTS)
    assert set(json.loads(hooks.read_text())["hooks"]) == set(ah.HOOK_EVENTS)
    assert ah.main(["codex-remove", str(hooks), "c"]) == 0
    assert json.loads(hooks.read_text()) == {}


def test_cli_rejects_malformed_json_without_writing(tmp_path, capsys):
    hooks = tmp_path / "hooks.json"
    hooks.write_text("{nope")
    assert ah.main(["codex-merge", str(hooks), "c"]) == 1
    assert hooks.read_text() == "{nope"
    assert "hooks.json" in capsys.readouterr().err


def test_cli_keeps_a_symlinked_config_and_its_mode(tmp_path):
    target = tmp_path / "dotfiles" / "settings.json"
    target.parent.mkdir()
    target.write_text("{}")
    target.chmod(0o644)
    link = tmp_path / "settings.json"
    link.symlink_to(target)
    ah.main(["claude-wrap", str(link), str(tmp_path / "inner"), "/h/ghostd claude-statusline-tap"])
    assert link.is_symlink()
    assert json.loads(target.read_text())["statusLine"]["command"].endswith("claude-statusline-tap")
    assert target.stat().st_mode & 0o777 == 0o644

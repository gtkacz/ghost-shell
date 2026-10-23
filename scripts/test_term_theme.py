import importlib.util
import textwrap
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parent.parent
DEFAULT_TOML = REPO / "crates/ghostd/src/config/default.toml"
TERM_TOML = REPO / "term/palette.toml"


def _load():
    spec = importlib.util.spec_from_file_location("term_theme", REPO / "scripts/term-theme.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


tt = _load()


def _write(tmp_path, name, body):
    path = tmp_path / name
    path.write_text(textwrap.dedent(body))
    return path


DESKTOP_OK = """
    [appearance.colors]
    ground = "#0a0f11"
    hair = "#2a3a3d"
    text = "#b7d1cd"
    hi = "#e4f1ef"
    dim = "#587170"
    signal = "#a3d4cc"
    warn = "#e9a84c"
    crit = "#ff3b45"
"""


def test_loads_desktop_colours_from_default_toml():
    p = tt.load_palette(DEFAULT_TOML, TERM_TOML)
    assert p.desktop["signal"] == "#a3d4cc"
    assert p.desktop["crit"] == "#ff3b45"


def test_overrides_replace_named_ansi_hue(tmp_path):
    term = _write(tmp_path, "palette.toml", TERM_BODY + '\n[overrides]\nred = "#ff6b81"\n')
    desktop = _write(tmp_path, "default.toml", DESKTOP_OK)
    assert tt.load_palette(desktop, term).ansi[1] == "#ff6b81"


def test_missing_desktop_key_names_the_key(tmp_path):
    desktop = _write(tmp_path, "default.toml", DESKTOP_OK.replace('    dim = "#587170"\n', ""))
    term = _write(tmp_path, "palette.toml", TERM_BODY)
    with pytest.raises(tt.PaletteError, match="dim"):
        tt.load_palette(desktop, term)


def test_malformed_hex_raises(tmp_path):
    desktop = _write(tmp_path, "default.toml", DESKTOP_OK.replace("#587170", "#12345"))
    term = _write(tmp_path, "palette.toml", TERM_BODY)
    with pytest.raises(tt.PaletteError, match="dim"):
        tt.load_palette(desktop, term)


TERM_BODY = """
    [terminal]
    ground = "#070a0b"
    opacity = 94
    cursor = "#a3d4cc"
    selection = "#2a3a3d"
    orange = "#e0875a"

    [ansi]
    normal = ["#0a0f11", "#ff3b45", "#86d19a", "#e9a84c", "#7fa6e8", "#b98ae0", "#a3d4cc", "#b7d1cd"]
    bright = ["#587170", "#ff6b73", "#a6e3b4", "#f3c274", "#a3c1f5", "#d0b0f0", "#c4e8e1", "#e4f1ef"]
"""


def _contrast_after(kind, fg, bg):
    return tt.contrast(tt.simulate(fg, kind), tt.simulate(bg, kind))


def test_simulate_protan_darkens_pure_red():
    assert _contrast_after("protan", "#ff0000", "#070a0b") < tt.contrast("#ff0000", "#070a0b")


def test_simulate_leaves_neutral_grey_unchanged():
    simulated = tt.simulate("#808080", "protan")
    assert all(abs(int(simulated[i:i + 2], 16) - 0x80) <= 1 for i in (1, 3, 5))


def _with_ansi(palette, **replacements):
    ansi = list(palette.ansi)
    slots = {name: i for i, name in enumerate(tt.ANSI_NAMES)}
    for name, value in replacements.items():
        ansi[slots[name]] = value
    return tt.Palette(**{**palette.__dict__, "ansi": tuple(ansi)})


def test_check_rejects_a_dark_red():
    palette = _with_ansi(tt.load_palette(DEFAULT_TOML, TERM_TOML), red="#8b0000")
    assert any("red" in m and "ground" in m for m in tt.check_palette(palette))


def test_check_rejects_blue_and_magenta_with_equal_lightness():
    palette = _with_ansi(tt.load_palette(DEFAULT_TOML, TERM_TOML), blue="#8080c0", magenta="#8080c0")
    assert any("blue" in m and "magenta" in m for m in tt.check_palette(palette))


def test_shipped_palette_passes():
    assert tt.check_palette(tt.load_palette(DEFAULT_TOML, TERM_TOML)) == []


def _palette():
    return tt.load_palette(DEFAULT_TOML, TERM_TOML)


def _line(text, key):
    return next(line for line in text.splitlines() if line.strip().startswith(f"{key}"))


def test_ghostty_declares_all_sixteen_palette_slots():
    p = _palette()
    out = tt.render_ghostty(p)
    for slot, colour in enumerate(p.ansi):
        assert f"palette = {slot}={colour}" in out


def test_ghostty_sets_ground_text_cursor_selection():
    out = tt.render_ghostty(_palette())
    assert "background = #070a0b" in out
    assert "foreground = #b7d1cd" in out
    assert "cursor-color = #a3d4cc" in out
    assert "selection-background = #2a3a3d" in out


def test_ghostty_uses_kode_mono_then_cjk_fallback():
    fonts = [line for line in tt.render_ghostty(_palette()).splitlines() if line.startswith("font-family")]
    assert fonts == ['font-family = "Kode Mono"', 'font-family = "Noto Sans CJK JP"']


def test_ghostty_brightens_bold_palette_text():
    assert "bold-color = bright" in tt.render_ghostty(_palette())


def test_ghostty_opacity_is_ninety_four_percent():
    assert "background-opacity = 0.94" in tt.render_ghostty(_palette())


def test_eza_write_bit_uses_orange_not_red():
    p = _palette()
    out = tt.render_eza(p)
    assert p.orange in _line(out, "user_write")
    assert p.ansi[1] not in out


def test_eza_directories_use_blue_and_sizes_green():
    p = _palette()
    out = tt.render_eza(p)
    assert p.ansi[4] in _line(out, "directory")
    assert "is_bold" in _line(out, "directory") and "is_underline" in _line(out, "directory")
    assert p.ansi[2] in _line(out, "number_kilo")


def test_fsh_valid_command_is_not_dim_or_red():
    p = _palette()
    line = _line(tt.render_fsh(p), "command ")
    assert p.desktop["dim"] not in line and p.ansi[1] not in line


def test_fsh_typed_default_text_is_body_text():
    p = _palette()
    assert p.desktop["text"] in _line(tt.render_fsh(p), "default")


ANSI_ESCAPE = __import__("re").compile(r"\x1b\[[0-9;]*m|\x01|\x02|%\{|%\}")


def _fg(colour):
    r, g, b = (int(colour[i:i + 2], 16) for i in (1, 3, 5))
    return f"\x1b[38;2;{r};{g};{b}m"


def _git(repo, *args):
    import subprocess

    subprocess.run(["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@t", *args], check=True,
                   capture_output=True)


def _repo(tmp_path, name="work"):
    repo = tmp_path / name
    repo.mkdir()
    _git(repo, "init", "-b", "main")
    (repo / "a").write_text("a")
    (repo / "b").write_text("b")
    _git(repo, "add", ".")
    _git(repo, "commit", "-m", "init")
    return repo


def _prompt(tmp_path, repo, *flags):
    import os
    import subprocess

    config = tmp_path / "starship.toml"
    config.write_text(tt.render_starship(_palette()))
    env = {**os.environ, "STARSHIP_CONFIG": str(config), "HOME": str(tmp_path), "PWD": str(repo),
           "STARSHIP_SHELL": ""}
    result = subprocess.run(["starship", "prompt", f"--path={repo}", "--terminal-width=80", *flags],
                            cwd=repo, env=env, capture_output=True, text=True, check=True)
    return result.stdout


def _plain(text):
    return ANSI_ESCAPE.sub("", text)


def test_clean_repo_shows_branch_only(tmp_path):
    out = _plain(_prompt(tmp_path, _repo(tmp_path)))
    assert "main" in out and "*" not in out


def test_two_modified_one_untracked_shows_main_star_three(tmp_path):
    repo = _repo(tmp_path)
    (repo / "a").write_text("changed")
    (repo / "b").write_text("changed")
    (repo / "c").write_text("new")
    assert "main*3" in _plain(_prompt(tmp_path, repo))


def test_no_upstream_shows_no_arrows(tmp_path):
    out = _plain(_prompt(tmp_path, _repo(tmp_path)))
    assert "⇡" not in out and "⇣" not in out


def test_ahead_one_behind_zero_shows_ahead_arrow_only(tmp_path):
    origin = _repo(tmp_path, "origin")
    import subprocess

    clone = tmp_path / "clone"
    subprocess.run(["git", "clone", str(origin), str(clone)], check=True, capture_output=True)
    (clone / "x").write_text("x")
    _git(clone, "add", ".")
    _git(clone, "commit", "-m", "ahead")
    out = _plain(_prompt(tmp_path, clone))
    assert "⇡1" in out and "⇣" not in out


def test_detached_head_shows_short_hash(tmp_path):
    repo = _repo(tmp_path)
    _git(repo, "checkout", "--detach")
    short = __import__("subprocess").run(["git", "-C", str(repo), "rev-parse", "--short", "HEAD"],
                                         capture_output=True, text=True, check=True).stdout.strip()
    assert short in _plain(_prompt(tmp_path, repo))


def test_prompt_is_a_single_line_ending_in_chevron(tmp_path):
    out = _plain(_prompt(tmp_path, _repo(tmp_path)))
    assert "\n" not in out.rstrip("\n") and out.rstrip().endswith("❯")


def test_failure_colours_chevron_with_the_red(tmp_path):
    p = _palette()
    repo = _repo(tmp_path)
    red = _fg(p.ansi[1]).removeprefix("\x1b[").removesuffix("m")
    assert f"{red}m✗" in _prompt(tmp_path, repo, "--status=101")
    assert _fg(p.desktop["signal"]) + "❯" in _prompt(tmp_path, repo, "--status=0")
    assert "❯" not in _plain(_prompt(tmp_path, repo, "--status=101"))


def test_right_prompt_shows_code_and_duration_on_failure(tmp_path):
    out = _plain(_prompt(tmp_path, _repo(tmp_path), "--right", "--status=101", "--cmd-duration=3200"))
    assert "101" in out and "3s" in out


def test_right_prompt_empty_on_fast_success(tmp_path):
    assert _plain(_prompt(tmp_path, _repo(tmp_path), "--right", "--status=0", "--cmd-duration=100")).strip() == ""


def test_chip_has_bold_hi_on_hair_background(tmp_path):
    p = _palette()
    out = _prompt(tmp_path, _repo(tmp_path))
    chip = out.split(_plain(out).split()[0])[0]

    def triple(colour):
        return ";".join(str(int(colour[i:i + 2], 16)) for i in (1, 3, 5))

    assert "\x1b[1;" in chip or "\x1b[1m" in chip
    assert f"38;2;{triple(p.desktop['hi'])}" in chip
    assert f"48;2;{triple(p.desktop['hair'])}" in chip


OUTPUTS = {"ghostty-config", "ghostty.css", "starship.toml", "eza-theme.yml", "fsh-ghost.ini", "env.zsh"}


def test_main_writes_every_file_when_palette_passes(tmp_path):
    out = tmp_path / "out"
    assert tt.main(["--out", str(out)]) == 0
    assert {f.name for f in out.iterdir()} == OUTPUTS


def test_main_refuses_to_write_when_check_fails(tmp_path, capsys):
    term = _write(tmp_path, "palette.toml", TERM_BODY)
    out = tmp_path / "out"
    assert tt.main(["--out", str(out), "--term-toml", str(term)]) != 0
    assert not out.exists() or not list(out.iterdir())
    assert "red" in capsys.readouterr().err


def test_main_check_flag_writes_nothing(tmp_path, capsys):
    out = tmp_path / "out"
    assert tt.main(["--check", "--out", str(out)]) == 0
    assert not out.exists()
    assert "protan" in capsys.readouterr().out


def test_main_reports_palette_error_with_the_key(tmp_path, capsys):
    desktop = _write(tmp_path, "default.toml", DESKTOP_OK.replace('    dim = "#587170"\n', ""))
    assert tt.main(["--out", str(tmp_path / "out"), "--default-toml", str(desktop)]) != 0
    assert "dim" in capsys.readouterr().err


ZSHRC = """export ZSH="$HOME/.oh-my-zsh"
ZSH_THEME="robbyrussell"
plugins=(
  git
  fzf
  fast-syntax-highlighting
)
source $ZSH/oh-my-zsh.sh
eval "$(atuin init zsh)"
eval "$(zoxide init zsh)"
"""


def _home(tmp_path):
    home = tmp_path / "home"
    (home / ".config/ghostty").mkdir(parents=True)
    (home / ".config/ghostty/config").write_text("old = 1\n")
    (home / ".zshrc").write_text(ZSHRC)
    return home


def _run_script(script, home, *args):
    import os
    import subprocess

    env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(home / ".config")}
    return subprocess.run(["bash", str(REPO / "scripts" / script), *args], env=env, capture_output=True, text=True,
                          cwd=REPO)


def test_backup_archive_lists_every_source_and_checksum_matches(tmp_path):
    import subprocess

    home = _home(tmp_path)
    result = _run_script("term-backup.sh", home)
    assert result.returncode == 0, result.stderr
    archive = Path(result.stdout.strip().splitlines()[-1])
    listing = subprocess.run(["tar", "-tzf", archive], capture_output=True, text=True, check=True).stdout
    assert ".zshrc" in listing and ".config/ghostty/config" in listing
    check = subprocess.run(["sha256sum", "-c", archive.name + ".sha256"], cwd=archive.parent, capture_output=True)
    assert check.returncode == 0


def test_install_aborts_without_touching_files_when_backup_fails(tmp_path):
    home = _home(tmp_path)
    before = (home / ".zshrc").read_bytes()
    (home / ".config").chmod(0o500)
    try:
        result = _run_script("install-term.sh", home)
    finally:
        (home / ".config").chmod(0o700)
    assert result.returncode != 0
    assert (home / ".zshrc").read_bytes() == before
    assert (home / ".config/ghostty/config").read_text() == "old = 1\n"


def test_install_sets_zsh_theme_empty_and_inits_starship_before_zoxide(tmp_path):
    home = _home(tmp_path)
    assert _run_script("install-term.sh", home).returncode == 0
    text = (home / ".zshrc").read_text()
    assert 'ZSH_THEME=""' in text and 'ZSH_THEME="robbyrussell"' not in text
    assert text.index("oh-my-zsh.sh") < text.index("starship init zsh") < text.index("zoxide init zsh")
    assert (home / ".config/starship.toml").exists() and (home / ".config/eza/theme.yml").exists()
    assert (home / ".config/ghostty/config").read_text().startswith("# Generated")


def test_install_twice_leaves_zshrc_identical_to_once(tmp_path):
    home = _home(tmp_path)
    _run_script("install-term.sh", home)
    once = (home / ".zshrc").read_text()
    _run_script("install-term.sh", home)
    assert (home / ".zshrc").read_text() == once


def test_dry_run_changes_nothing_and_prints_planned_edits(tmp_path):
    home = _home(tmp_path)
    result = _run_script("install-term.sh", home, "--dry-run")
    assert result.returncode == 0
    assert (home / ".zshrc").read_text() == ZSHRC
    assert (home / ".config/ghostty/config").read_text() == "old = 1\n"
    assert "starship init zsh" in result.stdout


def test_install_keeps_every_plugin_line(tmp_path):
    home = _home(tmp_path)
    _run_script("install-term.sh", home)
    plugins = ZSHRC[ZSHRC.index("plugins=("):ZSHRC.index(")\n", ZSHRC.index("plugins=(")) + 2]
    assert plugins in (home / ".zshrc").read_text()


# The styles fast-theme reports as missing when a theme omits them, so each leaves that token unstyled.
FSH_STYLES = (
    "default unknown-token reserved-word alias suffix-alias builtin function command precommand commandseparator "
    "hashed-command path pathseparator globbing globbing-ext history-expansion single-hyphen-option "
    "double-hyphen-option back-quoted-argument single-quoted-argument double-quoted-argument "
    "dollar-quoted-argument back-or-dollar-double-quoted-argument back-dollar-quoted-argument assign redirection "
    "comment variable mathvar mathnum matherr assign-array-bracket forvar fornum foroper forsep exec-descriptor "
    "here-string-tri here-string-text here-string-var secondary case-input case-parentheses case-condition "
    "correct-subtle incorrect-subtle subtle-separator subtle-bg path-to-dir paired-bracket bracket-level-1 "
    "bracket-level-2 bracket-level-3 global-alias subcommand single-sq-bracket double-sq-bracket double-paren "
    "optarg-string optarg-number recursive-base"
).split()


def test_fsh_defines_every_style_the_plugin_knows():
    defined = {line.split("=")[0].strip() for line in tt.render_fsh(_palette()).splitlines() if "=" in line}
    assert [name for name in FSH_STYLES if name not in defined] == []


def test_chip_shows_the_home_relative_path_inside_a_repo(tmp_path):
    assert "~/work" in _plain(_prompt(tmp_path, _repo(tmp_path)))


def _palette_with(tmp_path, replace_from, replace_to):
    desktop = _write(tmp_path, "default.toml", DESKTOP_OK)
    term = _write(tmp_path, "palette.toml", TERM_BODY.replace(replace_from, replace_to))
    return desktop, term


def test_missing_opacity_raises_palette_error_naming_it(tmp_path):
    desktop, term = _palette_with(tmp_path, "    opacity = 94\n", "")
    with pytest.raises(tt.PaletteError, match="opacity"):
        tt.load_palette(desktop, term)


@pytest.mark.parametrize("bad", ['"abc"', "101", "-1", "94.7"])
def test_opacity_outside_zero_to_hundred_integer_raises(tmp_path, bad):
    desktop, term = _palette_with(tmp_path, "opacity = 94", f"opacity = {bad}")
    with pytest.raises(tt.PaletteError, match="opacity"):
        tt.load_palette(desktop, term)


def test_ansi_row_that_is_a_string_raises_naming_the_row(tmp_path):
    desktop, term = _palette_with(
        tmp_path, 'normal = ["#0a0f11", "#ff3b45", "#86d19a", "#e9a84c", "#7fa6e8", "#b98ae0", "#a3d4cc", "#b7d1cd"]',
        'normal = "abcdefgh"')
    with pytest.raises(tt.PaletteError, match="ansi.normal"):
        tt.load_palette(desktop, term)


def test_failure_chevron_is_bold_as_well_as_red(tmp_path):
    out = _prompt(tmp_path, _repo(tmp_path), "--status=101")
    assert "\x1b[1;38;2;255;107;129m✗" in out


def test_bare_repo_shows_no_branch(tmp_path):
    import subprocess

    origin = _repo(tmp_path, "origin")
    bare = tmp_path / "bare.git"
    subprocess.run(["git", "clone", "--bare", str(origin), str(bare)], check=True, capture_output=True)
    assert "main" not in _plain(_prompt(tmp_path, bare))


def test_git_scans_skip_dirty_submodules_and_have_time_to_finish():
    out = tt.render_starship(_palette())
    assert out.count("--ignore-submodules=dirty") >= 3
    assert "command_timeout = 1000" in out


def test_install_still_reports_the_backup_when_fast_theme_fails(tmp_path):
    home = _home(tmp_path)
    (home / ".oh-my-zsh/custom/plugins/fast-syntax-highlighting").mkdir(parents=True)
    result = _run_script("install-term.sh", home)
    assert result.returncode == 0, result.stderr
    assert "backup:" in result.stdout and "restore:" in result.stdout


def test_gtk_css_draws_signal_left_edge_hairline_frame_and_dim_selected_tab():
    p = _palette()
    css = tt.render_gtk_css(p)
    assert f"border-left: 3px solid {p.desktop['signal']}" in css
    assert f"outline: 1px solid {p.desktop['hair']}" in css
    assert f"box-shadow: inset 0 -2px {p.desktop['dim']}" in css


def test_gtk_css_paints_the_tab_strip_on_the_dark_ground():
    p = _palette()
    assert f"tabbar {{ background: {p.ground}" in tt.render_gtk_css(p)


def test_ghostty_loads_the_gtk_css():
    assert "gtk-custom-css = ~/.config/ghostty/ghost.css" in tt.render_ghostty(_palette())


def test_env_makes_eza_draw_directories_bold_underlined_blue():
    # eza applies LS_COLORS over its theme, and LS_COLORS has a plain di entry.
    assert 'export EZA_COLORS="di=1;4;38;2;143;184;245"' in tt.render_env(_palette())


def test_install_writes_the_css_and_sources_the_env_file_once(tmp_path):
    home = _home(tmp_path)
    _run_script("install-term.sh", home)
    _run_script("install-term.sh", home)
    assert (home / ".config/ghostty/ghost.css").exists()
    assert (home / ".config/ghost-shell/term.zsh").exists()
    assert (home / ".zshrc").read_text().count("ghost-shell/term.zsh") == 1

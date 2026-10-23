# ghost-shell terminal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the user's terminal to Ghostty with a Starship prompt, themed from the ghost-shell palette and verified for protan-type colour vision, without changing any zsh or oh-my-zsh behaviour.

**Architecture:** One stdlib-only Python generator (`scripts/term-theme.py`) reads the six desktop colours from `crates/ghostd/src/config/default.toml` and the terminal-only colours from `term/palette.toml`, checks the result under a colour-vision simulation, and renders Ghostty, Starship, eza and syntax-highlighting files into `term/out/`. A shell installer backs up the user's dotfiles, verifies the backup, then installs the files and edits `~/.zshrc` idempotently.

**Tech Stack:** Python 3.11+ stdlib (`tomllib`) run through `uv`, bash, Ghostty (COPR), Starship, eza, `fast-syntax-highlighting`.

**Spec:** `docs/superpowers/specs/2026-10-22-ghost-shell-terminal-design.md` (mockups in `docs/superpowers/specs/mockups/`, newest `2026-10-22-ghost-shell-terminal-a-edge-v4.html` and `...prompt-contrast.html`)

## Global Constraints

- Python is always run through `uv`, in the repo's existing style: `uv run --no-project python -I scripts/<file>.py`. Stdlib only, no new dependencies.
- Colours: desktop palette unchanged; terminal ground about `#070a0b` at 94% opacity; font Kode Mono with Noto Sans CJK JP; cursor `#a3d4cc`; selection `#2a3a3d`.
- Red is reserved for failure. Terminal red is provisional: expected to become lighter and bluer (about `#ff6b81`); desktop `crit` `#ff3b45` is unchanged. All terminal colours are provisional until the colour-vision check passes.
- Prompt is one line: path chip, git segment, `❯`. Chip is bold `hi` text on a `hair` background. Typed text stays regular weight. No language icon, no `user@host`, no branch glyph.
- Git segment is one token: clean `main` (green); dirty `main*N` (warn-amber, N is the changed-file count); `⇡N` ahead and `⇣N` behind only when non-zero.
- After a non-zero exit `❯` is red and the right prompt shows the exit code and, for a command of 2 s or more, the duration. No exit number on the left.
- Window chrome is best-effort: no shader and no desktop-side frame. Report what Ghostty cannot do.
- Dotfiles (`~/.zshrc`, files under `~/.config/`) are edited only after a verified backup. No oh-my-zsh plugin behaviour changes. `bat`, `fzf`, `delta` and `tmux` are untouched.
- Commit only when the user asks. Commit steps below are proposals to run on request: conventional commits, no attribution trailers (the user's rules override any default).
- Comments only for a non-obvious why. Match the density of `scripts/land-mask.py` and `scripts/install.sh`.

## Review Focus

- A key missing or malformed in `default.toml` or `term/palette.toml` must stop the generator with a message naming the key, not fall back to a default. (Task 2)
- Running the installer twice must not duplicate or reorder lines in `~/.zshrc`. (Task 8)
- If the backup cannot be written or fails its checksum, no dotfile may be changed. (Task 8)
- Git edge states: a repo with no upstream, a detached HEAD, a clean tree, and a repo with 2 modified plus 1 untracked file must render `main`, a short hash, no `*`, and `main*3` respectively, with `⇡`/`⇣` only when non-zero. (Task 6)
- A terminal too narrow for the right prompt must hide it rather than wrap or overwrite the command line. (Task 6)

## File Structure

- `term/palette.toml` (create): terminal-only colours and a per-hue override table. Source of truth with `default.toml`.
- `scripts/term-theme.py` (create): loader, colour-vision checks, renderers, CLI. Single file because `python -I` does not add the script's directory to the path.
- `scripts/test_term_theme.py` (create): tests, loading the script with `importlib`.
- `scripts/term-backup.sh` (create): verified backup of the user's dotfiles.
- `scripts/install-term.sh` (create): backup, install, edit `~/.zshrc`; supports `--dry-run`.
- `term/out/` (generated, add to `.gitignore`): `ghostty-config`, `starship.toml`, `eza-theme.yml`, `fsh-ghost.ini`.

---

### Task 1: Install Ghostty and Starship

**Files:** none in the repo.

**Interfaces:**
- Produces: `ghostty` and `starship` on `PATH`.

- [ ] **Step 1: Find the Ghostty COPR for Fedora 44**

Run: `dnf copr list --available 2>/dev/null | rg -i ghostty` and check the project's install page for the Fedora 44 route. Expected: a named COPR repository. If none supports Fedora 44, stop and report the source-build alternative to the user before building.

- [ ] **Step 2: Have the user run the privileged install**

Ask the user to run `! sudo dnf copr enable <repo> && sudo dnf install ghostty starship` (substitute the repo from Step 1; fall back to `cargo install starship --locked` if Starship is not packaged).

- [ ] **Step 3: Verify**

Run: `ghostty --version && starship --version`
Expected: both print a version. Record both versions in the task report.

---

### Task 2: Palette loader

**Files:**
- Create: `term/palette.toml`, `scripts/term-theme.py`, `scripts/test_term_theme.py`

**Interfaces:**
- Produces: `@dataclass(frozen=True) class Palette` with `desktop: dict[str, str]` (keys `ground hair text hi dim signal warn crit`), `ground: str`, `opacity: int`, `cursor: str`, `selection: str`, `orange: str`, `ansi: tuple[str, ...]` (16 entries, normal 0-7 then bright 8-15, order black red green yellow blue magenta cyan white); `load_palette(default_toml: Path, term_toml: Path) -> Palette`; `class PaletteError(Exception)`.

- [ ] **Step 1: Write failing tests**

`test_loads_desktop_colours_from_default_toml` (asserts `p.desktop["signal"] == "#a3d4cc"` and `p.desktop["crit"] == "#ff3b45"` from the real `default.toml`); `test_overrides_replace_named_ansi_hue` (a temp `palette.toml` with `[overrides] red = "#ff6b81"` gives `p.ansi[1] == "#ff6b81"`); `test_missing_desktop_key_names_the_key` (temp `default.toml` without `dim` raises `PaletteError` whose message contains `dim`); `test_malformed_hex_raises` (`"#12345"` raises `PaletteError` naming the key).

- [ ] **Step 2: Run to verify failure**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py -v`
Expected: FAIL (no module/functions).

- [ ] **Step 3: Create `term/palette.toml` and implement `load_palette`**

`palette.toml` holds `[terminal]` (`ground = "#070a0b"`, `opacity = 94`, `cursor = "#a3d4cc"`, `selection = "#2a3a3d"`, `orange = "#e0875a"`), `[ansi]` with `normal` and `bright` arrays using the spec's provisional values, and an empty `[overrides]` table keyed by ansi name (`black red green yellow blue magenta cyan white`, prefix `bright-` for the bright row). `load_palette` reads with `tomllib`, validates every colour with a `#rrggbb` regex, and applies overrides last.

- [ ] **Step 4: Run to verify pass.** Same command as Step 2. Expected: 4 passed.

- [ ] **Step 5: Commit (on request):** `feat(term): load the terminal palette from the desktop config`

---

### Task 3: Colour-vision simulation and checks

**Files:**
- Modify: `scripts/term-theme.py`, `scripts/test_term_theme.py`

**Interfaces:**
- Consumes: `Palette`, `load_palette` (Task 2).
- Produces: `simulate(colour: str, kind: str, severity: float = 1.0) -> str` with `kind` in `"protan" "deutan"`; `contrast(a: str, b: str) -> float` (WCAG ratio); `check_palette(p: Palette) -> list[str]` returning one message per failed rule, empty when the palette passes.

- [ ] **Step 1: Write failing tests**

`test_simulate_protan_darkens_pure_red` (`contrast(simulate("#ff0000","protan"), "#070a0b") < contrast("#ff0000", "#070a0b")`); `test_simulate_leaves_neutral_grey_unchanged` (`simulate("#808080","protan") == "#808080"` within one unit per channel); `test_check_rejects_a_dark_red` (a palette with red `#8b0000` returns a message mentioning `red` and `ground`); `test_check_rejects_blue_and_magenta_with_equal_lightness`; `test_shipped_palette_passes` (`check_palette(load_palette(real, real)) == []`).

- [ ] **Step 2: Run to verify failure.** Expected: FAIL (`simulate` undefined).

- [ ] **Step 3: Implement `simulate`, `contrast`, `check_palette`**

`simulate` uses the Machado et al. (2009) 3x3 matrices on linear-light RGB (convert from sRGB, apply, clamp, convert back). The rules in `check_palette`, each evaluated after simulating protan at severity 1.0 and deutan at 1.0: (a) red against `ground` meets a minimum contrast ratio; (b) blue and magenta differ by a minimum lightness gap; (c) `text` and `hi`, and `dim` against `ground`, still meet legibility minimums. Choose the thresholds, state them as named constants with one line on why, and record the values in the task report. Green, yellow and orange are deliberately unchecked (the spec lets them merge).

- [ ] **Step 4: Run to verify pass.** Expected: the first four pass; `test_shipped_palette_passes` fails with the red message because `#ff3b45` is too dark.

- [ ] **Step 5: Tune and re-run**

Set `[overrides] red`, `bright-red` in `term/palette.toml` to the lightest-bluest rose that passes while still reading as red to a typical eye (start at `#ff6b81` and `#ff8fa0`), adjusting blue/magenta lightness if rule (b) fails. Expected: all pass. Print the simulated swatches (`uv run --no-project python -I scripts/term-theme.py --check`) and show them to the user for a sanity look before continuing.

- [ ] **Step 6: Commit (on request):** `feat(term): check the palette under a colour-vision simulation`

---

### Task 4: Ghostty renderer

**Files:**
- Modify: `scripts/term-theme.py`, `scripts/test_term_theme.py`

**Interfaces:**
- Consumes: `Palette`.
- Produces: `render_ghostty(p: Palette) -> str`.

- [ ] **Step 1: Write failing tests**

`test_ghostty_declares_all_sixteen_palette_slots` (output has `palette = N=#rrggbb` for N 0-15 matching `p.ansi`); `test_ghostty_sets_ground_text_cursor_selection` (`background = #070a0b`, `foreground = #b7d1cd`, `cursor-color = #a3d4cc`, `selection-background = #2a3a3d`); `test_ghostty_uses_kode_mono_then_cjk_fallback` (two `font-family` lines in that order); `test_ghostty_opacity_is_ninety_four_percent` (`background-opacity = 0.94`).

- [ ] **Step 2: Run to verify failure.** Expected: FAIL.

- [ ] **Step 3: Implement `render_ghostty`**

Also emit block cursor, `window-padding-x/y`, `window-padding-color = background`, and `background-blur` as the closest available approximation of the frame and glass. Do not emit anything for the left edge, title tag or tab strip.

- [ ] **Step 4: Run to verify pass, then validate against the real binary**

Run: `uv run --no-project python -I scripts/term-theme.py --out term/out && ghostty +validate-config --config-file=term/out/ghostty-config`
Expected: tests pass; validation reports no errors. If a key is rejected by the installed version, fix the key and note it in the report.

- [ ] **Step 5: Commit (on request):** `feat(term): render the Ghostty theme from the palette`

---

### Task 5: eza, LS_COLORS and syntax-highlighting themes

**Files:**
- Modify: `scripts/term-theme.py`, `scripts/test_term_theme.py`

**Interfaces:**
- Consumes: `Palette`.
- Produces: `render_eza(p: Palette) -> str` (a `theme.yml`); `render_fsh(p: Palette) -> str` (a `fast-syntax-highlighting` theme `.ini`).

- [ ] **Step 1: Write failing tests**

`test_eza_write_bit_uses_orange_not_red` (the theme's write-permission style is `p.orange`, and `p.ansi[1]` appears nowhere in the file); `test_eza_directories_use_blue_and_sizes_green`; `test_fsh_valid_command_is_not_dim_or_red` (the style for a valid command is neither `dim` nor the red); `test_fsh_typed_default_text_is_body_text` (default style is `text`, regular weight).

- [ ] **Step 2: Run to verify failure.** Expected: FAIL.

- [ ] **Step 3: Implement both renderers**

Mirror the mapping in the v4 mockup: permissions r yellow, w orange, x green, `-` dim; size green; user yellow; date blue; git column dim with green for new; directories blue; special files (Cargo.toml, Makefile, README) underlined warn. Follow the key names of the installed eza version (check `eza --version` and its `theme.yml` docs) and the `fast-syntax-highlighting` theme format shipped under `~/.oh-my-zsh/custom/plugins/fast-syntax-highlighting/themes/`.

- [ ] **Step 4: Run to verify pass, then load them**

Run: `uv run --no-project python -I scripts/term-theme.py --out term/out && EZA_CONFIG_DIR=term/out eza -la --color=always | head`
Expected: tests pass; eza prints a coloured listing with no theme errors.

- [ ] **Step 5: Commit (on request):** `feat(term): theme eza and syntax highlighting from the palette`

---

### Task 6: Starship renderer

**Files:**
- Modify: `scripts/term-theme.py`, `scripts/test_term_theme.py`

**Interfaces:**
- Consumes: `Palette`.
- Produces: `render_starship(p: Palette) -> str`.

- [ ] **Step 1: Write failing tests**

Each test writes `render_starship(...)` to a temp file, points `STARSHIP_CONFIG` at it, builds a temp git repo, and asserts on `starship prompt --status=<n> --cmd-duration=<ms> --terminal-width=<w>` output with ANSI stripped:
`test_clean_repo_shows_branch_only` (output contains `main` and no `*`); `test_two_modified_one_untracked_shows_main_star_three` (contains `main*3`); `test_no_upstream_shows_no_arrows`; `test_ahead_one_behind_zero_shows_ahead_arrow_only` (`⇡1`, no `⇣`); `test_detached_head_shows_short_hash`; `test_prompt_is_a_single_line_ending_in_chevron`; `test_failure_colours_chevron_with_the_red` (raw output with `--status=101` contains the escape for `p.ansi[1]` immediately before `❯`; with `--status=0` it contains `p.desktop["signal"]` instead); `test_right_prompt_shows_code_and_duration_on_failure` (`starship prompt --right --status=101 --cmd-duration=3200` contains `101` and `3.2s`); `test_right_prompt_empty_on_fast_success`; `test_chip_has_bold_hi_on_hair_background` (raw path segment carries bold, `#e4f1ef` foreground and `#2a3a3d` background).

- [ ] **Step 2: Run to verify failure.** Expected: FAIL.

- [ ] **Step 3: Implement `render_starship`**

Set `add_newline = false`, `format` to chip, git segment, character, and `right_format` to status and duration with `min_time = 2000`. If Starship's `git_status` cannot yield one summed count, add a `custom` module that prints `*N` from `git status --porcelain | wc -l`; the observable output must be exactly `main*N` in amber. Disable every language module by name so none can appear.

- [ ] **Step 4: Run to verify pass.** Expected: all pass. Then check narrow width by hand: `STARSHIP_CONFIG=term/out/starship.toml starship prompt --right --terminal-width=30 --status=101` and confirm the right segment is dropped or short enough to leave room for the command line; if not, add the zsh-side `ZLE_RPROMPT_INDENT` handling to Task 8 and say so.

- [ ] **Step 5: Commit (on request):** `feat(term): render the Starship prompt from the palette`

---

### Task 7: Generator CLI

**Files:**
- Modify: `scripts/term-theme.py`, `scripts/test_term_theme.py`, `.gitignore`

**Interfaces:**
- Consumes: `load_palette`, `check_palette`, `render_ghostty`, `render_eza`, `render_fsh`, `render_starship`.
- Produces: `main(argv: list[str] | None = None) -> int`. Flags: `--out DIR` (write the four files, default `term/out`), `--check` (print simulated swatches and the check result, write nothing). Output file names: `ghostty-config`, `starship.toml`, `eza-theme.yml`, `fsh-ghost.ini`.

- [ ] **Step 1: Write failing tests**

`test_main_writes_four_files_when_palette_passes`; `test_main_refuses_to_write_when_check_fails` (returns non-zero, writes nothing, stderr lists the failed rules); `test_main_check_flag_writes_nothing`; `test_main_reports_palette_error_with_the_key` (broken temp config exits non-zero naming the key).

- [ ] **Step 2: Run to verify failure.** Expected: FAIL.

- [ ] **Step 3: Implement `main`** with `argparse`, paths relative to the repo root found from `__file__`, and a `--default-toml`/`--term-toml` pair for tests. Add `term/out/` to `.gitignore`.

- [ ] **Step 4: Run all tests.** Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py -v`. Expected: all pass.

- [ ] **Step 5: Commit (on request):** `feat(term): add the terminal theme generator`

---

### Task 8: Backup and installer

**Files:**
- Create: `scripts/term-backup.sh`, `scripts/install-term.sh`
- Modify: `scripts/test_term_theme.py` (installer tests run against a temp `HOME`)

**Interfaces:**
- Consumes: the four files in `term/out/`.
- Produces: `term-backup.sh [dest_dir]` creates `term-backup-<YYYYMMDD-HHMMSS>.tar.gz` and a `.sha256` beside it under `~/.config/` (matching the existing `desktop-backup-*` naming), covering `~/.zshrc` and any existing `~/.config/ghostty`, `~/.config/starship.toml`, `~/.config/eza`, and the `fast-syntax-highlighting` theme directory; verifies the checksum and that the archive lists every source; exits non-zero otherwise. `install-term.sh [--dry-run]` runs the generator, then the backup, then installs.

- [ ] **Step 1: Write failing tests** (each runs the scripts with `HOME` set to a temp directory containing a fake `.zshrc` that includes `ZSH_THEME="robbyrussell"`, `source $ZSH/oh-my-zsh.sh`, and a final `eval "$(zoxide init zsh)"` line)

`test_backup_archive_lists_every_source_and_checksum_matches`; `test_install_aborts_without_touching_files_when_backup_fails` (make the destination unwritable; assert `.zshrc` is byte-identical afterwards); `test_install_sets_zsh_theme_empty_and_inits_starship_before_zoxide`; `test_install_twice_leaves_zshrc_identical_to_once` (second run produces no diff); `test_dry_run_changes_nothing_and_prints_planned_edits`; `test_install_keeps_every_plugin_line` (the `plugins=(...)` block is unchanged).

- [ ] **Step 2: Run to verify failure.** Expected: FAIL (scripts missing).

- [ ] **Step 3: Implement `term-backup.sh`.** Style as `scripts/install.sh` (`set -euo pipefail`, `: "${HOME:?}"`, `config_home=${XDG_CONFIG_HOME:-$HOME/.config}`). Follow a symlinked `~/.zshrc` to its target and back up the target.

- [ ] **Step 4: Implement `install-term.sh`.** Order is fixed: generate, back up and verify, install files (`ghostty/config`, `starship.toml`, `eza/theme.yml`, the FSH theme into the plugin's themes directory and select it), then edit `.zshrc` between `# >>> ghost-shell terminal >>>` / `# <<< ghost-shell terminal <<<` marker lines (replace the block if present, otherwise insert it): set `ZSH_THEME=""` and place `eval "$(starship init zsh)"` after the oh-my-zsh `source` line and before the `zoxide init` line, since zoxide must stay last. Print the backup path and the one-line restore command at the end.

- [ ] **Step 5: Run to verify pass.** Expected: all installer tests pass.

- [ ] **Step 6: Commit (on request):** `feat(term): add the backup and the installer for the terminal theme`

---

### Task 9: Install on this machine and verify in Ghostty

**Files:** none in the repo (changes the user's real dotfiles).

**Interfaces:**
- Consumes: Task 1 binaries, Task 7 generator, Task 8 installer.

- [ ] **Step 1: Dry run.** Run: `bash scripts/install-term.sh --dry-run`. Expected: lists the files to write and the exact `.zshrc` edits, and changes nothing. Show the output to the user.

- [ ] **Step 2: Install.** Run: `bash scripts/install-term.sh`. Expected: prints a verified backup path under `~/.config/term-backup-*` and the restore command.

- [ ] **Step 3: Verify in a new shell**

Run: `zsh -ic 'print -P "%~"; echo $ZSH_THEME; starship --version; alias | wc -l'` and open Ghostty. Expected: no startup errors, your aliases and plugins intact, the one-line prompt visible. Ask the user to confirm: path chip, git segment, red `❯` and right-side code after `false`, and `la` colours.

- [ ] **Step 4: Report the chrome gap.** List, from the actual Ghostty build, which of frame, left edge, title tag and tab strip were approximated or dropped, and the Kode Mono glyph fallbacks seen for `⇡`, `⇣`, `❯` and `ターミナル`.

---

## Self-review

- **Spec coverage:** install (T1); palette from one source plus terminal-only file and overrides (T2); protan-leaning check and the verification step (T3); Ghostty (T4); eza and syntax highlighting (T5); prompt, git segment, red chevron, right prompt, no language icons (T6); generator (T7); backup-first dotfile edits, idempotent (T8); live verification and chrome report (T9). Unbuilt by design: shader, desktop frame, `bat`/`fzf`/`delta`/`tmux`.
- **Types:** `Palette` fields and `load_palette`, `check_palette`, `render_*`, `main` names are used identically across tasks.
- **Proportion:** signatures, test names and assertions only; algorithms (the Machado matrices) are left to the implementer.

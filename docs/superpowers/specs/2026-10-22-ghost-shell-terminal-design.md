# ghost-shell terminal: design

## Intent

Make the user's terminal belong to ghost-shell while still reading as a different place from the desktop. The desktop is a HUD (dot grid, scanlines,
hairline panels, Japanese tags, translucent glass). The terminal is where work happens, so it keeps the palette, font and panel vocabulary but is
calmer, denser and opaque, with the one exception that it is the only part of the system that carries a full colour spectrum.

The user is colour-blind. A self-administered test (enchroma, six saved results: three pointing to a red-green pattern, three unclear; cone response blue 100%, green 75%, red 0%) points toward a protan type. This is not a diagnosis. The consequences for design: pure red looks dark against a near-black ground, and green, yellow and orange tend to merge. The user cannot rely on hue alone, so every distinction that matters must also be carried by lightness, weight, shape or text.

## Goals

- Switch the terminal emulator to Ghostty (user-approved direction; not yet installed) and theme it from the ghost-shell palette.
- Replace the `robbyrussell` prompt with Starship, themed to the design below.
- Keep all existing zsh, oh-my-zsh, atuin, zoxide, fzf and mise behaviour. Nothing about the shell's functionality changes.
- One source of truth for colours: the `[appearance.colors]` table of `crates/ghostd/src/config/default.toml`. Terminal configs are generated from it.

## Non-goals

- No terminal multiplexer work (tmux stays as is).
- No change to the desktop's own palette or widgets.
- No shader or CRT effects (direction C, "phosphor", was rejected).
- No language/toolchain icons in the prompt.

## Anti-goals

- Anything that makes the terminal read as just another desktop widget (rejected: a fully translucent, dotted, scanlined window).
- Information carried by colour alone (dirty state, failure, path vs typed text).
- A prompt that inserts extra lines or noise between commands.

## Decided

Window chrome is **best-effort** (the user's decision): Ghostty has no setting for a frame, a left edge accent, a border title tag or a custom tab strip, so the palette, font, cursor, padding, opacity and prompt carry the look, and the pieces below are approximated where Ghostty allows and otherwise dropped, with the builder reporting which. No shader or desktop-side frame is to be built for this. Target look (direction "A + edge", mockups in `mockups/2026-10-22-ghost-shell-terminal-a-edge-v4.html`):
- Opaque, near-black ground (about `#070a0b` at about 94%), no dot grid, a very light scanline overlay at most.
- Hairline frame (`hair`, `#2a3a3d`) with a 3px signal-coloured (`#a3d4cc`) left edge as the only strong accent.
- Title tag on the top border in small spaced caps with the Japanese label (ターミナル · TERM), matching the desktop panels.
- Tab strip above the content; the active tab is marked by a dim underline (`dim`, `#587170`), not signal, so the left edge stays the sole strong accent.
- Font: Kode Mono, with Noto Sans CJK JP for Japanese, as on the desktop. (Martian Mono with a 300/800 weight split was tried and rolled back by the user.)

Prompt (mockups: `...a-edge-v3.html`, `...prompt-contrast.html`):
- One line: prompt and the command are typed on the same line. Output starts on the next line. Nothing is inserted between commands.
- Content, left to right: working directory chip, git segment, `❯`.
- Working directory: bold text on a `hair`-tinted block (the "chip", option A). The typed command stays regular weight in the body text colour. The chip separates path from typing by shape, not hue.
- Git segment: a single token. Branch name only, with no branch glyph. Clean is plain green. Dirty is warn-amber and adds `*` plus the changed-file count (`main*2`). Ahead of upstream adds `⇡N` after it, behind adds `⇣N`, both only when non-zero.
- Exit status: no number in the prompt. After a non-zero exit the chevron becomes a bold, red `✗` (the user's choice), so a failure differs from a success by glyph and weight as well as hue. The exit code and the command duration are shown right-aligned on that line, for a failure or for a command slower than about 2s.
- No language or toolchain segment. `user@host` is dropped.

Colour (palette strip in `...a-edge-v4.html`):
- The values below are **provisional**: they are the colours shown in the mockups and must pass the colour-vision check in "Accessibility" before they are final. Expected change: the terminal's red becomes lighter and bluer (about `#ff6b81`, a rose) so it stays visible to a protan eye; the desktop's `crit` is unchanged.
- The desktop palette is unchanged. The terminal uses a full eight-hue ANSI set at similar lightness: red `#ff3b45` (the desktop's `crit`, reserved for failure; see the expected change above), green `#86d19a`, yellow `#e9a84c` (`warn`), blue `#7fa6e8`, magenta `#b98ae0`, cyan `#a3d4cc` (`signal`), white `#b7d1cd` (`text`), black `#0a0f11`. Brights: `#587170`, `#ff6b73`, `#a6e3b4`, `#f3c274`, `#a3c1f5`, `#d0b0f0`, `#c4e8e1`, `#e4f1ef`.
- Orange `#e0875a` is used for tokens that would otherwise tempt red (for example `eza`'s write permission bit), so a red element always means something went wrong.
- Command output keeps its programs' own colours (colourful `eza`, `git`, `cargo` output), as in the user's current terminal. In `eza`, directories are bold, underlined and blue, so weight is not the only cue. Ghostty's `bold-color = bright` lightens bold text that uses palette colours.
- Cursor and selection: cursor `#a3d4cc`, selection background `#2a3a3d`.

Other:
- File-type icons in `eza` are kept (user's current setup); only the prompt's language icon was ruled out.

## Accessibility

- Red versus green must never be the only cue. Dirty state is carried by `*N`, failure by the code on the right and the words `FAILED` and `error`, path versus typing by the chip.
- Red text must stay legible on the terminal ground under a protan simulation (a minimum lightness contrast against the ground).
- Green, yellow and orange may collapse into one another under a protan simulation, so nothing meaningful may depend on telling them apart without a text cue. Blue versus magenta must stay separable by lightness.
- The generator must include a verification step that runs a colour-vision simulation (protanopia and protanomaly at minimum, deuteranopia as a second check) over the final palette and fails if those pairs fall below its thresholds. The thresholds are for the builder to choose.

## Constraints

- Platform: Fedora (KDE, Wayland). Ghostty and Starship are not installed. Ghostty on Fedora comes from a COPR repository or a source build (not verified for this release).
- The user has authorised installing Ghostty and Starship (enabling the COPR repository for Ghostty, a package or `cargo` install for Starship), and editing their dotfiles (`~/.zshrc`, and files under `~/.config/`) **after backing each up**. A backup must exist and be checked before any dotfile is changed.
- Kode Mono is already installed (`~/.local/share/fonts/ghost-shell/`). No Nerd Font symbols font is installed; Ghostty ships its own symbol fallback, so none needs installing for `eza --icons` (the builder should confirm this on the installed version).
- The user's global rules apply: `uv` for any Python, `bun` for JS where new, Rust-backed tools preferred, no comments except for non-obvious reasons, no attribution lines, no feature branches unless asked.
- The user's current `.zshrc` sets `ZSH_THEME="robbyrussell"` and loads a long omz plugin list ending in `fast-syntax-highlighting` and `zsh-autocomplete`. Plugin behaviour is not to be changed.

## Calls made without the user (marked as the author's, open to change)

- Colours not in `[appearance.colors]` (ANSI hues, brights, orange, cursor, selection, terminal ground) live in a terminal-only palette file in `term/`. The generator reads the six desktop colours (`ground`, `hair`, `text`, `dim`, `signal`, `warn`, `crit`) from `default.toml` and the rest from that file, so `ghostd` and its config schema are untouched. The terminal palette file also holds a per-hue override table, so the palette can be adjusted for the user's colour vision without changing the generator.
- The right-aligned exit code and duration use Starship's normal right prompt: they appear on the line being edited after the failure, next to the red `❯`, as in the v3 and v4 mockups. They do not appear on the already-executed line. If `zsh-autocomplete` or `fast-syntax-highlighting` makes the right prompt misbehave, dropping the right prompt and putting the code on the left is an acceptable fallback; the red `❯` carries the failure either way.
- A `fast-syntax-highlighting` theme file counts as configuration, not a change to plugin behaviour, and is part of "done" if the default theme makes typed commands illegible on the new ground.
- The Ghostty opacity and decoration values are set to approximate the 94% opaque ground; the mockups are a guide, not a pixel target.
- `bat`, `fzf`, `delta` and the `tmux` status line are untouched.

## Left to the builder

- Exact Ghostty option names and how to express the frame, left edge, title tag and tab strip within what Ghostty can render (some of this may need approximations; report what cannot be done).
- How to generate the palettes from `default.toml` (a small script, a Rust or Python tool run through `uv`) and where it lives (suggested: `term/` in this repo).
- Starship module configuration, including how to draw the path chip with a background and the right-aligned code and duration.
- The `eza` colour mapping (`EZA_COLORS`/theme file) and `LS_COLORS` so listings match the palette.
- How to tell the colour of the typed command from syntax highlighting (`fast-syntax-highlighting` theme) so valid commands and arguments stay legible.
- A colour-vision check of the final palette once the user's type is known.

## Open items

- The user's colour vision is only known from a self-test (protan-leaning); the palette is provisional until the simulation check passes.
- Install method for Ghostty (COPR vs source): the builder chooses, preferring COPR.
- Whether to also theme `bat`, `fzf`, `delta` and the `tmux` status line to the palette (not asked for yet).

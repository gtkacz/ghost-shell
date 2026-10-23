"""Generates the terminal's Ghostty, Starship, eza and syntax-highlighting themes from the ghost-shell palette.

usage:
    uv run --no-project python -I scripts/term-theme.py [--out term/out]
    uv run --no-project python -I scripts/term-theme.py --check

The desktop colours come from crates/ghostd/src/config/default.toml and the terminal-only ones from
term/palette.toml. The palette is checked under a colour-vision simulation before anything is written.
"""

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

DESKTOP_KEYS = ("ground", "hair", "text", "hi", "dim", "signal", "warn", "crit")
ANSI_NAMES = ("black", "red", "green", "yellow", "blue", "magenta", "cyan", "white")
HEX = re.compile(r"^#[0-9a-fA-F]{6}$")


class PaletteError(Exception):
    pass


@dataclass(frozen=True)
class Palette:
    desktop: dict[str, str]
    ground: str
    opacity: int
    cursor: str
    selection: str
    orange: str
    ansi: tuple[str, ...]


def _colour(value: object, key: str) -> str:
    if not isinstance(value, str) or not HEX.match(value):
        raise PaletteError(f"{key}: expected #rrggbb, got {value!r}")
    return value.lower()


def _section(data: dict, path: str, source: Path) -> dict:
    node = data
    for part in path.split("."):
        if not isinstance(node, dict) or part not in node:
            raise PaletteError(f"{source}: missing [{path}]")
        node = node[part]
    return node


def load_palette(default_toml: Path, term_toml: Path) -> Palette:
    with default_toml.open("rb") as handle:
        colors = _section(tomllib.load(handle), "appearance.colors", default_toml)
    desktop = {}
    for key in DESKTOP_KEYS:
        if key not in colors:
            raise PaletteError(f"{default_toml}: missing appearance.colors.{key}")
        desktop[key] = _colour(colors[key], key)

    with term_toml.open("rb") as handle:
        term = tomllib.load(handle)
    terminal = _section(term, "terminal", term_toml)
    rows = _section(term, "ansi", term_toml)
    ansi = []
    for row in ("normal", "bright"):
        values = rows.get(row)
        if not isinstance(values, list) or len(values) != len(ANSI_NAMES):
            raise PaletteError(f"ansi.{row}: expected a list of {len(ANSI_NAMES)} colours")
        ansi.extend(_colour(v, f"ansi.{row}[{i}]") for i, v in enumerate(values))

    overrides = term.get("overrides", {})
    slots = {name: i for i, name in enumerate(ANSI_NAMES)}
    slots.update({f"bright-{name}": i + len(ANSI_NAMES) for i, name in enumerate(ANSI_NAMES)})
    for name, value in overrides.items():
        if name not in slots:
            raise PaletteError(f"overrides.{name}: unknown ansi name")
        ansi[slots[name]] = _colour(value, f"overrides.{name}")

    opacity = terminal.get("opacity")
    if isinstance(opacity, bool) or not isinstance(opacity, int) or not 0 <= opacity <= 100:
        raise PaletteError(f"terminal.opacity: expected an integer from 0 to 100, got {opacity!r}")

    return Palette(
        desktop=desktop,
        ground=_colour(terminal.get("ground"), "terminal.ground"),
        opacity=opacity,
        cursor=_colour(terminal.get("cursor"), "terminal.cursor"),
        selection=_colour(terminal.get("selection"), "terminal.selection"),
        orange=_colour(terminal.get("orange"), "terminal.orange"),
        ansi=tuple(ansi),
    )


# Machado, Oliveira and Fernandes (2009), full-severity matrices on linear-light RGB.
CVD_MATRICES = {
    "protan": (
        (0.152286, 1.052583, -0.204868),
        (0.114503, 0.786281, 0.099216),
        (-0.003882, -0.048116, 1.051998),
    ),
    "deutan": (
        (0.367322, 0.860646, -0.227968),
        (0.280085, 0.672501, 0.047413),
        (-0.011820, 0.042940, 0.968881),
    ),
}
IDENTITY = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0))


def _to_linear(channel: int) -> float:
    c = channel / 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def _from_linear(value: float) -> int:
    v = min(1.0, max(0.0, value))
    c = v * 12.92 if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055
    return round(c * 255)


def _rgb(colour: str) -> tuple[int, int, int]:
    return int(colour[1:3], 16), int(colour[3:5], 16), int(colour[5:7], 16)


def simulate(colour: str, kind: str, severity: float = 1.0) -> str:
    """Simulates a colour-vision deficiency; severity blends linearly between normal vision and the full matrix."""
    full = CVD_MATRICES[kind]
    matrix = tuple(
        tuple((1 - severity) * IDENTITY[r][c] + severity * full[r][c] for c in range(3)) for r in range(3)
    )
    linear = [_to_linear(v) for v in _rgb(colour)]
    out = [_from_linear(sum(matrix[r][c] * linear[c] for c in range(3))) for r in range(3)]
    return "#{:02x}{:02x}{:02x}".format(*out)


def _luminance(colour: str) -> float:
    r, g, b = (_to_linear(v) for v in _rgb(colour))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a: str, b: str) -> float:
    hi, lo = sorted((_luminance(a), _luminance(b)), reverse=True)
    return (hi + 0.05) / (lo + 0.05)


# Red only has to clear the WCAG floor for large text and UI parts, because making it brighter than that turns it
# into a light grey-tan for a protan eye, which is what hid a near-limit usage bar.
MIN_RED_CONTRAST = 3.0
# Red must stay clearly darker than green and cyan under protan, so danger is recognisable by darkness.
MIN_RED_GAP = 2.5
# Blue and magenta are the pair a protan eye merges once hue is gone, so they must differ in lightness.
MIN_BLUE_MAGENTA_CONTRAST = 1.25
# Body text and the dimmest caption colour must stay readable on the terminal ground.
MIN_TEXT_CONTRAST = 7.0
MIN_DIM_CONTRAST = 3.0
SIMULATED = ("protan", "deutan")


def check_palette(p: Palette) -> list[str]:
    """Returns one message per rule the palette fails under protan and deutan simulation; empty means it passes."""
    slot = {name: p.ansi[i] for i, name in enumerate(ANSI_NAMES)}
    failures = []
    for kind in SIMULATED:
        def seen(colour: str) -> str:
            return simulate(colour, kind)

        ground = seen(p.ground)
        ratio = contrast(seen(slot["red"]), ground)
        if ratio < MIN_RED_CONTRAST:
            failures.append(f"{kind}: red on ground contrast {ratio:.2f} < {MIN_RED_CONTRAST}")
        if kind == "protan":
            for other in ("green", "cyan"):
                ratio = contrast(seen(slot["red"]), seen(slot[other]))
                if ratio < MIN_RED_GAP:
                    failures.append(f"{kind}: red vs {other} contrast {ratio:.2f} < {MIN_RED_GAP}")
        ratio = contrast(seen(slot["blue"]), seen(slot["magenta"]))
        if ratio < MIN_BLUE_MAGENTA_CONTRAST:
            failures.append(f"{kind}: blue vs magenta contrast {ratio:.2f} < {MIN_BLUE_MAGENTA_CONTRAST}")
        ratio = contrast(seen(p.desktop["text"]), ground)
        if ratio < MIN_TEXT_CONTRAST:
            failures.append(f"{kind}: text on ground contrast {ratio:.2f} < {MIN_TEXT_CONTRAST}")
        ratio = contrast(seen(p.desktop["dim"]), ground)
        if ratio < MIN_DIM_CONTRAST:
            failures.append(f"{kind}: dim on ground contrast {ratio:.2f} < {MIN_DIM_CONTRAST}")
    return failures


def render_ghostty(p: Palette) -> str:
    # Ghostty cannot draw the frame, the signal edge, the title tag or a tab strip, so only the colours,
    # fonts, padding and glass are set here.
    lines = [
        "# Generated by scripts/term-theme.py; edit term/palette.toml instead.",
        'font-family = "Kode Mono"',
        'font-family = "Noto Sans CJK JP"',
        "bold-color = bright",
        f"background = {p.ground}",
        f"foreground = {p.desktop['text']}",
        f"cursor-color = {p.cursor}",
        f"selection-background = {p.selection}",
        f"background-opacity = {p.opacity / 100:.2f}",
        "background-blur = true",
        "cursor-style = block",
        "window-padding-x = 16",
        "window-padding-y = 12",
        "window-padding-color = background",
        # New windows would otherwise open where the previous one was; tabs and splits keep inheriting.
        "working-directory = home",
        "window-inherit-working-directory = false",
        "gtk-custom-css = ~/.config/ghostty/ghost.css",
    ]
    lines += [f"palette = {slot}={colour}" for slot, colour in enumerate(p.ansi)]
    return "\n".join(lines) + "\n"


def render_gtk_css(p: Palette) -> str:
    d = p.desktop
    # `.window` rather than `.background` is the node that takes the left border below the title bar. The title bar
    # is drawn by KWin on KDE, so neither CSS nor window-titlebar-* reaches it.
    return f"""/* Generated by scripts/term-theme.py; edit term/palette.toml instead. */
window {{ outline: 1px solid {d['hair']}; outline-offset: -1px; }}
.window {{ border-left: 3px solid {d['signal']}; }}
tabbar {{ background: {p.ground}; border-bottom: 1px solid {d['hair']}; }}
tabbar tab {{ color: {d['dim']}; background: transparent; border-radius: 0; border-right: 1px solid {d['hair']}; }}
tabbar tab:selected {{ color: {d['hi']}; background: {d['ground']}; box-shadow: inset 0 -2px {d['dim']}; }}
"""


def render_env(p: Palette) -> str:
    r, g, b = _rgb(p.ansi[4])
    return (
        "# Generated by scripts/term-theme.py; edit term/palette.toml instead.\n"
        f'export EZA_COLORS="di=1;4;38;2;{r};{g};{b}"\n'
    )


def _style(colour: str, *flags: str) -> str:
    extra = "".join(f", {flag}: true" for flag in flags)
    return f'{{foreground: "{colour}"{extra}}}'


def render_eza(p: Palette) -> str:
    red_free = {
        "yellow": p.ansi[3], "green": p.ansi[2], "blue": p.ansi[4], "magenta": p.ansi[5],
        "cyan": p.ansi[6], "orange": p.orange, "dim": p.desktop["dim"], "text": p.desktop["text"],
        "hi": p.desktop["hi"],
    }
    c = red_free
    sections = {
        "filekinds": {
            "normal": _style(c["text"]), "directory": _style(c["blue"], "is_bold", "is_underline"),
            "symlink": _style(c["cyan"]), "pipe": _style(c["yellow"]),
            "block_device": _style(c["yellow"], "is_bold"), "char_device": _style(c["yellow"], "is_bold"),
            "socket": _style(c["magenta"]), "special": _style(c["orange"]),
            "executable": _style(c["green"], "is_bold"), "mount_point": _style(c["blue"], "is_underline"),
        },
        "perms": {
            "user_read": _style(c["yellow"]), "user_write": _style(c["orange"]),
            "user_execute_file": _style(c["green"]), "user_execute_other": _style(c["green"]),
            "group_read": _style(c["yellow"]), "group_write": _style(c["orange"]),
            "group_execute": _style(c["green"]), "other_read": _style(c["yellow"]),
            "other_write": _style(c["orange"]), "other_execute": _style(c["green"]),
            "special_user_file": _style(c["magenta"]), "special_other": _style(c["magenta"]),
            "attribute": _style(c["dim"]),
        },
        "size": {
            **{f"number_{u}": _style(c["green"]) for u in ("byte", "kilo", "mega", "giga", "huge")},
            **{f"unit_{u}": _style(c["dim"]) for u in ("byte", "kilo", "mega", "giga", "huge")},
        },
        "users": {
            "user_you": _style(c["yellow"]), "user_root": _style(c["orange"], "is_bold"),
            "user_other": _style(c["yellow"]), "group_yours": _style(c["yellow"]),
            "group_other": _style(c["dim"]), "group_root": _style(c["orange"], "is_bold"),
        },
        "links": {"normal": _style(c["cyan"]), "multi_link_file": _style(c["orange"])},
        "git": {
            "new": _style(c["green"]), "modified": _style(c["yellow"]), "deleted": _style(c["orange"]),
            "renamed": _style(c["blue"]), "typechange": _style(c["magenta"]),
            "ignored": _style(c["dim"]), "conflicted": _style(c["orange"], "is_bold"),
        },
        "git_repo": {
            "branch_main": _style(c["green"]), "branch_other": _style(c["yellow"]),
            "git_clean": _style(c["green"]), "git_dirty": _style(c["yellow"]),
        },
        "file_type": {
            "image": _style(c["magenta"]), "video": _style(c["magenta"]), "music": _style(c["cyan"]),
            "lossless": _style(c["cyan"]), "crypto": _style(c["orange"]), "compressed": _style(c["orange"]),
            "temp": _style(c["dim"]), "build": _style(c["yellow"], "is_underline"),
        },
    }
    singles = {
        "date": _style(c["blue"]), "header": _style(c["hi"], "is_underline"), "punctuation": _style(c["dim"]),
        "symlink_path": _style(c["cyan"]), "control_char": _style(c["orange"]),
        "broken_symlink": _style(c["orange"]), "broken_path_overlay": _style(c["dim"], "is_underline"),
    }
    lines = ["# Generated by scripts/term-theme.py; edit term/palette.toml instead."]
    for section, entries in sections.items():
        lines.append(f"{section}:")
        lines += [f"  {key}: {value}" for key, value in entries.items()]
    lines += [f"{key}: {value}" for key, value in singles.items()]
    return "\n".join(lines) + "\n"


def render_fsh(p: Palette) -> str:
    d = p.desktop
    a = dict(zip(ANSI_NAMES, p.ansi[: len(ANSI_NAMES)]))
    bg = f"bg:{d['hair']}"
    # A valid command is teal and an unknown one is red, bold and underlined, so the difference is carried by
    # more than hue. Every style is named because fast-theme leaves an omitted one unstyled. `secondary` names a
    # built-in theme, which fast-theme loads, so it cannot be a colour.
    sections = {
        "base": {
            "default": d["text"], "unknown-token": f"{a['red']},bold,underline", "commandseparator": "none",
            "redirection": "none", "comment": d["dim"], "correct-subtle": d["dim"],
            "incorrect-subtle": a["red"], "subtle-separator": d["dim"], "subtle-bg": bg,
            "secondary": "free", "recursive-base": "none",
        },
        "command-point": {
            "reserved-word": a["yellow"], "subcommand": a["yellow"], "alias": a["cyan"],
            "suffix-alias": a["cyan"], "global-alias": a["cyan"], "builtin": a["cyan"],
            "function": a["cyan"], "command": a["cyan"], "precommand": a["cyan"],
            "hashed-command": a["cyan"], "single-sq-bracket": a["cyan"], "double-sq-bracket": a["cyan"],
            "double-paren": a["yellow"],
        },
        "paths": {
            "path": a["blue"], "pathseparator": "none", "path-to-dir": f"{a['blue']},underline",
            "globbing": f"{p.orange},bold", "globbing-ext": p.orange,
        },
        "brackets": {
            "paired-bracket": bg, "bracket-level-1": f"{a['cyan']},bold",
            "bracket-level-2": f"{a['yellow']},bold", "bracket-level-3": f"{a['magenta']},bold",
        },
        "arguments": {
            "single-hyphen-option": a["magenta"], "double-hyphen-option": a["magenta"],
            "back-quoted-argument": "none", "single-quoted-argument": a["yellow"],
            "double-quoted-argument": a["yellow"], "dollar-quoted-argument": a["yellow"],
            "back-or-dollar-double-quoted-argument": a["cyan"], "back-dollar-quoted-argument": a["cyan"],
            "optarg-string": a["yellow"], "optarg-number": a["magenta"],
        },
        "other": {
            "variable": p.orange, "assign": "none", "assign-array-bracket": a["cyan"],
            "history-expansion": f"{a['blue']},bold", "exec-descriptor": f"{a['yellow']},bold",
            "here-string-tri": a["yellow"], "here-string-text": d["dim"], "here-string-var": a["cyan"],
        },
        "math": {"mathvar": f"{a['blue']},bold", "mathnum": a["magenta"], "matherr": a["red"]},
        "for-loop": {
            "forvar": "none", "fornum": a["magenta"], "foroper": a["yellow"], "forsep": f"{a['yellow']},bold",
        },
        "case": {"case-input": a["cyan"], "case-parentheses": a["yellow"], "case-condition": bg},
    }
    lines = ["; Generated by scripts/term-theme.py; edit term/palette.toml instead."]
    for section, entries in sections.items():
        lines += ["", f"[{section}]"]
        lines += [f"{key:<20} = {value}" for key, value in entries.items()]
    return "\n".join(lines) + "\n"


# Submodule dirtiness is skipped so a large submodule cannot stall the prompt.
GIT_DIRTY_COUNT = "git status --porcelain --ignore-submodules=dirty 2>/dev/null | wc -l"
# A detached HEAD has no branch name, so the short hash stands in for it.
GIT_REF = 'b=$(git branch --show-current); [ -n "$b" ] || b=$(git rev-parse --short HEAD)'


def render_starship(p: Palette) -> str:
    d = p.desktop
    green, amber, red = p.ansi[2], d["warn"], p.ansi[1]
    return f"""# Generated by scripts/term-theme.py; edit term/palette.toml instead.
add_newline = false
command_timeout = 1000
format = "$directory${{custom.git_dirty}}${{custom.git_clean}}$git_status$character"
right_format = "$status$cmd_duration"

[directory]
format = "[ $path ]($style) "
style = "bold fg:{d['hi']} bg:{d['hair']}"
truncation_length = 3
truncate_to_repo = false
truncation_symbol = "…/"

# Starship's git_status cannot sum changed files into one count, so the branch token is drawn by two
# custom modules that are mutually exclusive.
[custom.git_dirty]
when = 'test "$({GIT_DIRTY_COUNT})" -gt 0'
command = '{GIT_REF}; printf "%s*%s" "$b" "$({GIT_DIRTY_COUNT} | tr -d " ")"'
format = "[$output]($style) "
style = "fg:{amber}"
shell = ["bash", "--noprofile", "--norc"]

[custom.git_clean]
# rev-parse exits 0 inside a bare repo too, so its printed value is what counts.
when = '[ "$(git rev-parse --is-inside-work-tree 2>/dev/null)" = true ] && test "$({GIT_DIRTY_COUNT})" -eq 0'
command = '{GIT_REF}; printf "%s" "$b"'
format = "[$output]($style) "
style = "fg:{green}"
shell = ["bash", "--noprofile", "--norc"]

[git_status]
format = "[$ahead_behind]($style) "
style = "fg:{d['signal']}"
ahead = "⇡${{count}}"
behind = "⇣${{count}}"
diverged = "⇡${{ahead_count}}⇣${{behind_count}}"
up_to_date = ""

[character]
format = "$symbol "
success_symbol = "[❯](fg:{d['signal']})"
# A different glyph as well as bold red, so a failure still differs from a success when the right prompt is hidden.
error_symbol = "[✗](bold fg:{red})"

[status]
disabled = false
format = "[$status]($style) "
style = "fg:{red}"

[cmd_duration]
min_time = 2000
format = "[$duration]($style)"
style = "fg:{d['dim']}"
"""


REPO = Path(__file__).resolve().parent.parent
OUTPUTS = {
    "ghostty-config": render_ghostty,
    "ghostty.css": render_gtk_css,
    "env.zsh": render_env,
    "starship.toml": render_starship,
    "eza-theme.yml": render_eza,
    "fsh-ghost.ini": render_fsh,
}


def _swatches(p: Palette) -> str:
    rows = []
    for kind in SIMULATED:
        seen = " ".join(f"{name}={simulate(p.ansi[i], kind)}" for i, name in enumerate(ANSI_NAMES))
        rows.append(f"{kind}: {seen}")
    return "\n".join(rows)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Generate the terminal themes from the ghost-shell palette.")
    parser.add_argument("--out", type=Path, default=REPO / "term/out")
    parser.add_argument("--check", action="store_true", help="print the simulated swatches and the check result")
    parser.add_argument("--default-toml", type=Path, default=REPO / "crates/ghostd/src/config/default.toml")
    parser.add_argument("--term-toml", type=Path, default=REPO / "term/palette.toml")
    args = parser.parse_args(argv)

    try:
        palette = load_palette(args.default_toml, args.term_toml)
    except PaletteError as error:
        print(f"palette error: {error}", file=sys.stderr)
        return 1

    failures = check_palette(palette)
    if args.check:
        print(_swatches(palette))
        print("\n".join(failures) if failures else "colour-vision check: pass")
        return 1 if failures else 0
    if failures:
        print("colour-vision check failed:\n" + "\n".join(failures), file=sys.stderr)
        return 1

    args.out.mkdir(parents=True, exist_ok=True)
    for name, render in OUTPUTS.items():
        (args.out / name).write_text(render(palette))
    return 0


if __name__ == "__main__":
    sys.exit(main())

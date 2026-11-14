# Tron Legacy palette research

## Evidence

- FrameThrower's colour palette for TRON: Legacy (2010), measured across 79
  frames shot by Claudio Miranda:
  https://framethrower.ai/movie-color-palettes/tron-legacy-2010
  Reports "natural, cool and dark" frame colours `#060a0c`, `#fbfcfd`,
  `#2b2c2c`, `#061a25`. The measured darks are near-neutral to blue-black: the
  Grid's famous cyan lives in glowing light lines, not in the ground. This
  supports a dark blue-leaning ground with luminous cyan accents.
- American Society of Cinematographers' interview with Claudio Miranda, ASC:
  https://theasc.com/video/claudio-miranda-asc-tron-legacy/
  Shooting at low light levels; the look is built from dark fields and
  luminous accents rather than mid-tones.
- Wikipedia's Tron: Legacy article confirms Joseph Kosinski as director,
  Claudio Miranda as cinematographer and Darren Gilford as production
  designer: https://en.wikipedia.org/wiki/Tron:_Legacy

Exact proposed hex values below are our design adaptations, not official brand
tokens or measurements sampled from film frames.

## Design

The film's factions supply the accent pairing: Tron and Sam's cyan-white light
(light ribbons, identity discs, suit lines) and Clu's orange (sentries,
recognizers, derezz effects). Keeping warm colours reserved for alerts, Clu's
orange becomes `warn` and red stays `crit`, matching Section Nine's pairing.
The value hierarchy follows the Netrunner preset: pale near-white text, cool
muted captions, saturated cyan signal, tinted surfaces.

| Token | Color | Role |
| --- | --- | --- |
| ground | `#050a14` | The Grid's night void, blue-black |
| panel | `#0a1424` | Lifted navy panel surface |
| hair | `#1e3558` | Blue panel edges |
| hair2 | `#12233d` | Recessed dividers |
| text | `#d9e4f7` | Blue-leaning ice body text |
| hi | `#2b8cff` | Deep Tron-blue headings and values |
| dim | `#7e93b5` | Muted blue-grey captions |
| signal | `#62b8ff` | Electric blue live data |
| warn | `#ffb02e` | Clu's amber-orange for warnings |
| crit | `#ff3f37` | Red-orange danger |
| dot | `#62b8ff` | Blue background grid |
| burn | `#d2e2ff` | Pale blue greeting flash |

Terminal-only colors live in `term/tron-legacy.toml`; blue stays lighter than
magenta for color-vision separation, as in the other presets.

## Live feedback iterations

First trial shipped with `hi = #82e9ff`, a near-white cyan. In use it read as
washed out — Section Nine with the saturation raised — and was shifted to the
deeper `#19c8f5` (`burn` retinted to match).

Second round of feedback: the whole cyan family still sat too close to Section
Nine's teal-cyan identity. Every cool token moved from cyan (~195° hue) to
actual blue (~210°): signal `#3fd8f5` → `#62b8ff`, hi `#19c8f5` → `#2b8cff`,
with text, dim, hair, hair2, panel, ground, the terminal selection and the ANSI
slots all de-cyaned the same way. The protan red-versus-cyan-slot floor (2.50)
shaped the choice: a mid blue like `#47a2ff` left only 2.14, so the signal
landed on `#62b8ff` (gap 2.63). The ANSI blue slot became a paler periwinkle
(`#a2c8ff`) so commands (cyan slot) and paths (blue slot) stay separated by
lightness now that both are blue. Lighter-bluer hi candidates (`#4fd7ff`,
`#5fd4ff`) had already collapsed to a 1.00–1.05 step against signal, which
would have merged headings with live data in normal vision; `#2b8cff` keeps a
1.56 step and follows Netrunner's precedent of a deeper saturated heading
colour over lighter body text.

## Local checks

Reused the Machado full-severity simulation and contrast functions in
`scripts/term-theme.py` without changing them. Against `#050a12`:

| Token | Normal contrast | Protan simulation | Deutan simulation |
| --- | --- | --- | --- |
| Tron blue hi #2b8cff | 5.94 | #4d97ff · 6.73 | #1384fd · 5.44 |
| Electric blue signal #62b8ff | 9.26 | #99baff · 10.14 | #84aafe · 8.68 |
| Amber warn #ffb02e | 10.85 | #ccb40d · 9.47 | #dfc733 · 11.70 |
| Red crit #ff3f37 | 5.68 | #7b6e34 · 3.86 | #aa982e · 6.86 |
| Text #d9e4f7 | 15.46 | #dee5f8 · 15.62 | #dbe2f7 · 15.38 |
| Dim #7e93b5 | 6.35 | #8795b7 · 6.57 | #828fb4 · 6.19 |

Red versus green is 3.30 and red versus the cyan slot 2.63 under protan (floor
2.50); blue versus magenta is 1.94 (floor 1.25). The shipped `--check` passes
for all three palettes.

## Caveats

- Hi and signal converge to nearly the same light blue in the simulations, as
  Netrunner's violet and blue did. Neither carries a distinct status meaning
  (headings versus live data), so the pairing is safe; do not give them
  separate statuses.
- Orange `warn` sits closer to red than Netrunner's yellow, trading warn/crit
  hue separation for the film's identity. Both presets support colour with
  labels; colour alone cannot encode severity reliably.
- These are opaque swatch checks, not checks of translucent panels against
  wallpaper. The live desktop must be judged in context.

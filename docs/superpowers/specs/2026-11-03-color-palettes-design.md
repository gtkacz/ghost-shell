# Preset color palettes

Add named desktop and lock-screen palettes, keeping the current appearance as
**Section Nine**, the default. **Netrunner** draws on the saved CP2077 mockup.
Terminal generation and installation are outside this change.

## Approach

Use `appearance.palette = "section-nine"` or `"netrunner"`. Resolve the palette
in the daemon before publishing configuration, then apply explicitly supplied
`appearance.colors` overrides. Existing consumers continue receiving complete
color tokens, including the lock-screen snapshot. Existing configurations keep
their explicit colors. Unknown palette names are rejected through the existing
configuration error path, preserving the last good configuration on reload.

This builds on the current shared config and live reload. Separate config files
would make switching cumbersome; resolving palettes independently in QML would
duplicate the rules between desktop and lock screen.

## Netrunner colors

Following live feedback and the palette research, Netrunner uses violet headings,
cyan data and plum-black surfaces. Yellow is reserved for warnings, with red for
critical alerts. Supporting lilac text and violet borders distinguish ordinary
operation from Section Nine without competing with warm alert colors.

| Token | Color | Role |
| --- | --- | --- |
| ground | `#0b0914` | Plum-black screen |
| panel | `#130e20` | Slightly lifted violet panel surface |
| hair | `#48345e` | Violet panel edges |
| hair2 | `#241a32` | Recessed dividers |
| text | `#d4cce5` | Lilac body text |
| hi | `#b58aff` | Violet titles and emphasized values |
| dim | `#9285a8` | Muted lilac captions |
| signal | `#38d9f5` | Cyan live data and active controls |
| warn | `#fcee0a` | Yellow reserved for warnings |
| crit | `#ff3b45` | Original red danger color |
| dot | `#38d9f5` | Cyan background grid |
| burn | `#e9ddff` | Pale lilac greeting flash |

No new decorative effects, motion changes, or widget-specific colors. Keep
Section Nine's existing values exactly. A single resolved palette keeps status
colors consistent throughout the desktop and lock screen.

## Validation

Cover default equivalence, both preset names, partial color override precedence,
unknown names, invalid overridden colors, and switching back to Section Nine.
Run existing Rust configuration tests and shell tests. Document selecting a
preset and removing explicit overrides to see all of its colors.

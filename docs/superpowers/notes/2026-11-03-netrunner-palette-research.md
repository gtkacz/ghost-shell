# Netrunner palette research

## Evidence

- Michael Rigley's commissioned 2017 screen-style development for CDPR:
  https://www.michaelrigley.com/work/cyberpunk-2077/
  Identifies its material as early concepts and style guides, not shipped UI.
  Inspected inventory study `mfr-cyberpunk-2077-0021.jpg`: violet/magenta
  structure, green/cyan active content, red meters, dark warm-black ground.
  This is direct evidence that CP2077's design exploration extends beyond yellow.
- Anna Kuźmińska's CP2077 UI-art portfolio:
  https://komisarzbob.artstation.com/projects/18mLB8
  Confirms authorship of game UI art; image presentation was not readable through
  the research tool, so no detailed color claims are derived from it.
- Adam Blumert's in-game UI/ads portfolio:
  https://adamblumert.pl/work/cp2077
  Describes UI, street ads, billboards, loading screens and their production.
  Supports distinguishing commercial world graphics from functional HUD design.
- Official Edgerunners character artwork:
  https://www.cyberpunk.net/en/edgerunners
  Inspected Lucy image: pale lilac/pink, icy blue, slate violet, near-black, and
  small red accents. These offer a quieter cool palette within the same IP.
- American Cinematographer's interviews and photography:
  https://theasc.com/article/uncanny-valley-blade-runner-2049/
  https://staging.ascmag.com/articles/blade-runner-2049-designing-the-future
  Film-specific lighting and color vary with location and story. Violet/pink
  holographic scenes provide a broader cyberpunk reference, not a universal
  franchise palette or published hex specification.

Exact proposed hex values below are our design adaptations, not official brand
tokens or measurements sampled from those images.

## Why the current palette resembles Section Nine

It retains the same value hierarchy: pale near-white `hi`, cool muted body,
cyan `signal`. Blue-black ground alone changes too little. Saturated headings
and a violet-tinted ground create a different identity without competing with
warm alert colors. Coloring every surface would instead erase the alert hierarchy.

## Candidate directions

| Direction | Ground | Primary/hi | Signal | Text | Dim |
| --- | --- | --- | --- | --- | --- |
| Violet / cyan — recommended | #0b0914 | #b58aff | #38d9f5 | #d4cce5 | #9285a8 |
| Electric blue / cyan | #080d19 | #6ea8ff | #38d9f5 | #c9d8ec | #8496b3 |
| Orchid / cyan | #120a16 | #f07bdf | #38d9f5 | #e2cfe2 | #a18aa4 |

Violet gives the strongest departure from teal-grey while staying on the cool
side of the palette. Electric blue is more technical and conservative. Orchid
has more nightlife character, but pink competes with red and loses some identity
in red-green color-vision simulations. Avoid neon green as the dominant hue:
it returns toward a green terminal aesthetic and weakens warm/cool separation
for some viewers. Avoid dark pure blue for small text: use lifted periwinkle.

## Warning choice

Keep `warn = #fcee0a`, `crit = #ff3b45` for the first violet/cyan trial. Yellow
is now absent from routine UI and remains the brightest token. Orange is not
needed to rescue yellow-primary decoration. If yellow still feels too bright,
trial `#ffb454` (amber-orange); it retains better small-text contrast than deeper
orange, but moves toward red and reduces warning/critical hue separation.

This means warning-level notifications, not every notification: informational
notifications retain neutral text. Keep existing labels/chips/threshold shapes;
color alone cannot encode severity reliably.

## Local checks

Reused the existing Machado full-severity simulation and contrast functions in
`scripts/term-theme.py` without changing terminal files. Against #0b0914:

| Token | Normal contrast | Protan simulation | Deutan simulation |
| --- | --- | --- | --- |
| Violet #b58aff | 7.56 | #66a0ff | #709ffc |
| Blue #6ea8ff | 8.19 | #84aeff | #71a1fd |
| Orchid #f07bdf | 8.09 | #799be2 | #9aacdc |
| Cyan #38d9f5 | 11.67 | #c4d1f7 | #aabff5 |
| Yellow #fcee0a | 16.34 | #ffe500 | #ffed2f |
| Amber #ffb454 | 11.20 | #ceb849 | #dfca56 |
| Red #ff3b45 | 5.61 | #776e43 | #a8983e |

Violet and blue converge in the simulations; do not assign them separate status
meanings. Cyan remains lighter. Existing red drops to 3.84 contrast under protan
simulation: preserve its established role and support it with labels; do not
claim full accessibility compliance. These are opaque swatch checks, not complete
checks of translucent panels against wallpaper. The live desktop must be judged
in context, particularly its persistent orange/yellow weather alerts.

No live settings or production tokens changed during this research.

# BlockGraph spike: one shader per graph for the telemetry lanes

Date: 2026-10-21. The first step of Plan 2b item 3 (ruling R15, deferral P17): measure a shader-drawn block graph on
six full-width lanes against the current `BlockGraph` before the lane widget is written.

## Setup

- Same machine as the rendering spike: Fedora 44, KDE Plasma 6.7 Wayland, Quickshell 0.2.1, Qt 6.11.2, M27Q
  2560x1440 at scale 1.15 (2226x1252 logical, so `Theme.u` = 1.739). The user's `ghostd` and `ghost-shell` services
  ran throughout and were not touched.
- `spikes/blockgraph-shader/`:
  - `ShaderBlockGraph.qml`: `BlockGraph`'s properties and look, drawn by one `ShaderEffect`. JavaScript still picks
    each column's sample and computes its lit count and level with `segments.mjs`, as `BlockGraph` does, and hands
    them to the shader as a two-row PPM image in a `data:` URL: one texel per column (lit count, level), and one per
    graph row (the ramp's alpha byte, from `Segments.cellColor`). The unlit, hi, warn and crit colours are uniforms
    taken from `segments.mjs` and `Theme`, so the shader restates no colour or alpha.
  - `blockgraph.frag`: finds the cell under each fragment with `BlockGraph`'s own layout arithmetic and colours it by
    `cellColor`'s rules.
  - `shell.qml`: a `PanelWindow` on the Bottom layer (namespace `ghost-spike`, empty input mask) across the bottom of
    the screen, holding six lanes 1256 x 70 design px with 6 px gaps. `SPIKE_MODE` picks `rect` (the kit's
    `BlockGraph`), `shader`, `none` (the same subscriptions, no lanes) or `compare`.
  - `run.sh`: the whole measurement, on a private bus (`scripts/private-bus.conf`) with its own fake daemon.
  - `kit`: a symlink to `shell/kit`. Quickshell refuses relative imports that leave the config directory (`Script
    qrc:/qs-blackhole unavailable`), and the spike needs `segments.mjs` and `scale.mjs`.
- Lane geometry. 70 px is MONITOR's lane height less the mockup's 8 px graph padding above and below. The main runs
  use the mockup's lane cell, 4 x 3 with a 1 px gap: 251 columns x 17 rows, 4 267 cells per lane, 25 602 in all.
  One run per kind uses the kit's default cell, 3 x 2: 314 x 23, 7 222 per lane, 43 332 in all.
- Data. Every lane is fed by `Data.history`, from `ghostd run --fake` at its normal 1 s tick. The daemon records four
  series (`load`, `ccd0`, `ccd1`, `gpu_load`), so six lanes reuse them and all six scroll on every tick. A `socat`
  client keeps `cpu` and `gpu` subscribed for the whole session, and `run.sh` waits 320 s before the first run, so
  every run starts with the full 300-sample history. A 3 x 2 lane is wider than that, and its 14 oldest columns read
  as missing, as MONITOR's would.

## Method

- `spikes/blockgraph-shader/run.sh` with no variables set. Per run, it:
  - starts `qs` with `QSG_RENDER_TIMING=1`;
  - creates the lanes once, when the history is full and `Theme.u` is final, and logs how long that took. The lanes
    `Loader` is synchronous, so this is wall time on the GUI thread;
  - records RSS at creation, waits 10 s, then runs `scripts/measure-cpu.sh <qs pid> 60`;
  - counts `syncAndRender: frame rendered` lines before and after that window, and stops `qs` by its pid.
- Runs went none, rect, shader, three times over, then rect and shader once each at 3 x 2.
- `compare` mode draws both kinds from one fixed series (every level, nulls, empty and full columns) and from the live
  `load` history. It grabs both twice, 2.5 s apart, at the screen's 1.15 scale, and `magick compare` reports how
  many pixels differ and the largest difference in any channel.
- CPU covers only the `qs` process: GPU time and KWin's compositing are not included. The surface is a band about
  800 logical px high rather than the whole screen, to keep it small on the user's live desktop. The `none` runs
  show what that surface and the subscriptions cost by themselves.

## Results

Main lane cell (4 x 3), three runs each. RSS is in MiB, and "+" is the increase over `none`.

| Mode | CPU % of one core | RSS (MiB) | Frames/s | Creation (ms) | First frame after creation (ms) |
|---|---|---|---|---|---|
| `none` | 0.03 / 0.05 / 0.07 | 156.8 | 0.00 | — | — |
| `rect` (BlockGraph) | 4.25 / 3.93 / 4.03 | 347.0 (+190) | 1.00 / 0.95 / 1.00 | 192 / 192 / 177 | 19 / 17 / 17 (sync 12–13) |
| `shader` | 0.97 / 0.93 / 1.13 | 214.2 (+57) | 1.55 / 1.42 / 1.60 | 8 / 12 / 8 | 1 / 1 / 1 |

Kit default cell (3 x 2), one run each:

| Mode | CPU % | RSS (MiB) | Frames/s | Creation (ms) | First frame (ms) |
|---|---|---|---|---|---|
| `rect` | 8.43 | 463.6 (+307) | 1.00 | 304 | 31 (sync 22) |
| `shader` | 1.10 | 223.1 (+66) | 2.00 | 11 | 1 |

Pixel comparison (grabs of 2512 x 140 physical px):

| Series | 4 x 3 | 3 x 2 |
|---|---|---|
| fixed | 1 536 pixels in one ramp row differ by at most 1.2/255 in one channel | identical |
| live, first grab | identical | identical |
| live, second grab | identical | identical |
| shader, first vs second grab | differ (6 047 / 4 122 px): the image follows new data | |

Over the baseline, six lanes cost 4.0 % of a core as Rectangles and 0.96 % as shaders at 4 x 3, and 8.4 % against
1.05 % at 3 x 2. The shader's cost hardly depends on the cell count, while the Rectangles' grows with it. Creation
is 20–28 times faster, and the frame that builds the scene graph goes from 17–31 ms to 1 ms.

The first full run, before `retainWhileLoading` was added, measured 0.95 / 0.92 / 0.93 % at exactly 2.00 frames/s.
The extra frames are cheap enough to vanish in the noise (see below).

## Observations

- **The shader version renders a second, empty frame after 60 % of its updates at 4 x 3, and after all of them at
  3 x 2.** A `data:` URL decodes on a
  later event-loop turn, even with `asynchronous: false`. Logging the `Image` status showed the pattern: the history
  changes, all six images go to Loading, a frame renders, the images go to Ready and upload, and a second frame
  renders 1–2 ms later. `retainWhileLoading: true` cut the extra frame from 1.0 to about 0.6 per update at 4 x 3, but
  not at 3 x 2. The extra frame has sync = 0 and render = 0 ms, and by the rendering spike's marginal figure of about
  0.035 % per frame/s it costs a few hundredths of a percent. It still breaks the rendering spike's rule that nothing
  should render frames of its own. Ways to remove it, none tried here:
  - pack the columns into named `vec4` uniforms. ShaderEffect ignores array uniforms (tested: a `vec4 packed[4]`
    read as zeros), so the shader would select among, for example, 32 members by index. With 8 bits per column and
    3 columns per float, that holds 384 columns;
  - build the data texture from a `layer`ed row of one-pixel Rectangles, which renders in the same frame. That is
    about 270 Rectangles per lane (one per column and one per row) instead of 4 267;
  - a C++ texture provider, which pure QML can't offer.
- **Gotchas found on the way**, each needed for a correct picture:
  - an `int` QML property reached a `float` uniform as 0, so the shader saw no columns and no rows. The counts are
    `real` properties in the spike;
  - ShaderEffect passes `color` properties premultiplied. Premultiplying again made the 6 % unlit cells invisible;
  - `qsb --qt6` also bakes GLSL ES 100, which rejects `texelFetch` and `round`. The shader samples texel centres with
    nearest filtering (`smooth: false`) and rounds with `floor(x + 0.5)`;
  - the first cut interpolated the ramp's alpha between its two ends and came out a byte off on some rows. Sending
    each row's alpha byte from `segments.mjs` made the grabs identical, apart from the one 4 x 3 row in the table,
    which is premultiply rounding at alpha 157;
  - `grabToImage` without a size cut a 140.0-px-high grab to 139 px. The uneven scale put cell edges on pixel
    centres, and one pixel row in a thousand differed. Grabs now pass a size that makes the scale exactly 1.15.
- **RSS.** Six shader lanes add 57–66 MiB over the baseline, against 190–307 MiB for Rectangles. Where the 57 MiB goes
  (the shader pipelines, the texture uploads, the driver) was not broken down.
- **Frame counts** are read from `qs`'s log, which reaches the file late. Runs whose frames were all 1 s apart
  counted 56–57 in 60 s. The table's frames/s are within a few frames of the truth. The 1–2 ms follow-up frames were
  counted from each run's whole log: 43 of 113, 42 of 105 and 45 of 115 at 4 x 3, and 75 of 145 at 3 x 2.
- **CPU without the surface.** These figures cover the graphs alone. A real MONITOR lane adds a label block and the
  panel frame, which don't change at 1 Hz.

## Decision

1. **Build Plan 2b's telemetry lanes on a shader graph, not on `BlockGraph`.** Six lanes cost about 1 % of a core
   instead of 4–8 %, 57 MiB instead of 190–307 MiB, and about 10 ms to create instead of 180–300 ms. The pixels match
   `BlockGraph` (identical on live data in both cell sizes).
2. **Remove the follow-up frame before shipping.** The fixed-uniform packing is the smallest change that keeps
   everything in QML and needs no `Image`. Measure it with this script: 1.00 frames/s at 1 Hz is the pass mark.
3. **Keep `segments.mjs` the single source of the look.** The spike's shader takes every colour and alpha from it.
   The kit version should export the ramp's alpha per row, rather than derive it from `cellColor("ok", row, row + 2,
   rows)` as the spike does.
4. **Consider replacing `BlockGraph` itself** with the shader version, once it renders one frame per update. C14
   (a half-width graphics slot costs about 0.8 s and 300 MiB to create) comes from the same per-cell Rectangles. That
   decision belongs to the plan, not to this spike.

## Reproduce

```sh
spikes/blockgraph-shader/run.sh                  # about 22 minutes; prints and keeps results.txt in a temp dir
SPIKE_STAGES=compare spikes/blockgraph-shader/run.sh  # pixel comparison only (still waits for the history)
```

`SPIKE_TICK_MS=100 SPIKE_WARMUP_S=35 SPIKE_MEASURE_S=5 SPIKE_REPEATS=1` checks the script in about 3 minutes. At
that tick its numbers are not comparable. `run.sh` compiles `blockgraph.frag.qsb` (git-ignored) and uses
`target/release/ghostd`, so build that first.

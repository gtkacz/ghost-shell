# Rendering spike: layer, frame pacing, cost

Date: 2026-10-20. Task 1 of plan 1. Task 14 follows the **Decision** section.

## Setup

- Fedora 44, KDE Plasma 6.7 Wayland (KWin), Quickshell 0.2.1, Qt 6.11.2, one 2560x1440 monitor (M27Q, DP-1) at 170 Hz and display scale 1.15. `plasmashell` and the `gridlock` HUD were running throughout.
- `spikes/pacing/shell.qml`: a full-screen, transparent `PanelWindow` on `WlrLayer.Bottom`, namespace `desktop`, empty input mask. It draws the dot grid (1 px dots, 24 spacing, 16 % alpha) and, depending on `SPIKE_MODE`:
  - `static`: nothing else;
  - `data`: 600 `Rectangle`s recoloured by a 1000 ms `Timer`;
  - `anim`: `data` plus a square stepped 1 px by a 66 ms `Timer`.
- Build the shader with `qsb-qt6 --qt6 -o spikes/pacing/dots.frag.qsb spikes/pacing/dots.frag` (the `.qsb` is git-ignored), then run `SPIKE_MODE=<mode> qs -p spikes/pacing`.
- The namespace is `desktop` because gridlock's source says KWin needs it to keep the surface under Show Desktop. Reusing it leaves the layer as the only difference between the spike and gridlock.
- Confirmed with `WAYLAND_DEBUG=1`: `get_layer_surface(..., layer 1, "desktop")`, all four anchors, exclusive zone -1, keyboard interactivity none, configured at 2226x1252 logical.
- Confirmed visually with `grabToImage` on the window's own content (a PNG in `/tmp`): the dot grid at the expected 27.6 physical px pitch, the 30x20 block grid and the moving square all render. No spectacle screenshots were taken.

## Method

- CPU and RSS: `scripts/measure-cpu.sh <qs pid> 60`, started 5 s after launch. The pid is the spike's own `qs`; gridlock was not touched.
- Frames: Qt 6.11 prints one `qt.scenegraph.time.renderloop ... syncAndRender: frame rendered in Nms, ...` line per rendered frame under `QSG_RENDER_TIMING=1`. Frames were counted with `grep -ac 'syncAndRender: frame rendered'` on the log at the start and the end of the 60 s window, and the difference was divided by 60.
- Two deviations from the brief's command, both needed to get any data:
  - Quickshell writes its log to **stdout**, so `2> log` captures nothing. The runs used `> log 2>&1`.
  - The brief's pattern `Frame rendered` (capital F) matches nothing in Qt 6.
- Each run used `$!` for the pid and `kill $pid` to stop it, because `pgrep -f` / `pkill -f 'qs -p spikes/pacing'` also match the calling shell's own command line.

## Results

| Mode | CPU % of one core | RSS (KiB) | Frames in 60 s | Frames/s |
|---|---|---|---|---|
| `static` | 0.00 | 158512 | 0 (1 at mapping) | 0.00 |
| `data` | 0.15 | 165356 | 60 | 1.00 |
| `anim` | 0.68 | 165572 | 969 | 16.15 |

Supplementary runs, from throwaway copies of `shell.qml` that are not committed. Each differs from `anim` in one respect:

| Variant | CPU % | RSS (KiB) | Frames in 60 s | Frames/s |
|---|---|---|---|---|
| One 67 ms `Timer` drives the square and, every 15th tick, the recolour | 0.65 | 165656 | 896 | 14.93 |
| A looping `NumberAnimation` drives the square instead of the 66 ms `Timer` | 2.07 | 166404 | 3810 | 63.50 |

Reference, measured at the same time as the three main runs: the running `qs -c gridlock` used 3.83 / 4.23 / 3.98 % CPU and about 562-569 MB RSS. It is a different workload and is only here for scale.

Observations:

- Frame time is at most 2 ms in every measured run, and 0 ms for all but a handful of frames (the log has 1 ms resolution). A 20 ms first frame showed up once, on the first ever launch, probably shader pipeline creation. CPU-side frame cost is sub-millisecond. CPU % covers only the `qs` process, so GPU time and `kwin_wayland`'s compositing cost are not included.
- `anim` renders 16.15 frames/s because two independent timers each produce their own frames: 1000 / 66 = 15.15 Hz for the square plus 1.00 Hz for the recolour. Frame spacing is mostly 65-66 ms, with one short gap per second where the recolour lands between two steps.
- Folding both onto one clock removes the extra frames: 14.93 frames/s (1000 / 67), same CPU.
- Qt's own animation driver does not follow the 170 Hz display here. The `NumberAnimation` frames are spaced 15-16 ms apart, which is about 62.5 Hz, plus the 1 Hz recolour makes 63.5. It still costs three times the CPU of the 15 Hz clock for a trivial scene.
- Marginal cost is about 0.035 % of a core per frame/s ((0.68 - 0.15) / 15.15) for a full-screen scene with trivial content. The real dashboard will cost more per frame.
- `devicePixelRatio` reads 2 inside `Component.onCompleted` and 1.15 once the surface is configured. Bind to it, never read it once.

## Acceptance

| Criterion | Result |
|---|---|
| `static` renders about 0 frames/s | Pass: 0.00 frames/s, 0.00 % CPU |
| `anim` renders at most 15 frames/s | Fail by 1.15 frames/s as written (16.15), entirely from the two independent timers. Pass with one clock (14.93) |
| `anim` costs at most 1.5 % CPU | Pass: 0.68 % (0.65 % with one clock) |

Mitigation recorded: one clock at 15 Hz with every change applied on its tick. A lower rate or separate small surfaces for animated regions are not needed for this load. They stay as the fallback if Task 14's re-measurement with real panels exceeds the spec's 2 % budget.

## To verify with the user

These need the user's windows or Show Desktop, so they were not attempted (ruling R6). What can be said without them:

| Check | Inference | How to confirm |
|---|---|---|
| Above Plasma's desktop | Likely. gridlock-keeper's source says a Background-layer surface and plasmashell's desktop window share KWin's desktop layer and fight over stacking, which is why that script exists. Bottom is a different, higher layer in wlr-layer-shell order (background, bottom, windows, top, overlay) | Run the spike in `anim` mode, click an empty part of the desktop, and check that the block grid at the bottom right stays visible |
| Below windows | Expected from layer order | Drag any window over the block grid; it should hide behind the window |
| Visible under Meta+D | Unknown. It rests on the `desktop` namespace, which makes KWin treat the surface as a desktop window | Press Meta+D with a window open; the block grid should stay |
| Covered surfaces stop rendering | Not measured. Nothing in the spike's design makes it go quiet: the frame source is a `Timer`, not a compositor callback | Start `SPIKE_MODE=anim QSG_RENDER_TIMING=1 qs -p spikes/pacing > /tmp/spike-covered.log 2>&1 &`, maximise a window over the whole screen, run `scripts/measure-cpu.sh $! 60`, then `grep -ac 'syncAndRender: frame rendered' /tmp/spike-covered.log` before and after. About 0 frames/s means KWin throttles covered surfaces |

The occlusion check is also the one that decides whether Plan 2 needs its own detection.

## Decision

1. **Layer: `Bottom`, namespace `desktop`.** It avoids the shared-layer stacking fight that gridlock needs a KWin script to work around. This is provisional on the Meta+D and above-Plasma checks. If either fails, switch to `Background` and port gridlock-keeper's logic. Task 14 should keep the layer in one place so that is a one-line change.
2. **FrameClock: one `Timer` at 15 Hz, not tied to the display.** Use `interval: 67` (14.93 frames/s). 66 ms gives 15.15 and breaks the 15 frames/s ceiling. Do not drive anything with `NumberAnimation`, `Animator` or another vsync-style driver: they ran at about 62.5 frames/s and cost 2.07 %, over the 1.5 % acceptance threshold on a trivial scene.
3. **Nothing that changes pixels gets its own timer.** An independent timer adds its own frames (measured: +1.00 frames/s for a 1 Hz one). Data updates are applied on a clock tick. Task 14's 250 ms blink timer would add 4 frames/s, so derive the blink step from the tick count instead, for example `Math.floor(frame * 4 / 15) % 4`. The 30 fps spectrum needs the clock at interval 33, only while it is visible and playing; 15 extra frames/s is about 0.5 % more by the marginal figure above.
4. **Occluded surfaces: assume they keep rendering.** This is unverified. Plan 2 needs occlusion detection, and Task 14's FrameClock must be stoppable as a whole. The `static` run shows the mechanism works: with every timer stopped the surface renders 0 frames/s at 0.00 % CPU. If the user's check shows KWin already throttles covered surfaces, Plan 2 can drop the requirement.
5. **Re-measure in Task 14 with the real panels.** This spike draws trivial content, so 0.68 % is a floor, not a forecast.

## Results (2026-10-21)

The user ran the spike in `anim` mode on the live desktop, with the old `gridlock` HUD running.

| Check | Result |
|---|---|
| Above Plasma's desktop | Not observable while gridlock runs: the spike drew *under* gridlock. gridlock's source says KWin derives the window type from the namespace, so a surface named `desktop` sits in KWin's desktop layer whatever its wlr layer (Bottom or Background). `gridlock-keeper` raises every desktop window with resource class `quickshell` (gridlock and ghost-shell alike) above plasmashell's. Staying above Plasma's desktop therefore depends on a keeper script, and ghost-shell needs its own before gridlock is retired |
| Below windows | Pass (user) |
| Visible under Meta+D | Pass (user) |
| Covered surfaces stop rendering | Not measured. With gridlock covering most of the spike (but partly translucent) it still rendered 16.1 frames/s. The user said to assume covered surfaces keep rendering, so Plan 2 keeps occlusion detection |

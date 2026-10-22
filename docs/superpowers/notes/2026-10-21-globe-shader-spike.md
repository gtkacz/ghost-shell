# Globe spike: the uplink globe as one shader

Date: 2026-10-21. Ruling R49 drops Plan 2b's C++ scene-graph module, so the uplink globe becomes a shader like the
kit's `BlockGraph`. This spike measures that globe before Plan 2b's Task 11 is written against it, as
`2026-10-21-blockgraph-shader-spike.md` did for `BlockGraph`. It ran on branch `spike/globe-shader`, which is not
merged.

## Setup

- Same machine and harness as the README's "Plan 2a: block graphs drawn by one shader":
  - Fedora 44, Quickshell 0.2.1, Qt 6.11.2, RTX 4080 SUPER on driver 615.78.08;
  - every `qs` rendered into a private headless KWin (`headless-kwin.sh`, 2560×1440) with `QT_SCALE_FACTOR=1.15`, so
    `Theme.u` = 1.739 as on the M27Q;
  - a fake release `ghostd` on a private bus, at `/tmp/ghost-smoke-globe.sock`;
  - `scripts/measure-cpu.sh <qs pid> 60` after a 15 s settle.

  The user's `ghostd` and `ghost-shell` services ran throughout and were not touched.
- **Renderer, checked rather than assumed.** In Qt 6.11, `QSG_INFO=1` prints the QRhi setup but not the GL strings, so
  the runs also set `QT_LOGGING_RULES=qt.rhi.general=true`, which logs:

  ```
  qt.rhi.general: OpenGL VENDOR: NVIDIA Corporation RENDERER: NVIDIA GeForce RTX 4080 SUPER/PCIe/SSE2 VERSION: OpenGL ES 3.2 NVIDIA 615.78.08
  ```

  Every measured `qs` pid also appeared in `nvidia-smi` as a `G` process (VRAM in the tables). The KWin is virtual,
  but the client renders on the GPU through its own EGL.
- The runs used the installed service's environment: EGL pointed at NVIDIA alone (the `nvidia.conf` drop-in's two
  variables) and the 512×512 atlas. The driver's shader cache and Qt's caches went to the run's own directory, never
  to the user's.
- `spikes/globe-shader/`:
  - `globe.frag`: the mockup's `drawGlobe()`, less the labels. It ray-casts the sphere, finds the dots near the pixel
    with an inverse spherical-Fibonacci lookup (below), and draws the outline circle. Built with `-DGLOBE_ARCS`, it
    also draws the arcs, packets, end squares and the home marker.
  - `GlobeSpike.qml`: one `ShaderEffect`. Its `overlay` property selects what draws the arcs:
    - `shader`: the `GLOBE_ARCS` build;
    - `shape`: a `Shape` (one `PathMultiline` of every arc's visible runs, 8 arcs × 40 segments) plus `Rectangle`s
      for the squares, all recomputed in JavaScript (`globe.mjs`) on every tick;
    - `none`: the dots only.
  - `shell.qml`: the globe at a panel body's size, created 2 s after the window's first frame. Its clock is
    `FrameClock`'s 67 ms tick. Sizes, in design px:
    - **V2 centre:** 608 × 330, the centre tabs' 632 × 372 panel less its header and padding;
    - **V1 scope:** 276 × 146, the scope column's tabs panel, taking about 100 for Task 8's scope height. This size is
      "small", so the margin is 10 and the arcs are 0.7 wide.

    `empty` holds the clock and draws nothing.
  - `land-mask.png`: the **placeholder** mask, 2048 × 1024, 1-bit, equirectangular. `land-mask.py` writes it offline
    from the mockup's own coarse `LAND` table, smoothed by the mockup's `landAt()` and thresholded at 0.5. The real
    Natural Earth mask is Task 11's job. Because this is the mockup's own land, a dot here is land exactly when the
    mockup draws it as land, apart from dots within a texel of a coast.
  - `reference.py`: draws the dots by brute force (every point projected, every square rasterised) to check the
    shader's lookup.
  - `run.sh`: the measurement. It compiles the shaders, makes the kit's shaders (the lanes need them), starts the
    daemon and a client that keeps `cpu` and `gpu` subscribed, and waits 320 s so the lanes start with a full
    history. It then grabs every mode and runs the measured rounds.
- Home is São Paulo, and the endpoints are the mockup's six peers plus Tokyo and Sydney, for 8 arcs.

## The lookup

The dots are the mockup's lattice: `y = 1 − 2i/(N − 1)`, longitude `i` golden angles, N = 3200. The lookup follows
Keinert et al. 2015 ("Spherical Fibonacci Mapping") and does no loop over the 3200 points:

1. Take the point under the pixel.
2. Pick a Fibonacci basis from its latitude, and solve a 2×2 system for the lattice cell around it.
3. Test that cell's four corner points, drawing each one's square with box-filter coverage.

Three additions were needed:

- **Adapting the lattice.** The paper's lattice has `z = 1 − (2i + 1)/n`. This one has points on both poles and steps
  of `2/(N − 1)`, so the offset and step change. The paper's longitude also runs the other way.
- **The poles.** In float64, the four corners missed the true nearest point in 2 of 20 000 random directions, both
  within a dot of a pole, where the lattice's pole points sit. The nearer pole is now a fifth candidate.
- **The limb.** Near the edge the sphere is foreshortened. A dot two lattice cells away radially can still cover the
  pixel, and so can a dot further out than the pixel's own point. Within 4 dots of the limb, the lookup therefore
  repeats a quarter dot and a half dot inwards and outwards. That makes 5 lookups and up to 25 candidates, with
  repeats removed.

Against `reference.py`, at the frozen 4000 ms frame and the grabs' 1216 × 660 and 552 × 292 physical px, the dots
differ as follows:

| Version | V2 pixels differing > 2/255 | V1 | Largest difference |
|---|---|---|---|
| Four corners only | 648 | 597 | 58/255 (V2), 76/255 (V1): rim dots cut off |
| Plus pole and limb lookups (the spike's shader) | 4 | 25 | 11/255 (V2), 10/255 (V1) |

Every remaining difference is at r/R ≥ 0.985. The `shape` and `shader` overlays draw the same geometry, and they differ
only in antialiasing along the arcs: 4 357 px of the V2 grab differ by more than 2/255, by at most 113/255 at one end
of a run. The snapshots look the same.

## Results

Two passes of interleaved rounds. Each round ran lanes, empty, then none, shape and shader at V2 and at V1. That is 5
rounds of the globe, plus one `Shape.GeometryRenderer` run per size per pass. The second pass started with the first
pass's shader caches warm.

**`lanes`** is the README's baseline in the same harness: six full-width shader `BlockGraph` lanes, from
`spikes/blockgraph-shader` in `rect` mode, on a 1 Hz history. The first two lanes runs of pass 1 are left out. The
worktree's kit shaders weren't compiled yet, so they drew nothing. The other three read 0.20 % at 1.00 frames/s, which
matches the README's 0.17 %. The table's lanes RSS is pass 2's, with the BlockGraph shader cached. Pass 1's valid run,
which compiled it, held 186 MiB.

Shell `qs` process only. CPU is % of one core over 60 s. RSS is `smaps_rollup` at the end of the window, VRAM is the
pid's line in `nvidia-smi`, and SM is the pid's `nvidia-smi pmon` utilisation averaged over 55 one-second samples.
Load average was 0.1–2.4.

| Mode | CPU % (each run) | RSS (MiB) | VRAM (MiB) | GPU SM % | Frames/s |
|---|---|---|---|---|---|
| `lanes` (BlockGraph baseline) | 0.20 / 0.20 / 0.20 | 127 | 54 | 0.0 | 1.00 |
| `empty` (clock held, no globe) | 0.12 / 0.08 / 0.07 / 0.08 / 0.08 | 122–123 | 23 | 0.0 | 0 |
| V2 `none` (dots only) | 0.77 / 0.55 / 0.55 / 0.58 / 0.57 | 155–156 | 32 | 0.1–0.7 | 14.93 |
| V2 `shader` | 0.77 / 0.57 / 0.55 / 0.58 / 0.53 | 155–156 | 36 | 1.5–2.4 | 14.93 |
| V2 `shape` (CurveRenderer) | 1.90 / 1.80 / 1.77 / 1.72 / 1.82 | 160 | 34 | 0.4–1.4 | **29.87** |
| V2 `shape` (GeometryRenderer) | 1.55 / 1.62 | 158 | 34 | 0.5–0.6 | **29.87** |
| V1 `none` | 0.65 / 0.57 / 0.58 / 0.53 / 0.50 | 155–156 | 17 | 0.2–1.1 | 14.93 |
| V1 `shader` | 0.55 / 0.57 / 0.57 / 0.57 / 0.57 | 155–156 | 21 | 0.3–0.9 | 14.93 |
| V1 `shape` (CurveRenderer) | 1.87 / 1.78 / 1.78 / 1.77 / 1.77 | 160 | 19 | 0.1–0.9 | **29.85–29.87** |
| V1 `shape` (GeometryRenderer) | 1.48 / 1.53 | 157–158 | 19 | 0.4–0.7 | **29.87** |

Creation and the first frame, with warm caches:

- creating the globe (a synchronous `Loader`, which includes decoding the 2048 × 1024 PNG) took 23–28 ms in every
  mode, against 2–3 ms for the six lanes;
- the first frame after creation took 6–7 ms, with `render` at 0–1 ms;
- with the shaders not yet in the driver's cache (the grab stage, which ran first), the first frame took 134 ms for the
  dots shader, 97 ms for the `GLOBE_ARCS` build and 44 ms for the Shape's curve shaders. A cold compile also kept
  about 60–70 MiB of RSS: 216 MiB in the first GeometryRenderer run, and 231 against 159 MiB in a manual run.

Reading the table:

- **The shader globe costs what 15 frames/s costs.** At V2, dots alone and dots with every arc both read about
  0.57 %, about 0.5 % over `empty`. That is the rendering spike's 0.035 % per frame/s × 14.93. Moving the arcs into
  the shader costs no CPU, because nothing but `elapsedMs` changes per tick: the endpoint uniforms are set once.
- **The Shape version renders two frames per tick and costs about 3 times as much** (1.8 % at both sizes, 1.5–1.6 %
  with the GeometryRenderer). The second frame comes 1–13 ms after the first, while the tick's own frame is on time
  (`perWindowFrameDelta` alternating 53–66 and 1–13). A probe with nothing but one `PathPolyline` changing every 67 ms
  shows the same pair with both renderers, and a static Shape with a moving Rectangle shows single frames. The
  follow-up frame therefore comes from Qt 6.11's `Shape` whenever its path changes. It breaks the ≤ 15 frames/s
  ceiling by itself.
- **GPU.** On SM utilisation, the arcs in the shader cost about 1–2 % of the 4080 at V2 against 0.1–0.7 % for the dots
  alone. At V1 the two are within noise. `pmon`'s whole-percent samples are coarse, and the GPU idles at low clocks, so
  treat these figures as an order of magnitude. Per pixel the arcs walk 8 × 41 points, with a bounding-circle reject
  per arc, so the cost grows with the globe's area and the arc count.
- **RSS: the texture, not the shader.** The globe is about 33 MiB over `empty`, and the lanes about 5 MiB. A manual
  decomposition (V2 `none`, warm caches, 12 s after start, one run each) shows where it goes:

  | Mask | RSS (MiB) | Over `empty` | Anonymous | `/dev/nvidiactl` | VRAM (MiB) |
  |---|---|---|---|---|---|
  | `empty`, for reference | 121–122 | — | 13 | 3 | 23 |
  | no mask (missing file) | 125 | +3 | 14 | 4 | 26 |
  | 512 × 256 | 137 | +15 | 15 | 4 | 26 |
  | 1024 × 512 | 142–143 | +21 | 18 | 6 | 26 |
  | 2048 × 1024 | 154–155 | +33 | 22 | 14 | 32 |

  Loading any image costs about 10 MiB, and the rest grows with the mask. The decoded image is stored as 32-bit, so
  the 2048 × 1024 mask is 8 MiB of RAM, plus the driver's chunks. The live shell holds about 245 MiB against the
  300 MB budget. A 2048 × 1024 globe would take it to about 278 MiB, and a 512 × 256 one to about 260.

## Snapshots

`spikes/globe-shader/snapshots/`, grabbed at the frozen 4000 ms frame and flattened onto `Theme.ground`:

- `globe-v2-shader.png` and `globe-v2-shape.png`: V2 centre, 1216 × 660 px;
- `globe-v1-shader.png` and `globe-v1-shape.png`: V1 scope, 552 × 292 px.

South America, North America and western Europe are in view, with São Paulo on Brazil's south-east coast. Two of the
long arcs run past the disc at its top and bottom, as the mockup's would: a point stays visible while it is in front or
outside the disc.

## Kit gotchas

- **Textures work where arrays don't.** A hidden `Image` assigned to a `var` property (`property var landMask:
  maskImage`) reaches `layout(binding = 1) uniform sampler2D landMask`. A local file loads synchronously, so there is
  none of the `data:` URL's extra frame. `smooth: false` gives nearest sampling, so a dot is land exactly when its texel
  is. A texture can't stand in for per-frame data, though, since every new frame would need a new image.
- **Precision.**
  - `qsb --qt6` bakes GLSL ES 100 with `precision highp float` (mediump is opt-in), and NVIDIA's ES 3.2 context runs
    that variant, so the shader computes in 32-bit floats.
  - `fract(i · 0.618…)` for `i` up to 3199 loses about 5·10⁻⁴ rad there, a third of a pixel at V2 size. The shader
    splits the index as 64a + b, with `fract(64 · 0.618…)` as a constant.
  - A device that really runs mediump would need that split and more.
- **GLSL ES 100.**
  - `half` is a reserved word, and the shader fails to compile if a variable is called that.
  - Loops need constant bounds. A uniform-dependent count is a `break` inside a constant loop.
  - A local `float seen[25]` indexed by an expression of the loop counters compiles.
  - `texture()` inside branches and loops is safe here only because the mask has no mipmaps.
- **`ThemeLib.withAlpha` takes a percentage.** Passing 0.5 gives 0.2 % alpha. The Shape's arcs were invisible until
  that was fixed.
- **The driver's shader cache needs its directory to exist.** Otherwise every start compiles, which costs the first
  frame 90–160 ms and keeps 60–70 MiB. The service's `CacheDirectory=` covers this for the installed shell, and any
  harness must `mkdir` it.
- **`Shape` renders a second frame after every path change** (above). A Shape is fine for static frames, as `Panel`
  uses it, but not for geometry that changes every tick.
- **`qs` buffers its log.** A run rendering 1 frame/s can end its 60 s window with none of that window's lines written.
  `run.sh` also counts every frame since creation from the whole log after `qs` stops.

## Decision

1. **Write Task 11 as a kit `Globe.qml` over one `ShaderEffect` (`shaders/globe.frag`), with the arcs, packets, end
   squares and home pulse in the shader.**
   - At 15 frames/s it costs about 0.5 % of a core, all of it the frame cost, against 1.8 % and doubled frames for a
     Shape overlay.
   - Its inputs are `elapsedMs`, `home` and 8 named `vec4` endpoints, set only when `uplink` changes, which caps the
     arcs at 8.
   - The inverse lookup with the pole and limb additions matches a brute-force draw within 11/255 on a few rim pixels.
   - Port `globe.mjs`'s maths (`unitVector`, `view`, `slerp`, `arcAltitude`) as the tested JavaScript half. The plan's
     C++ tests become node tests of it.
2. **Labels need positions every tick.** Task 12's labels are `Text` items, so the widget must project home and the
   endpoints in JavaScript on every tick, with the same `rot`, `tilt` and `R` as the shader. That replaces the native
   `labelAnchors`. It is 9 projections a tick, which this spike did not measure.
3. **Ship a smaller mask, or accept 33 MiB.** The dots are 3.6° apart, so a 1024 × 512 mask (0.35° a texel) or
   512 × 256 (0.7°) loses nothing visible, and costs 21 or 15 MiB instead of 33.
   - I'd pick 1024 × 512: it still settles the São Paulo coast test, where the sea point is 2.9° away.
   - Task 11 can keep rasterising Natural Earth at 2048 × 1024 for its land tests and commit the downsampled PNG the
     widget loads.
   - Packing 3200 land bits into about 34 uniform words would cost about 3 MiB. But it ties the bits to N, which
     the manifest's `dots` setting makes configurable.
4. **Not measured here:** how much the globe adds to V1's animating state, where `FrameClock` already ticks for other
   motion and the globe's frames would coincide with existing ones; the labels; and the GPU cost on an iGPU.

## Reproduce

```sh
eval "$(.superpowers/sdd/2026-10-21-ghost-shell-plan-2-dashboard/headless-kwin.sh globe)"  # prints WAYLAND_DISPLAY and pid
export WAYLAND_DISPLAY
spikes/globe-shader/run.sh                      # about 45 minutes; results.txt, logs and grabs in a temp dir
kill "$pid" && rm -r -- /tmp/gs-sbx-globe
```

Before running it, build the daemon with `cargo build --release -p ghostd`. `SPIKE_STAGES=grab` takes only the grabs.
`SPIKE_CACHE_DIR=<an earlier run's directory>` starts with every shader compiled. `reference.py` and `land-mask.py`
document their own usage.

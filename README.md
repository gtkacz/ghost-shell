# ghost-shell

A Tron: Legacy / Cyberpunk 2077 / Ghost in the Shell desktop shell for KDE Plasma (Wayland): a Quickshell UI fed by a Rust daemon, `ghostd`.

## Build

Requires a recent stable Rust toolchain, Quickshell (`qs`, Qt 6) and `qsb-qt6` for the shaders.

```sh
make release
```

This runs `cargo build --release` and `make shaders`. The daemon is `target/release/ghostd`; the shell's shaders are compiled next to their sources as `<name>.frag.qsb`.

## Test

Requires Node for the shell's logic tests.

```sh
make test
```

This runs `node --test 'shell/tests/*.test.mjs'` and `cargo test --workspace`.

The QML smoke tests start a debug `ghostd --fake` on a private socket, load the shell against it, restart the daemon once and fail on any QML error in the shell's log:

```sh
make smoke          # the default configuration
make smoke-layout   # scripts/layout-test.toml: a broken widget, a missing one and a tabs node
```

Two environment variables adjust a smoke run:

- `GHOST_CONFIG=<path>` runs it against that config file instead of a temporary copy of the defaults (this is how `make smoke-layout` works).
- `GHOST_SMOKE_ALLOW=<extended regex>` accepts extra shell-log lines, for example a widget the layout names before it exists. A pattern that does not compile, or that matches an empty line, fails the run instead of hiding every error.

The smoke tests run their daemon without a session bus, so they never raise desktop notifications.

## Install

```sh
make install
```

This runs `make release` and then `scripts/install.sh`. The script is idempotent and installs for the current user only:

- `ghostd` into `~/.local/bin/`;
- `shell/` (with its compiled shaders) and `widgets/` (without `broken-example`) into `~/.local/share/ghost-shell/`;
- the Kode Mono font into `~/.local/share/fonts/ghost-shell/`, if it is not installed yet;
- the user units `ghostd.service` and `ghost-shell.service` into `~/.config/systemd/user/`;
- on NVIDIA machines only, the drop-in `ghost-shell.service.d/nvidia.conf` next to them. It runs the shell on NVIDIA's `egl-wayland` 1.x, which does not leak a buffer per frame as the newer `egl-wayland2` does, keeps Mesa's EGL (about 30 MiB) out of the process, and gives the shell its own shader cache in `~/.cache/ghost-shell`. It is installed only when the NVIDIA driver is loaded, the EGL files it names exist, and every DRM device is driven by `nvidia`, because it makes EGL fail on a system that renders through Mesa. Otherwise a copy installed earlier is removed.

It stops with an error if `cargo`, `fc-list`, `fc-cache`, `shell/` or the compiled shaders are missing. It does not enable or start anything, and it does not touch Plasma.

## Run

```sh
systemctl --user start ghostd ghost-shell
systemctl --user stop ghost-shell ghostd
```

Neither unit is enabled at login. Their logs, including every config reload and config problem, are in the journal:

```sh
journalctl --user -u ghostd -u ghost-shell -f
```

To go back to the previous desktop, stop both services and start the old HUD again, either with your own `dash` script or directly:

```sh
systemctl --user stop ghost-shell ghostd
qs -c gridlock &
```

## Configuration

`ghostd` reads `$XDG_CONFIG_HOME/ghost-shell/config.toml`, or `~/.config/ghost-shell/config.toml` when that variable is unset. Without the file it uses the defaults. To start from them:

```sh
mkdir -p ~/.config/ghost-shell
ghostd default-config > ~/.config/ghost-shell/config.toml
```

Changes are applied when the file is saved. A value that fails validation is reported as a notification, and the previous value stays in effect.

## Write your own widget

A widget is a folder with a `manifest.toml` and a `Widget.qml`. `widgets/placeholder` is the smallest example. `ghostd` finds widgets in `~/.local/share/ghost-shell/widgets/` (the built-in ones) and in `~/.config/ghost-shell/widgets/` (yours). The full widget contract will be written up in `docs/widgets.md`; that document does not exist yet.

## Performance

### Budgets

From the design spec (§15), for the shell and the daemon combined, averaged over 60 s:

| State | CPU (% of one core) |
|---|---|
| Desktop visible, default view animating | ≤ 2 %, and ≤ 300 MB RSS |
| Desktop visible, nothing animating | ≤ 1 % |
| Covered by windows | ≤ 0.5 %, zero dashboard frames |

Decorative motion runs at ≤ 15 fps in held steps, never at the display's 170 Hz.

### Test machine and method

- Ryzen 9 9950X3D (32 threads), RTX 4080 SUPER, 96 GB RAM, Fedora 44, kernel 7.2.11, KDE Plasma 6.7 Wayland, one 2560×1440 monitor at 170 Hz and scale 1.15. Measured 2026-10-20.
- Shell: `scripts/measure-cpu.sh <pid> 60` on the shell's own `qs` process; frames counted from `QSG_RENDER_TIMING=1` logs.
- Daemon collectors: release builds driven by a throwaway harness. Wall time comes from `Instant`; CPU time comes from the process's on-CPU time (`/proc/self/schedstat`, `/proc/thread-self/schedstat`). At 1 Hz, 1 ms of CPU per sample is 0.1 % of one core.
- Other things were running during these measurements, including `plasmashell` and the old `gridlock` HUD (which also polls the hwmon chip). Treat the figures as indicative, not lab-grade.
- **The shell's CPU figure depends on what else the machine is doing.** The same shell measures 1.05–1.60 % on an otherwise idle machine and 0.64–0.70 % while one unrelated thread keeps a core busy: each of its ≈ 15 wake-ups per second costs about twice as much when the CPU starts from deep idle. The idle-machine figure is the worst case.

### Rendering (shell)

Rendering spike with a full-screen transparent surface (dot grid plus test content). The scene is trivial, so these figures are a floor, not a forecast. Details: `docs/superpowers/notes/2026-10-20-rendering-spike.md`.

| Mode | CPU | RSS | Frames/s |
|---|---|---|---|
| Static | 0.00 % | 155 MiB | 0.00 |
| Data (600 rectangles recoloured at 1 Hz) | 0.15 % | 161 MiB | 1.00 |
| Animated, two independent timers (66 ms + 1 s) | 0.68 % | 162 MiB | 16.15 |
| Animated, one 67 ms clock | 0.65 % | 162 MiB | 14.93 |
| Animated by a looping `NumberAnimation` | 2.07 % | 163 MiB | 63.50 |

- The marginal cost is about 0.035 % of a core per frame/s.
- CPU-side frame time is at most 2 ms.
- Qt's animation driver ran at about 62.5 Hz. That is why decorative motion uses one 67 ms clock and no QML animation types.

### Daemon collectors

#### Memory and top processes (`collect/mem.rs`)

~800 `/proc` entries; about 490 of them are kernel threads.

| Version | 1 s spacing: wall / CPU per sample | 100 ms spacing: wall / CPU | Back-to-back: wall / CPU |
|---|---|---|---|
| First version (`stat` + `status` + `comm` per pid, `read_to_string`) | 7.87 / 7.36 ms | 6.06 / 5.70 ms | 4.67 / 4.69 ms |
| `stat` only, reused buffer, unsorted listing | 4.13 / 4.09 ms | 2.47 / 2.58 ms | 1.61 / 1.62 ms |
| Plus kernel threads skipped after first sight | **2.56 / 2.51 ms** | 1.14 / 1.15 ms | 0.90 / 0.91 ms |

All figures are medians. At the real 1 Hz cadence this version costs **≈ 0.25 % of a core**. Most of that is the kernel generating `stat` lines for about 340 user processes from cold caches.

**Current: processes rescanned every 2 samples.** Memory totals stay at 1 Hz, the top list is repeated in between, and process CPU % is the share over the 2 s window. Mean CPU per sample at 1 s spacing, 40 samples per run:

| Run | Every sample | Every 2 samples |
|---|---|---|
| 1 | 2.9 ms | 1.55 ms |
| 2 | 5.1 ms | 2.98 ms |
| 3 | 5.3 ms | 2.33 ms |

The new figures are 44–58 % of the old ones. The runs differ with machine activity.

Where the cost came from (one pass over every pid, ~790–850 entries):

| Read | Time per pass |
|---|---|
| `stat`, `File::open` + one read into a reused buffer | ≈ 1.2 ms |
| `stat` via `read_to_string` | 1.7–3.4 ms |
| `status` via `read_to_string` (≈ 1 MB) | 2.5–3.4 ms |
| `comm` via `read_to_string` | 1.0–2.0 ms |
| List `/proc` (collect + sort) | 0.26–0.72 ms |

#### Temperatures and fans (`collect/hwmon.rs`)

First version: 22 temperatures and 2 fans, every chip read every sample.

| Cadence | Wall per sample (median / mean / max) | CPU per sample (median / mean) |
|---|---|---|
| 1 s spacing, n = 61 | 4.9 / 6.0 / 12.9 ms | 0.51 / 1.25 ms |
| 100 ms spacing, n = 146 | 4.5 / 4.9 / 11.6 ms | 0.37 / 0.65 ms |

By chip, 1 s spacing, n = 20:

| Chip | Files read | Wall | CPU |
|---|---|---|---|
| `nct6799` (Super-I/O: motherboard temperature, fans) | 19 | median 5.6 ms (≈ 50 µs when cached) | median 4.65 ms, mean 2.6 ms |
| `spd5118` ×2 (DIMMs, i2c) | 2 | 3.5 ms | 0.36 ms |
| `nvme` ×3 | 3 | 3.9 ms (blocked, ≈ 1.3 ms per drive) | 0.09 ms |
| `k10temp` | 3 | 55 µs | ≈ 0 |
| `r8169` | 1 | 53 µs | ≈ 0 |

- **Nearly all of the CPU cost is the `nct6799` chip.** Its driver caches readings for 1.5 s, and any read after that refreshes the whole chip, costing ≈ 5.7 ms of CPU and 10–13 ms of wall time. At 1 Hz that happens on every second sample, about 3.3 ms of CPU per second, or ≈ 0.33 % of a core. Reading fewer of its files does not help.
- **Fix: Super-I/O chips (`nct*`, `it8*`) are read every 5 samples, and their last values are repeated in between.** Measured on-CPU time per sample (`CLOCK_THREAD_CPUTIME_ID`), 40 samples at 1 s spacing, old and new code run alternately for two rounds each:

  | Version | Mean CPU per sample | Median CPU per sample |
  |---|---|---|
  | Every chip read every sample | 3.31 / 3.28 ms | 3.30 / 3.24 ms |
  | Super-I/O chips every 5 samples (current) | **1.01 / 0.89 ms** | 0.45 / 0.45 ms |

  The current version costs **≈ 0.1 % of a core**. The baseline is ≈ 0.45 ms, with a ≈ 6 ms chip refresh on 4 of 40 samples. A lone reader would refresh on every 5th sample, for an expected mean of ≈ 1.65 ms (≈ 0.17 %). Here gridlock was also reading the chip and keeping its cache warm, so the measured mean is lower.
- The NVMe and i2c reads block for milliseconds but use little CPU: a sample still takes ≈ 4.5–5 ms of wall time. The daemon therefore runs its collectors off its event loop.

#### GPU (`collect/gpu.rs`, NVML)

RTX 4080 SUPER, driver 615.78.08. Two runs of 40 reads each, at 1 s spacing:

| Operation | Wall | CPU |
|---|---|---|
| `NvmlSource::open()` (NVML init, once) | 4.2–4.6 ms | — |
| First `read()` | 4–9 ms | — |
| Steady-state `read()` | 0.76 ms median, 0.78–0.79 ms mean | ≈ the same (no blocking) |

- About 0.61 ms of each read was `fan_speed`, so at 1 Hz reads cost ≈ 0.08 % of a core.
- **Current: the fan is read every 5 reads**, and the cached value is repeated in between. A failed fan read keeps the GPU absent until the next successful one. Mean CPU per read fell from 0.77–1.00 ms to **0.22–0.32 ms** (≈ 0.03 % of a core). There were no absent reads in 120.
- NVML init adds **+25.7 MiB RSS** to the daemon, about 20 MiB of it anonymous memory.

#### Drives (`collect/disk.rs`)

Five drives: three NVMe, one of them dead, plus two SATA. Thirteen loop devices and zram are filtered out. 1 s spacing:

| Sample | CPU |
|---|---|
| Ordinary, first version (reads `mountinfo` + `diskstats`) | ≈ 165 µs (max 221 µs) |
| Ordinary, current (also re-reads each drive's `device/state` and `size`, so a dying drive turns Dead within one sample) | ≈ 273 µs mean (max 375 µs) |
| Rescan of `/sys/block` + `statvfs` (every 60 samples) | ≈ 365 µs |

**≈ 0.03 % of a core.** `statvfs` on a hung FUSE or network mount would block the sample. The Kingston drive's NTFS partition is mounted through FUSE (`fuseblk`).

#### The whole daemon (`ghostd run`)

Release build, real collectors, 1 s tick, measured over 60 s with `scripts/measure-cpu.sh`:

| State | CPU (% of one core) | RSS |
|---|---|---|
| No client connected | **0.53 %** | 31.4 MiB |
| One UI client subscribed to all six built-in topics | **0.55 %** | 31.5 MiB |

- **Variance.** Idle CPU varies with what else the machine is doing. The same build, measured over four alternating 60 s rounds, ranged from **0.57 % to 1.17 %**. The live widget and config watchers used 0 CPU ticks in those runs, so the variance comes from the collectors: busier machines have more processes and colder caches to read.
- **Budget.** On a quiet machine the daemon alone used about half of the 1 % "nothing animating" budget. On a busy one it could use all of it. Three cuts were considered:
  1. refresh the top-process list every 2 samples — done;
  2. read the GPU fan every 5 samples — done;
  3. ~~cache the per-CPU frequency reads~~ — measured below: ≤ 0.01 % to gain, not worth doing.
- **After cuts 1 and 2.** Three alternating 60 s idle runs, no client, in separate builds:

  | Build | Runs | Mean |
  |---|---|---|
  | Before | 0.55, 0.52, 0.50 % | 0.52 % |
  | After | 0.30, 0.33, 0.30 % | **0.31 %** |

  RSS is unchanged at ≈ 32 MiB. The machine was quieter during these runs than during the 0.57–1.17 % runs above.
- **Breakdown.** The collector figures above roughly account for the total: about 0.25 % for processes, about 0.1 % for sensors, about 0.08 % for the GPU, about 0.03 % for drives, plus CPU sampling, JSON encoding and the socket.
- **`perf` granularity.** The daemon's own per-tick `perf` value moves in 1 % steps (clock-tick resolution), so the shell should average it.

#### CPU frequency reads (`collect/cpu.rs`)

The CPU collector reads `/proc/stat` plus one `scaling_cur_freq` per thread. Driver `amd-pstate-epp`, governor `performance`. These figures come from a Python probe, so they include interpreter overhead and are an upper bound for the Rust collector:

| Pass | CPU per pass |
|---|---|
| 32 files, back-to-back (warm caches) | ≈ 53 µs |
| 32 files, 100 ms apart | ≈ 162 µs |
| 16 files (one thread per core), 100 ms apart | ≈ 85 µs |

- At 1 Hz the frequency reads cost **≈ 0.01–0.016 % of a core**.
- Reading them causes no measurable inter-processor interrupts: `amd-pstate-epp` serves a cached value instead of waking each core.
- Caching them, or reading one thread per core, would save ≤ 0.01 %, so it is not worth doing.

### Shell skeleton (`qs -p shell`)

This is the desktop surface with the dot grid and scanlines and no panels yet, connected to `ghostd --fake`. Measured over 60 s with `scripts/measure-cpu.sh` on the shell's own `qs` process:

| State | CPU (% of one core) | RSS | Frames in 60 s |
|---|---|---|---|
| Idle, no subscriptions | **0.02 %** | 156 MiB | 0 |
| Subscribed to all six built-in topics | **0.03 %** | 155 MiB | — |

- With no animation running, the surface renders no frames.
- Most of the RSS is Qt and the scene graph, the same ≈ 155 MiB the rendering spike showed.

### GhostKit component gallery (`GHOST_VIEW=gallery`)

This view shows three panels, four segmented meters, a block-graph ramp, a "20:07" digit readout and two animated alert boxes (moving stripes and the held-step blink). It runs against `ghostd --fake` and was measured over 60 s:

| CPU (% of one core) | Frames/s | RSS |
|---|---|---|
| **0.73 %** | 14.92 (never above 15) | 204 MiB |

- **A memory bug found while measuring this.** At startup the window reports a few provisional widths (100, 0, 500 px) before its real one. The design unit followed them, so the block graph briefly built ≈ 26 000 cells, and RSS reached 367 MB after 65 s. The design unit now comes from the screen width.

### Default V1 with real panels (Processor, Graphics, Memory)

Measured against the real release daemon over 60 s. Storage and Sensors weren't built yet, so their two slots showed animated red error cards.

| State | Shell CPU / RSS / frames/s | Daemon CPU / RSS | Shell + daemon |
|---|---|---|---|
| Two error cards animating | 1.22 % / 219 MB / 14.93 | 0.33 % / 32 MB | **1.55 %** (budget ≤ 2 %) |
| Nothing animating | 0.55 % / — / 1 | 0.33 % | **≈ 0.9 %** (budget ≤ 1 %) |

- **Alert blink.** It first changed each alert's opacity directly, which made Qt rebuild every scene batch four times a second (about 2 600 elements). That alone put the view at 2.01 %. The blink now goes through a shader uniform on the alert's own layer, at the same held steps.
- **A stress layout** with large graphs everywhere costs 2.42 % / 297 MB in the shell. The block graph draws one rectangle per cell, so Plan 2's full-width lanes need a shader-based graph.

### Full default V1 (all five machine panels)

Real release daemon, 60 s. The dead WD Green's crit alert animates (stripes plus the held-step blink):

| Process | CPU (% of one core) | RSS | Frames/s |
|---|---|---|---|
| Shell (`qs`) | 1.25 % | 214 MiB | 14.93 |
| Daemon (`ghostd`) | 0.33 % | 31 MiB | — |
| **Together** | **1.58 %** (budget ≤ 2 %) | **245.6 MiB** (budget ≤ 300 MB) | |

A second 60 s run after the next round of fixes measured 1.03 % for the shell (14.92 frames/s) and 0.33 % for the daemon: **1.36 %** together, 245.9 MiB. The runs differ by machine activity rather than by code: those fixes don't touch the render path.

### Plan 1 acceptance: installed services (`ghostd` + `ghost-shell` user units)

These measurements are of the installed release build on the live session, with gridlock stopped, Kode Mono installed and the default config plus this machine's settings. The dead drive's crit alert animates throughout. `scripts/measure-cpu.sh` ran on both service PIDs at once:

| Run | Shell CPU | Daemon CPU | **Combined CPU** (≤ 2 %) | **Combined RSS** (≤ 300 MB = 307 200 KiB) |
|---|---|---|---|---|
| 1 (60 s) | 0.95 % | 0.37 % | 1.32 % | **313 604 KiB, over budget** |
| 2 (60 s) | 1.50 % | 0.42 % | 1.92 % | 253 880 KiB |
| 3 (300 s) | 1.49 % | 0.40 % | 1.89 % | 254 936 KiB |

- **Memory.** Run 1's overrun came from one ≈ 74 MiB anonymous memory block. It appeared in only 3 of 13 shell instances. Without it, shell + daemon use ≈ 254 MB. The block is NVIDIA's shader compiler; see the investigation below.
- **CPU.** The margin is thin. The installed shell measured ≈ 1.5 %, against 1.0–1.25 % for the development copy in Task 17. The difference is the machine's background load, not the build; see the investigation below.
- **The "nothing animating" budget (≤ 1 %) can't be measured on this machine.** Its dead drive keeps a crit alert animating permanently.
- **Frames.** They were not counted on the service itself, because that needs a restart with a render-timing variable. An identical copy rendered 15.04 frames/s.
- **Live config.** All four checks passed: panel fill 30, an invalid colour, swapping two slots, and restoring. Each reload took ≈ 150 ms.

### Plan 1 follow-up: the shell's memory and CPU

Measured 2026-10-21 with copies of the installed shell against a release daemon, plus one sample a minute of the running service, read-only. CPU runs were 60 s after a 20 s warm-up, interleaved across variants.

#### The ≈ 74 MiB block: NVIDIA's shader compiler

When NVIDIA's on-disk shader cache has no usable entry at start-up, the driver compiles the shell's shaders itself and keeps ≈ 40–57 MB of compiler memory for the life of the process. The cache misses when it is empty, and when another `qs` process opens the same cache file at the same moment. With a private cache directory, empty at first:

| Start | Anonymous RSS after 10 s |
|---|---|
| First, alone, empty cache | 123 868 KiB (block present) |
| Second, alone | 71 656 KiB |
| Three at once | 70 608 / 123 104 / 124 008 KiB (block in two) |

With NVIDIA's disk cache turned off, every start has the block: 4 of 4, and 7 of 7 with Qt's own shader cache also off.

#### Steady growth: a leak in NVIDIA's `egl-wayland2`

The running service grew linearly, by 213 KiB/min (≈ 12.5 MB/h), for 2 h 35 min:

| Time | Service VmRSS |
|---|---|
| 04:42, end of acceptance run 3 | 222 212 KiB |
| 04:51, after the five config reloads | 241 312 KiB |
| 05:08 | 244 980 KiB |
| 06:38 | 264 084 KiB |
| 07:43 | 278 052 KiB |

- All of the growth is native heap. The QML/JS heap stayed at 4 860 kB throughout.
- **Cause:** NVIDIA's `egl-wayland2` 1.0.2 leaves each buffer's `wl_buffer.release` events on an event queue it never dispatches, so one ≈ 225-byte event per frame is never freed: ≈ 3.5 KiB/s at 15 frames/s. With NVIDIA's older `egl-wayland` 1.1.24, 177 releases arrive for 178 frames and the growth stops.
- With the daemon at ≈ 33 MB, shell + daemon passed the 307 200 KiB budget after ≈ 2 h 48 min of uptime.
- **Config reloads:** the first one costs a one-off ≈ 19 MB (the rebuilt widget tree plus allocator fragmentation). Later reloads add 0–4 MB.

#### Shell CPU and RSS by variant (idle machine)

| Variant | Mean CPU | RSS |
|---|---|---|
| Baseline, development copy (15 runs, 1.05–1.52 %) | **1.33 %** | 213–220 MiB |
| Installed copy with the service's exact environment | 1.23 % | 214–217 MiB |
| Fallback font instead of Kode Mono | 1.37 % | 213–216 MiB |
| `QT_NO_GLIB=1` | 1.42 % | — |
| `__GL_YIELD=USLEEP` | 1.48 % | — |
| Threaded render loop | 1.55 % | 232–234 MiB |
| `egl-wayland` 1.x instead of `egl-wayland2` | 1.38 % | 214–216 MiB |
| Only NVIDIA's EGL loaded (no Mesa) | 1.39 % | **185–187 MiB** (−29 MiB) |
| Both of the above plus a private shader cache | 1.36 % | 186 MiB |
| Animated alert on its own small surface | 1.58 % | 217–219 MiB |
| Animation clock at 4 / 7.5 / 8 fps | 0.61 / 0.91 / 0.94 % | — |
| No animation | **0.32 %** | 214–217 MiB |
| Vulkan instead of OpenGL | — | 355 MiB |

- **About three quarters of the shell's CPU is the alert animation.** Each of its 15 frames per second costs ≈ 0.73 ms of CPU, mostly fixed overhead: NVIDIA's GL driver 34 %, Qt Quick's renderer 18 %, QML bindings 11 %, Qt Gui 9 %, Qt Core 9 %, libc and the allocator 9 %. A smaller surface for the animated region doesn't help.
- Without Mesa's EGL, LLVM and gallium are no longer loaded. Applied to the acceptance runs, shell + daemon would fall from 253 880 to ≈ 223 840 KiB, and run 1's case with the block from 313 604 to ≈ 283 560 KiB.
- **Load test.** The same instance, consecutive 25 s windows, with one `yes` loop as the only extra load:

  | Instance | Idle | Busy thread, same CCD | Busy thread, other CCD | Idle again |
  |---|---|---|---|---|
  | 1 | 1.40 % | 0.68 % | 0.64 % | 1.36 % |
  | 2 | 1.48 % | 0.64 % | 0.68 % | 1.48 % |

  The service itself read 1.50 % on the idle machine at 04:55 and 0.70 % at 07:33 while another session's test run kept the CPU busy. Task 17's 1.03–1.25 % and acceptance run 1's 0.95 % were taken while other work ran.

#### After the NVIDIA drop-in (installed service)

`scripts/install.sh` now gives the shell unit an NVIDIA-only drop-in: `egl-wayland` 1.x, only NVIDIA's EGL, and a private shader cache. The service was restarted with it at 08:15. Its process maps `libnvidia-egl-wayland.so.1.1.24` and no `libLLVM`.

| | Before (old service, 3 h 38 min up) | After the restart |
|---|---|---|
| Shell VmRSS | 284 340 KiB at 08:14, still growing | 251 708 KiB at 08:16 |
| Growth | 207–213 KiB/min | +348 KiB in 15 min (≈ 23 KiB/min), within the ±200 KiB minute-to-minute noise |
| Shell + daemon | ≈ 317 600 KiB (over the 307 200 KiB budget) | **285 000 KiB** |

- This first start found the private shader cache empty, so it carries the ≈ 74 MiB compiler block. Later starts read the cache: the same environment measured 189 MB warm against 249 MB cold in a development copy.

### Plan 2a: demand-driven sampling (daemon)

The daemon now reads only the sources that some client is subscribed to. It also runs on-demand feeds (desktop state, weather, sky) only while something wants them. Release build, real collectors, 60 s:

| Subscribers | Daemon CPU | RSS |
|---|---|---|
| None | **0.00 %** (0 CPU ticks) | 31.2 MiB |
| One client on `desktop` + `perf` only | **0.02 %** | 31.3 MiB |
| One client on every built-in topic | **0.38 %** | 32.1 MiB |

- While the dashboard is covered (Plan 2a's occlusion work), the daemon can drop to the first or second row.
- The history graphs show the skipped time as a gap, not as joined-up lines.

### Plan 2a: per-topic data in the shell

`Data` now keeps one object per topic, so a message re-evaluates only the bindings on its own topic. Previously every message re-ran every topic binding. Default V1 against `ghostd run --fake`, six interleaved 60 s runs; the installed services and several other sessions were running:

| Build | Runs | Mean CPU | RSS |
|---|---|---|---|
| Before (a73d3b9) | 1.62, 1.70, 1.70 % | 1.67 % | 219–222 MiB |
| After | 1.75, 1.65, 1.63 % | 1.68 % | 219–222 MiB |

No measurable change on V1. Its cost is mostly the 15 fps alert animation, and its data arrives at 1 Hz. The gain is meant for high-rate topics such as Plan 2a's 30 Hz audio spectrum.

### Plan 2a: clock modes (GhostKit gallery)

The gallery at 2560×1440 against `ghostd run --fake`, 30 s per state, the shell's `qs` process only. The session was loaded: Plasma, the installed services, browsers, chat apps and other sessions, load average ≈ 1.0–1.3.

| State | Frames/s | CPU | RSS |
|---|---|---|---|
| Stripes alone (normal clock, 67 ms) | 14.93 | 0.60 % | 218 MiB |
| Fast mode held (33 ms) | 30.33 | 1.10 % | 226 MiB |

The blink period stays 1.00 s in both (999.4 ms and 999.0 ms measured), because every motion follows `FrameClock.elapsedMs` rather than the tick count.

### Plan 2a: KWin desktop script

The script has no timers and does no polling. It evaluates the stacking order once per relevant window event and calls ghostd's `Report` only when the covered outputs or the focus state change. It also reads KWin's `showingDesktop` property once on load, once per new window and once per show-desktop change. On the live session, a 20 s window with no window events made 0 calls. `kwin_wayland` read 2.00 % of a core over those 20 s with the dashboard animating; that is KWin's own baseline, and no comparison without the script was taken.

## License

Copyright (C) 2026 Gabriel Mitelman Tkacz

GPL-3.0-or-later; see [LICENSE](LICENSE).

# ghost-shell

A Tron: Legacy / Cyberpunk 2077 / Ghost in the Shell desktop shell for KDE Plasma (Wayland): a Quickshell UI fed by a Rust daemon, `ghostd`.

## Build

Requires a recent stable Rust toolchain, Quickshell (`qs`, Qt 6) and `qsb-qt6` for the shaders. The daemon's audio spectrum binds to PipeWire, so building also needs `pipewire-devel` and `clang`, and `ghostd` (including `ghostd run --fake`) needs `libpipewire-0.3` at run time.

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

Three environment variables adjust a smoke run:

- `GHOST_CONFIG=<path>` runs it against that config file instead of a temporary copy of the defaults (this is how `make smoke-layout` works).
- `GHOST_SMOKE_ALLOW=<extended regex>` accepts extra shell-log lines, for example a widget the layout names before it exists. A pattern that does not compile, or that matches an empty line, fails the run instead of hiding every error.
- `GHOST_SMOKE_SOCKET=<path>` runs it on that socket instead of `/tmp/ghost-smoke.sock`, with its logs beside it (`<path without .sock>.log` and `-daemon.log`), so two smokes can run at once.

The smoke tests run their daemon without a session bus, so they never raise desktop notifications.

## Install

```sh
make install
```

This runs `make release` and then `scripts/install.sh`. The script is idempotent and installs for the current user only:

- `ghostd` and the INMET alert provider `ghost-alerts-inmet` into `~/.local/bin/`;
- `shell/` (with its compiled shaders) and `widgets/` (without `broken-example`) into `~/.local/share/ghost-shell/`;
- the Kode Mono font into `~/.local/share/fonts/ghost-shell/`, if it is not installed yet;
- the user units `ghostd.service` and `ghost-shell.service` into `~/.config/systemd/user/`;
- the KWin desktop script `ghost-shell-desktop`, through `scripts/kwin-script.sh install`. It is never enabled; enable it once with `scripts/kwin-script.sh enable`. KWin keeps running the copy it loaded, so an update to an enabled script takes effect at the next login;
- on NVIDIA machines only, the drop-in `ghost-shell.service.d/nvidia.conf` next to them. It runs the shell on NVIDIA's `egl-wayland` 1.x, which does not leak a buffer per frame as the newer `egl-wayland2` does, keeps Mesa's EGL (about 30 MiB) out of the process, and gives the shell its own shader cache in `~/.cache/ghost-shell`. It is installed only when the NVIDIA driver is loaded, the EGL files it names exist, and every DRM device is driven by `nvidia`, because it makes EGL fail on a system that renders through Mesa. Otherwise a copy installed earlier is removed.

It stops with an error if `cargo`, `fc-list`, `fc-cache`, `kpackagetool6`, `kreadconfig6`, `shell/` or the compiled shaders are missing. It does not enable or start anything, and it leaves the running session alone.

### CPU package power (optional)

The processor panel shows `PKG` (the CPU package's power draw) only when `ghostd` can read the RAPL energy counter, `/sys/class/powercap/intel-rapl:0/energy_uj`. The kernel makes it root-only, so the row stays hidden until you install the udev rule, which `scripts/install.sh` never does:

```sh
sudo install -m 0644 packaging/udev/70-ghost-shell-rapl.rules /etc/udev/rules.d/
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=powercap --action=change
```

The rule makes that one file world-readable (0444). **Trade-off:** the counter is a power side channel, and PLATYPUS (CVE-2020-8694) used it to recover cryptographic keys from Intel CPUs, which is why the kernel restricts it. Any local user or process can read it once the rule is in place. `ghostd` retries the read on every sample, so it needs no restart. To revert, delete the rule file and either reboot or run `sudo chmod 0400 /sys/class/powercap/intel-rapl:0/energy_uj`, because removing the rule does not restore the mode of a running system.

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
| Desktop visible, default view animating | ≤ 2 %, and ≤ 300 MB RSS, each page counted once (`smaps_rollup`) |
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

#### Memory and top processes (`collect/procs.rs` for the process list, topics `procs` and `procs.tree`; `collect/mem.rs` for memory)

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

### Plan 2a early install (installed services, 2026-10-21 14:01)

The Plan 2a build so far (Tasks 1–14), installed on the live session with the clock in V1. The services were restarted at 14:01. Each process was measured for 30 s, one after the other, at 14:02. The dashboard was visible and the dead drive's alert was holding still. Load average was 1.1–1.7, with other Claude sessions running:

| Process | CPU | RSS |
|---|---|---|
| Shell (`qs`) | 0.57 % | 205 824 KiB |
| Daemon (`ghostd`, with the INMET provider started) | 0.47 % | 38 608 KiB |
| **Together** | **≈ 1.04 %** (Plan 1's acceptance: 1.32–1.92 %) | **≈ 244 MiB** |

### Plan 2a: demand-driven sampling (daemon)

The daemon now reads only the sources that some client is subscribed to. It also runs on-demand feeds (desktop state, weather, sky) only while something wants them. Release build, real collectors, 60 s:

| Subscribers | Daemon CPU | RSS |
|---|---|---|
| None | **0.00 %** (0 CPU ticks) | 31.2 MiB |
| One client on `desktop` + `perf` only | **0.02 %** | 31.3 MiB |
| One client on every built-in topic | **0.38 %** | 32.1 MiB |

- While the dashboard is covered (Plan 2a's occlusion work), the daemon can drop to the first or second row.
- The history graphs show the skipped time as a gap, not as joined-up lines.

### Plan 2a: weather feed (Open-Meteo)

Three HTTPS requests per 30 min while the `weather` topic is wanted: the forecast every 15 min and air quality every 30 min. None while nothing subscribes. Release build, São Paulo:

| Fetch | Wall | CPU |
|---|---|---|
| Forecast, first in the process (cold) | 888 ms | 1.76 ms |
| Air quality, right after it | 868 ms | 1.41 ms |
| Forecast, warm, on a new connection | 880 ms | 1.17 ms |
| Air quality, warm, on a new connection | 859 ms | 0.98 ms |

- **Per 30 min:** ≈ 3.3 ms of CPU, ≈ 0.0002 % of a core. Parsing a response takes ≈ 10 µs.
- **While wanted:** the feed also wakes once a minute to compare the wall clock with the last fetch, so a system suspend can't leave old weather looking current. Each wake is a clock read.
- **Daemon, 60 s:**

  | State | CPU | RSS |
  |---|---|---|
  | Before the weather feed | 0.00 % | 32 324 KiB |
  | Weather feed present, not wanted | 0.00 % | 32 884 KiB |
  | After a fetch | 0.02 % | 36 032 KiB |

  The HTTP client (ureq with rustls) adds ≈ 3.1 MiB of RSS once used, and the binary grows by 2.69 MiB (8 345 008 → 11 165 144 bytes).

### Plan 2a: sky feed (sun and moon, computed locally)

No network. While the `sky` topic is wanted, the feed recomputes once a minute from the wall clock. Release build:

| Work | CPU |
|---|---|
| Sunrise, sunset and moon for one instant | 0.39 µs |
| One feed cycle (timer, snapshot, serialisation, send) | 1.8 µs |

With a client subscribed to `sky`, `scripts/measure-cpu.sh` reads 0.00–0.02 % over 60 s, which is its clock-tick floor. Over 120 s from `schedstat` the daemon spent 10.2 ms on CPU (0.0085 %), against 9.7 ms (0.0081 %) with the client subscribed to nothing.

### Plan 2a: INMET alert provider (`ghost-alerts-inmet`)

A separate process that ghostd starts from `location.alerts`. It makes one HTTPS GET every 10 minutes while it runs. Release build, São Paulo:

| Measure | Value |
|---|---|
| Response size | 589 791 bytes per fetch (≈ 85 MB a day) |
| Fetch wall time | 0.35–0.44 s |
| CPU per fetch | ≈ 2.5 ms (under 10 ms process-wide); 0 between fetches |
| Parse of the response | 0.18 ms warm (entries read one at a time, so one malformed entry is skipped alone) |
| Provider RSS | 6.8–7.0 MiB |
| Daemon's added cost | not measurable (≈ 0.071 ms/s of CPU with the provider, 0.072 without; RSS +0.2 MB) |

### Plan 2a: audio spectrum feed (`audio.spectrum`)

PipeWire capture of the default output's monitor (never a microphone), FFT 2048 into 32 bands, 30 frames/s while some client wants the topic. The capture is passive, so it never keeps the audio device awake by itself. Release build:

| State | Daemon CPU | Notes |
|---|---|---|
| Not wanted | 0.03 % (the daemon's idle) | no capture stream |
| Subscribed, silence | 0.10 % | before the capture became passive |
| Subscribed, music playing | 0.13 % | 300 of 300 frames non-zero |

- **Per frame:** ≈ 19 µs on the capture thread and ≈ 19 µs on the publishing thread. The analyzer alone takes 3.3 µs.
- **RSS:** ≈ +4.8 MB while capturing, ≈ +2 MB kept after the first stop.
- **Start and stop:** the `ghostd-spectrum` node is gone ≈ 4 ms after the last subscriber leaves. 10 start/stop cycles left nothing behind, and a SIGTERM mid-capture exits in 11 ms.
- **Not yet measured:** with nothing playing, whether the output sleeps while subscribed, and the CPU then. The user's browser kept the output running throughout.

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

### Plan 2a: view switching (CRT print transition)

The real shell on DP-1 (scale 1.15) against a fake daemon on a private bus, with the test config's placeholder views. `QSG_RENDER_TIMING=1`; shell CPU over the 1.5 s from each action; load average ≈ 0.3–0.7:

| Action | Frames | Shell CPU | Print ends |
|---|---|---|---|
| None (idle) | 0 | 0.2 ms | — |
| v1 → monitor (640 ms) | 34 | 27.4 ms | +654 ms |
| monitor → v1 (640 ms) | 26 | 20.4 ms | +653 ms |
| v1 → v2 swap (380 ms) | 32, plus 3 scramble frames after | 27.4 ms | ≈ +404 ms |

- Shell RSS 178 MiB. After each print only the incoming view's widgets exist.
- With real, narrow widgets one print costs ≈ 70–80 ms of shell CPU above idle.
- **A heavy incoming view delays the print by its creation time.** With the Graphics widget in a half-width slot, the first frame came at +808 ms, the shell spent 0.9 s of CPU and RSS reached 517 MiB. The cause is the block graph's one-rectangle-per-cell design, which a shader-based graph would replace.

### Plan 2a: covered by windows (occlusion)

`scripts/measure-covered.sh` runs a release `ghostd` with real collectors and the shell on a private bus. It reports the shell's output as covered, measures both processes, then reports it uncovered again. DP-1 was the only screen. The config is the default, so there is no location and no provider, and `GHOST_DEBUG` is off. The installed services kept running throughout.

| State | Window | Dashboard frames | Shell CPU | Daemon CPU | **Total** (budget ≤ 0.5 %) |
|---|---|---|---|---|---|
| Covered, before the change | 30 s | 448 | 1.43 % | 0.40 % | 1.83 % |
| Covered, after (run 1) | 60 s | **0** | 0.03 % | 0.02 % | **0.05 %** |
| Covered, after (run 2) | 60 s | **0** | 0.02 % | 0.02 % | **0.04 %** |
| Visible, after the change | 60 s | 896 | 1.43 % | 0.32 % | 1.75 % (budget ≤ 2 %) |

- While covered, the view is unloaded, the frame clock is suspended (including the seconds clock), and the shell subscribes only to `desktop` and `perf`. The dot grid and scanlines stay on screen, because they are static.
- The dashboard comes back within 2 s of the uncover, with no print transition.
- Covering mid-print drew 1 frame, then none. Uncovering drew 1 frame; a print draws about 26.
- The visible figure animates at 15 fps because the dead `nvme1n1` keeps the Storage alert running. 1.75 % is inside the 2 % budget with little headroom.

### Plan 2a: weather widget

A V1-like layout with the weather slot, against `ghostd run --fake`, 30 s per state, the shell's `qs` process only:

| State | Shell CPU | Frames in 30 s |
|---|---|---|
| Weather with its alert animating | 1.47 / 1.60 % | 478 (15.9/s) |
| Weather, alert hidden | 0.57 % | 30 |
| The weather slot replaced by a placeholder | 0.53 % | 30 |

- **Without an alert the widget is nearly free.** It holds the seconds clock while visible, so alert expiry, the dropping of past hours and the staleness check can run. That costs about 0.04 % and adds no frames.
- **An active alert costs what any alert does:** the 15 fps stripes and blink.

### Plan 2a: clock widget

The three styles stacked (hero, band, compact), against a fake daemon, 30 s each after a 10 s warm-up, native scale. The session load average was 0.7–1.1:

| On screen | Shell CPU | Frames/s |
|---|---|---|
| Three placeholders | 0.00 % | 0 |
| Three clocks, no alert | 0.13 / 0.17 % | 1.00 |
| Three clocks, with the hero and band alerts animating | 0.73 / 0.73 % | 15.93 |

### Plan 2a: alerts hold still after 10 s (by default)

An alert blinks for its first 10 s on screen, or `appearance.alert_attention_s`, then holds still. The time counts only while the alert is visible and the desktop isn't covered. Measured on the default V1 with the dead drive's alert, against a fake debug daemon, over 60 s starting 20 s after start. `godot-bin` was using ≈ 75 % of a core and `kwin_wayland` ≈ 6 % throughout:

| Build | Frames in 60 s | Shell CPU | Daemon CPU |
|---|---|---|---|
| Blinking for as long as the alert lasts | 896 (14.93/s) | 1.30 % | — |
| Holding still after 10 s | **60** (1/s) | **0.63 %** | 0.07 % |

The remaining 1 frame/s is the data panels repainting with each sample. The held alert adds nothing.

**The 10 s count only time on screen.** The fake daemon ran on a private bus while the default V1 showed the dead drive's alert:

| Phase | Frames/s |
|---|---|
| Blinking, first 4.0 s | 15.00 |
| Covered for 12 s | 0 |
| Uncovered: blinking for the remaining 6.05 s | 14.8 |
| Held | 2.00 |

**Held stripes step once a second.** Once an alert has held for its first 10 s, its stripes keep moving in one small jump a second, at the same average speed as the scroll. The text stays still. Held state, 20 s with `QSG_RENDER_TIMING=1`, fake daemon on a private bus:

| Config | Before | After |
|---|---|---|
| Default V1, placeholder Clock | 1.05 frames/s | 2.05 frames/s |
| V1 with the clock widget | 2.05 frames/s | 2.05 frames/s |

- Shell CPU read 0.60–0.70 % in every run.
- With a placeholder clock, the held alert is the only thing on the seconds clock, so its once-a-second frame doesn't line up with the data frame. That costs about 0.035 % of a core.
- Any widget that already runs once a second (clock, weather, media) absorbs the step, so it adds nothing.

**The attention window is a setting** (`[appearance] alert_attention_s`, default 10). Headless KWin, fake debug daemon on a private bus, default V1 with the dead drive's alert, which shows from about t = 2 s. Frames rendered in each second since start, from `QSG_RENDER_TIMING=1`:

| Case | Frames per second, t = 0, 1, 2, … |
|---|---|
| `alert_attention_s = 3` | 14 2 **16 14 15** 2 2 2 2 2 2 2 1 3 2 … |
| Default 10 | 14 2 **16 14 15 15 14 16 15 14 15 16** 1 3 2 2 … |
| 3, raised to 6 at t = 3.5 | 15 frames/s through t = 7, then 2 (the window grew) |
| 10, lowered to 3 at t = 5.1, after 3.1 s spent | 14 2 14 16 15 **6** 2 2 … (holds at once) |
| Held at 3, raised to 20 | 2 frames/s throughout (a spent alert stays held) |
| 600 | 15 frames/s still at t = 23 |

- An alert in its window renders 15 frames/s, and a held one 2: the stripe step and the 1 Hz data. A blinking alert measured 1.30 % before the shader graphs (the table above). Blinking hasn't been measured with them; the nearest reading is held stripes that keep scrolling, at about 1 % (below).

**Stripe speed and step are settings** (`[appearance] alert_stripe_period_s`, default 1.6, and `alert_stripe_step_s`, default 1.0). Held state on the default V1 with the dead drive's alert, counted from 15 s to 35 s after start, fake daemon on a private bus:

| Setting | Held frames/s |
|---|---|
| Defaults | 2.05, then 2.00 on a second run (the build before the settings gave 1.10, then 2.05) |
| `alert_stripe_step_s = 0.5` | 2.45 (2 or 3 in any one second) |
| `alert_stripe_period_s = 3.2` | 2.00 |

- The defaults vary from launch to launch between about 1 and 2 frames/s, with or without this change. When the daemon's 1 Hz data tick lands in the same vsync as the wall-second boundary, the two share a frame.
- A step that isn't whole seconds keeps the 15 fps frame clock running while the alert is held. That is about 15 wakeups a second, though Qt still renders only on steps.

**The user's own stripe settings**, against the defaults. Installed shell at `596648d` in a headless KWin (2560×1440, scale 1), fake daemon on a private bus, default V1 with the dead drive's alert held. Counted over 42 s starting 25 s after start, two runs each, alternating with the defaults. Load average 0.7–1.7:

| Setting | Frames/s | Shell CPU |
|---|---|---|
| Defaults (period 1.6, step 1.0) | 1.98, 2.02, 2.02, 2.02 | 0.72 %, 0.67 %, 0.70 %, 0.70 % |
| Twice as fast (period 0.8, step 0.5) | 3.00, 2.67 | 0.85 %, 0.75 % |
| Held stripes keep scrolling (period 1.6, step 0.01; the user's config from 17:04) | 14.90, 14.90 | 1.22 %, 1.20 % |

- Twice-as-fast held stripes cost about 0.1 % of a core and 1 frame/s more than the defaults.
- Stripes that keep scrolling cost about 0.5 % of a core more than the defaults, the same as a blinking alert, and pass the 1 % idle budget while an alert shows on the visible desktop. Covered, it costs nothing.
- With block graphs drawn by one shader (installed `67dbc8f`, same method, two runs each): defaults 2.02 frames/s at 0.30 % and 0.27 %; held stripes that keep scrolling 14.93 frames/s at 0.87 % and 1.05 %, about at the 1 % budget.
- The daemon's cost doesn't depend on these settings.

### Plan 2a: media widget

V1 with the media widget and the user's music playing. Release daemon, read-only on the user's session, rendered into a headless KWin. Load average 0.2–1.5:

| Setting | Shell + daemon | Shell | Daemon |
|---|---|---|---|
| `spectrum_fps = 15` (default), two runs | **1.90 %** each (budget ≤ 2 %) | 1.25–1.27 % | 0.63–0.65 % |
| `spectrum_fps = 30` | 2.43 % | 1.80 % | 0.63 % |
| Media placeholder instead | 0.90 % | 0.42 % | 0.48 % |

- **Frame rates:** 15 frames/s at the default, 30 at `spectrum_fps = 30`, and 0 while paused. On a pause the widget releases the spectrum at once.
- **The bars fall when playback pauses.** From full height they take 21 frames to go dark, about 1.4 s at 15 fps and 0.7 s at 30, and then the widget releases its clock hold: 0 clock frames and 0 rendered frames afterwards. Measured in a headless KWin with a fake player.
- **The spectrum is drawn by one shader** (`SegBars`), not ≈ 300 rectangles.
- **The daemon still analyses audio at 30 Hz** when the shell draws at 15. Feeding at 15 Hz would win back about 0.16 %.

### Plan 2a: block graphs drawn by one shader

`BlockGraph` now draws all of its cells with one shader instead of a Rectangle per cell. The values are passed as packed uniforms, so a new sample means one repack and one frame. Shell only, in a headless KWin (2560×1440) with `QT_SCALE_FACTOR=1.15` to match the M27Q. Fake daemon on a private bus, `measure-cpu.sh` for 60 s after a 15 s settle, before and after interleaved. 1-min load 0.3–2.4:

| | Before (Rectangles) | After (shader) |
|---|---|---|
| Default V1, still (alert held) | 0.70 / 0.72 / 0.63 %, RSS 220–223 MiB | 0.37 / 0.38 / 0.32 / 0.37 %, RSS 197–200 MiB |
| V1 frames | 2.00/s | 2.00/s |
| Stress layout (`layout-test.toml`) | 2.67 / 2.85 %, RSS 296 MiB | 0.27 / 0.30 %, RSS 197 MiB |
| v1 → v2 with half-width graphics and storage: first frame after the switch | +371 / +373 / +372 ms | +14 / +13 / +15 ms |
| Same switch: shell CPU in the 1.5 s after it | 461 / 464 / 460 ms | 28 / 30 / 31 ms |
| Same switch: RSS before → after | 222 → 515 MiB (three runs alike) | 197 → 202 MiB |
| Same switch: the 380 ms print | about 11 of its frames render; ends at +568–578 ms | all 24 steps render; ends at +392–403 ms |

- Six full-width lanes in a harness render 1.00 frames/s on a 1 Hz history at 0.17 %, against 1.85 % before. On a static series they render 0 frames/s.
- Wide graphs no longer cost about 300 MiB and most of a second to create, so V2 and MONITOR can carry them.
- A graph draws at most 384 columns and 264 rows. Past 384 columns it shows only the newest history. A slot taller than 264 rows draws the bottom 264 and leaves the top empty, with values scaled to the rows drawn. Each limit warns once.

### Plan 2a acceptance: the budget states (2026-10-21, HEAD `0bfec08`)

The installed binaries: `~/.local/bin/ghostd` with its INMET provider and Open-Meteo running for real, and the installed shell. Each state ran in a headless KWin (2560×1440, `QT_SCALE_FACTOR=1.15`) against a daemon of its own, so the user's desktop was never interrupted. 60 s per window from t = 26 s, frames counted with C21's settle. Load average 0.4–1.5. The live covered state was read from the user's own services:

| State | Shell + daemon | Frames/s | Shell RSS | Budget |
|---|---|---|---|---|
| Animating: V1 with the user's settings (600 s attention, 0.01 s step) | 1.59 %, 1.51 % | 15.92 | 244 MiB | ≤ 2 % ✓ |
| V1 held (default settings), for comparison | 0.93 %, 0.92 % | 2.99 | 244 MiB | — |
| Still: MONITOR, default settings, nothing playing | 0.76 %, 0.76 % | 2.95 | 228 MiB | ≤ 1 % ✓ |
| MONITOR with the user's settings | 1.33 %, 1.35 % | 15.93 | 228 MiB | — |
| Covered: `measure-covered.sh 60` | 0.05 % | 0 frames | — | 0 frames, ≤ 0.5 % ✓ |
| Covered: the live services | 0.07 % | — | 300 MiB | ≤ 0.5 % ✓ |
| Playing: V1 with Spotify playing, `spectrum_fps = 15` (installed `a507843`) | 1.83 %, 1.83 %, 1.90 % | 15.9 | 237–261 MiB | ≤ 2 % ✓ |
| Playing at the user's `spectrum_fps = 30` | 2.36 %, 2.43 % (2.61 % under load) | 27–28 | 238–240 MiB | recorded |

- The daemon reads 0.48–0.53 % of a core in every visible state and 0.00 % covered. The INMET provider reads 0.00 % at 7.5 MB.
- Playing passes with a thin margin. The spectrum adds about 0.8 % to the shell (0.43 % held, 1.23–1.27 % at 15 fps) and about 0.1 % to the daemon. At 30 fps the headless output drew 27–28 frames/s rather than 30.
- Playing used the shell on the real session bus, so it could see MPRIS, and its own daemon. Its only capture was the output monitor. The RSS in the Playing rows counts each page once, as the budget now does. The other rows' RSS is VmRSS.
- Held views render 3 frames/s rather than 2. The data tick, the stripe step and the clock's second don't always share a frame.
- The user's MONITOR animates because an INMET alert issued for later in the day shows in the weather widget, and the user's 10-minute attention window and scrolling stripes keep it moving.
- **RSS: within budget, counted once per page.** VmRSS counts the pages NVIDIA maps through `/dev/nvidiactl` twice: VmRSS minus the `smaps` total minus the `nvidiactl` share was a constant 164 KiB over 382 samples. Since then, the budget and `measure-cpu.sh` use `Rss` from `smaps_rollup`. Live at 19:50, the shell reads 250 540 KiB (VmRSS 306 144) and the daemon 37 888, so 288 428 KiB together: 94 % of 307 200.
- **No leak.** 30 cover and uncover cycles stay between 190 and 192 MiB headless. Switching views adds 8.8 MiB once, at the first switch to V2, then stays within ±1.5 MiB.
- **The live shell holds about 40 MiB more of NVIDIA's memory than any headless run:** twelve 4 MiB `nvidiactl` chunks against at most three, ten of them from one allocation. Three 2560×1440 buffers would be 42 MiB. The live and headless KWin offer the same Wayland globals apart from `wp_drm_lease_device_v1`, with three dmabuf tranches each. Not yet explained.
- **The media widget is the largest memory cost added in Plan 2a.** Album art costs about 21.6 MiB, of which Qt's image-format plugins are about 8.5 MiB, and the texture atlas about 60 MiB of VRAM. Playback costs about 10 MiB. PipeWire, the 600 s attention window and the extra views cost nothing measurable. The shell allocates with jemalloc, so `MALLOC_ARENA_MAX` changes nothing.

### Plan 2a: the shell's memory, album art and the texture atlas (HEAD `c851fae`)

Looking for savings in the media widget's album art, the largest memory cost Plan 2a added. The installed shell against `ghostd --fake`, in a headless KWin (2560×1440, `QT_SCALE_FACTOR=1.15`) with the service's environment (the NVIDIA drop-in's variables, a warm shader cache) and no PipeWire. V1 animating with the user's settings (600 s attention, 0.01 s step). Where a player is listed, a fake MPRIS player that is always playing, with a 640×480 JPEG `file:` cover; the spectrum draws at 15 fps. Each run: 26 s to settle, then 60 s with `measure-cpu.sh` and `QSG_RENDER_TIMING=1`. `Rss` is the shell's `smaps_rollup` figure at the end of the window, VRAM the shell's line in `nvidia-smi`. Runs of the settings were interleaved. Load average 0.3–1.5. Shell only; the fake daemon is 9 MiB and 0.02–0.08 %.

| State | Atlas | Rss (KiB), per run | VmRSS (KiB), per run | VRAM (MiB) | Shell CPU, per run | Frames/s |
|---|---|---|---|---|---|---|
| V1 animating, no player | Qt's default | 171 592, 171 652 | 180 604, 180 664 | 90 | 0.95, 0.98 % | 14.91 |
| | 1024×1024 | 175 704, 171 544 | 188 812, 180 556 | 90 | 0.95, 0.93 % | 14.91 |
| | **512×512** | 171 212, 171 276 | 180 224, 180 288 | 90 | 0.92, 0.95 % | 14.91 |
| V1 animating, playing with art | Qt's default | 199 124, 199 840, 200 208 | 220 424, 221 140, 220 996 | 156 | 1.37, 1.17, 1.20 % | 14.93, 14.93, 14.91 |
| | 1024×1024 | 194 656, 194 268, 194 944 | 211 348, 210 960, 212 148 | 100 | 1.20, 1.18, 1.17 % | 14.93, 14.91, 14.91 |
| | **512×512** | 190 080, 189 944 | 202 676, 202 540 | 92 | 1.23, 1.17 % | 14.91, 14.93 |
| The same, art changing every 3 s | Qt's default | 201 672, 201 768 | 222 460, 222 556 | 156 | 1.27, 1.25 % | 15.40, 15.41 |
| | 1024×1024 | 198 920, 197 604 | 216 124, 214 296 | 100 | 1.27, 1.25 % | 15.43, 15.45 |
| | **512×512** | 197 316, 198 412 | 214 520, 215 616 | 92 | 1.23, 1.25 % | 15.40, 15.38 |

- **Texture atlas: set to 512×512 in `ghost-shell.service`.** Qt sizes the atlas to the window, 4096×2048 here, and allocates it for the first small image, which is the cover at about 140 px (keeping the cover out of it with `QSG_ATLAS_SIZE_LIMIT=64` gave the same 92 MiB). With art showing it costs **64 MiB of VRAM** (156 to 92) and **9.5 MiB of Rss** while one cover plays (199 724 to 190 012 KiB, mean of the runs above), or 3.8 MiB while the art changes every 3 s; expect the live shell's 250 540 KiB to fall by 4–10 MiB. The Rss comes from the driver's `/dev/nvidiactl` pages: 21 136 to 12 432 KiB playing, 20 624 to 17 040 KiB rotating. 1024×1024, the investigation's value, saves 56 MiB of VRAM and 5.0 MiB (3.4 rotating); 512×512 was never worse. Without art no atlas is made, so nothing changes in the first rows. The run with 175 704 KiB had one extra 4 MiB `nvidiactl` chunk, which the driver adds at random: 12 944 against 8 848 KiB, with the same VRAM.
- **Nothing visible or measurable is lost.**
  - Frames: 895–896 in the 60 s of every playing run, 14.91–14.93 frames/s, at every size. CPU 1.17–1.37 % at the default against 1.17–1.23 % at 512×512; the 1.37 % run had a load average of 1.1–1.3, the others 0.3–1.0.
  - Rss and VRAM stay flat while the art changes every 3 s, about 28 changes in a run, within a one-chunk jitter.
  - Pixels, from `GHOST_SNAPSHOT` snapshots of the media panel with the player paused: in the mono icon style (the default) the cover's 255×255 square is identical at both sizes, 0 of 65 025 pixels. In the colour style 36 of them differ by 1/255 in one channel, probably the rounding of the atlas offset in sampling; two snapshots at the same size differ in none. Across the whole 580×440 panel 79 (mono) and 107 (colour) pixels differ by 1/255, against 77 between two runs at the same size. The dimmed art of a remembered track was not compared.
- **The atlas is set in the shell unit, not the NVIDIA drop-in:** it is Qt's, not NVIDIA's, and the unit installs on every machine. It takes effect at the next `scripts/install.sh`, daemon reload and shell restart. `measure-covered.sh` is unchanged: it counts frames and CPU, which the atlas does not change, and it does not carry the drop-in's variables either.
- **Qt's image-format plugins (about 8.5 MiB): no clean fix, left alone.** On the first `file:` or network image, Qt Quick's `QQuickPixmapReader` builds its backend table (`BackendSupport`), which calls `QImageReader::supportedImageFormats()`, and Qt 6 loads every `imageformats` plugin to answer that: all 24 KImageFormats `kimg_*` and Qt's own, with libheif, OpenEXR, x265, libjxl, libraw and libQt6Pdf behind them. It does this for a `.jpg`, a `.png`, a file without a suffix and an `http` URL alike (checked with a bare `Image`, and the call chain under gdb), so knowing that the art is JPEG or PNG does not help; Quickshell is not involved. With no art, none of them loads. `Image` has no format property, no Qt environment variable filters plugins (`QT_PLUGIN_PATH` only adds directories), and the image providers Quickshell offers take icon names, not `https` URLs. Only removing the plugins from the system or a private plugin tree through `qt.conf` would stop it, and neither is a change to this repo.

### Plan 2a: KWin desktop script

The script has no timers and does no polling. It evaluates the stacking order once per relevant window event and calls ghostd's `Report` only when the covered outputs or the focus state change. It also reads KWin's `showingDesktop` property once on load, once per new window and once per show-desktop change. On the live session, a 20 s window with no window events made 0 calls. `kwin_wayland` read 2.00 % of a core over those 20 s with the dashboard animating; that is KWin's own baseline, and no comparison without the script was taken.

### Plan 2b: shader mirror and heat graphs (HEAD `c480007`)

`MirrorGraph` and `HeatGraph` draw with shaders like `BlockGraph`. A heat lane is one ShaderEffect per band of at most 4 rows, fed by a ring buffer: a new sample writes one word and moves a `head` uniform. One ShaderEffect per lane held 512 words and cost +35.3 MiB of Rss against ≤ 30, because NVIDIA charges memory per program. Shell only, in a headless KWin (2560×1440, `QT_SCALE_FACTOR=1.15`), against a release `ghostd run --fake` on its own socket. The gallery's lane pages, a 320 s warm-up so the history is full, a 10 s settle, then `measure-cpu.sh` for 60 s. `Rss` is the shell's `smaps_rollup` figure at the end of the window. Three runs, interleaved, one warm-up run discarded. Load average 0.25–1.87.

| Page | CPU, per run | Rss (KiB), per run | Frames/s | Creation |
|---|---|---|---|---|
| Base (the kit page, no lanes) | 0.18, 0.18, 0.18 % | 196 576, 196 236, 196 448 | 1.00 | — |
| Mixed (two bar, two mirror, two heat lanes) | 0.42, 0.38, 0.38 % | 212 392, 213 248, 213 720 | 1.00 | 9 ms |

- Against the interleaved base, the mixed page costs **+0.21 %** and **+16.4 MiB** of Rss (the two earlier rounds of the design read +0.19 %). The target was ≤ 0.30 % and ≤ 30 MiB.
- Per lane, from earlier rounds on the same harness: a bar lane about 0.017 %, a mirror lane about 0.06 %, a 16-row heat lane about 0.03 % (it was 0.20 % with one effect per row). The heat program costs NVIDIA about +20 MiB once per process, so count it once, not per lane.
- Projection, not measured: a MONITOR with one heat lane, three bar lanes and two mirror lanes would add about 0.20 + 0.05 + 0.12 ≈ 0.37 % before labels, using the per-row heat cost of the first version. With the band design the heat share is smaller.

**Only the first shell start after any shader change runs about 61 MiB above steady state.** Same harness, the service's EGL variables, and the NVIDIA and Qt shader caches in directories of their own. Rss was read 2 s after the page existed, then at about 70 s:

| Session | Rss at start (KiB) | Rss at ≈ 70 s (KiB) |
|---|---|---|
| Base, cold caches | 228 908 | 225 824 |
| Base, second start (warm) | 165 752 | 164 340 |
| Mixed, cold caches | 234 728 | 228 328 |
| Mixed, second start (warm) | 169 228 | 167 188 |
| Mixed, NVIDIA cold only | 234 468 | 228 076 |
| Mixed, Qt cold only | 169 624 | 167 252 |

- A cold session does not decay: the mixed page reads 228.0 MiB at 300 s and 228.1 MiB at 600 s. The second start reads 167.2 MiB, **61.1 MiB lower**.
- NVIDIA's disk cache is the deciding layer. With only NVIDIA's cold the session reads cold, and with only Qt's cold it reads warm.
- The cost belongs to any shader change, not to these graphs: the base page pays +61.5 MiB too (225.8 against 164.3). The lanes add +2.5 MiB cold and +2.8 MiB warm over it.
- **Reinstalls therefore restart the shell once, to warm the caches.** The service's `nvidia.conf` drop-in sets `__GL_SHADER_DISK_CACHE_PATH` and `CacheDirectory=ghost-shell`, so the driver's cache directory exists.
- Load average over these runs was 0.57–5.99.

### Plan 2b: the uplink globe as one shader (spike, not merged)

A globe spike before the widget is written; details in `docs/superpowers/notes/2026-10-21-globe-shader-spike.md`. One ShaderEffect with a placeholder 2048×1024 land mask, 3 200 dots and 8 arcs. Headless KWin (2560×1440, `QT_SCALE_FACTOR=1.15`), a release fake daemon, `measure-cpu.sh` for 60 s after a 15 s settle, two passes of interleaved rounds, load average 0.1–2.4. The renderer was checked rather than assumed: the shell drew on the RTX 4080 SUPER through NVIDIA's EGL (`OpenGL ES 3.2 NVIDIA 615.78.08`).

| Mode (V2 centre, 608×330 design px) | CPU per run | Rss | Frames/s |
|---|---|---|---|
| Empty (clock held, no globe) | 0.12, 0.08, 0.07, 0.08, 0.08 % | 122–123 MiB | 0 |
| Dots only | 0.77, 0.55, 0.55, 0.58, 0.57 % | 155–156 MiB | 14.93 |
| Dots and arcs in the shader | 0.77, 0.57, 0.55, 0.58, 0.53 % | 155–156 MiB | 14.93 |
| Arcs as a `Shape` (CurveRenderer) | 1.90, 1.80, 1.77, 1.72, 1.82 % | 160 MiB | **29.87** |

- **The arcs in the shader cost nothing beyond the frames**, about 0.5 % over empty at 14.93 frames/s (about 0.57 % in total). The V1 scope size (276×146) reads 0.55–0.57 %.
- **A `Shape` draws two frames per tick**: Qt 6.11 renders a second frame after every path change, 1–13 ms after the first. That costs about 1.8 % at both sizes and breaks the 15 frames/s ceiling by itself.
- **The mask is the memory.** The globe is about 33 MiB over empty with the 2048×1024 mask, 21 MiB with 1024×512 and 15 MiB with 512×256 (one run each); with no mask it is +3 MiB. Any image costs about 10 MiB.
- Projection, not measured: with the live shell at about 245 MiB, a 2048×1024 mask would bring it to about 278 MiB and a 512×256 one to about 260 MiB, against the 300 MB budget.
- Creating the globe took 23–28 ms (a synchronous `Loader`, including the PNG decode) and its first frame 6–7 ms, with warm caches. With cold ones, the first frame took 97–134 ms for the globe shaders, and the process kept 60–70 MiB.
- The labels' per-tick projection of 9 points in JavaScript was not measured.

### Plan 2b: process list and tree (daemon)

The process scan moved from `mem` to its own source, with topics `procs` and `procs.tree`. Release builds, this machine with the desktop running (load average 0.2–3.1). The scan wall time comes from `Instant` and its CPU from `CLOCK_THREAD_CPUTIME_ID`; the old and new collectors ran side by side, one sample a second, 121 samples. Medians:

| Sample | Wall | CPU |
|---|---|---|
| Old scan (`mem` with the top list), n = 61 | 3.857 ms | 3.837 ms |
| New scan, no tree, n = 48 | 3.873 ms | 3.858 ms |
| New scan with the tree, n = 13 | 3.939 ms | 3.923 ms |
| Repeat between scans, old / new | 0.025 / 0.027 ms | 0.025 / 0.027 ms |

- The split costs nothing measurable, and the tree adds about 0.07 ms once every 10 s.
- Plan 2a's 2.56 ms scan (above) no longer describes this machine: it now has about 410 user processes and 485 kernel threads, against about 340 user processes then.
- The tree is 28 738–30 940 B of JSON, typically about 30.0 KB, for 389–416 processes. It is published every 10 s, so about 3 KB/s on the socket while wanted. A `procs` message is 398–415 B.
- Daemon CPU and Rss by demand (`measure-cpu.sh` for 60 s after a 6 s settle, two interleaved rounds, a client subscribed):

| Demand | Run 1 | Run 2 |
|---|---|---|
| Old `[mem]` | 0.20 % / 33 968 KiB | 0.15 % / 33 816 KiB |
| New `[mem, procs]` (the memory widget) | 0.17 % / 33 988 KiB | 0.17 % / 33 984 KiB |
| New `[procs]` | 0.18 % / 33 936 KiB | 0.18 % / 34 080 KiB |
| New `[procs, procs.tree]` | 0.18 % / 34 288 KiB | 0.18 % / 34 184 KiB |

- The memory widget's change is about 0.00 % CPU and about +100 KiB of Rss. The tree adds about 0.00 % and +200 to 350 KiB while wanted.
- On V1 in a headless KWin against `--fake`, the shell read 0.35 and 0.32 % (new) against 0.35 and 0.33 % (old). The Rss difference was inside the ±5 MiB run-to-run noise.

### Plan 2b: History for every sampled topic

The daemon now keeps a history for 26 series (3 cpu, 16 cores, 2 gpu, 1 mem, 1 sensors, 3 disks) instead of 4. Release builds, a 5-minute `history_minutes` at 1 s ticks, every topic subscribed. After 310 s a probe subscribed and recorded the History line.

| | Series | Samples each | History line |
|---|---|---|---|
| Before, `--fake` / real collectors | 4 | 300 | 8 850 B / 10 090 B |
| After, `--fake` | 26 | 300 | 52 668 B |
| After, real collectors, two rounds | 26 | 300 | 53 031 B, 56 315 B |
| After, `--fake`, at 60 minutes (`history_minutes = 3` at a 50 ms tick) | 26 | 3 600 | 628 706 B |
| After, real collectors, at 60 minutes | 26 | 3 600 | 457 887 B |

- The 5-minute line is about 21 % of `MAX_UNSENT_BYTES` (256 KiB). The 60-minute line exceeds it, so a History reply is now admitted against the bytes queued before it. A client that does not drain the reply by the next tick is dropped. The probe drained at 20 Hz ticks and stayed connected; a live shell at 60 minutes was not run.
- **Shell cost of one resend** (`JSON.parse` and `fromHistory` in a QV4 micro-benchmark, tight loop, so later rounds include GC): 0.465, 1.450, 1.215 ms at 5 minutes against 0.070, 0.210, 0.370 ms before. At 60 minutes: 5.9, 15.6, 24.2 ms (fake line) and 7.3, 19.8, 25.6 ms (real line).
- **Shell cost per tick** (the same benchmark, appending one message for each of the five topics): 1.734, 1.643, 1.777 ms at 300 samples against 0.288, 0.286, 0.288 ms before. At 3 600 samples: 14.8, 32.9, 51 ms. The append copies every series on every tick.
  - Projection from the benchmark, not measured end to end: at `history_minutes = 60` and 1 Hz that is roughly 1.5–5 % of a core, over the 2 % budget on its own.
  - End to end at the default 5 minutes (headless KWin, `--fake`, 340 s settle, two rounds with the trees swapped between KWins), the shell read 0.38 and 0.43 % (new) against 0.33 and 0.42 % (old), a mean of +0.03 points. Rss differed by −304 and +728 KiB.
- **Daemon:** Rss grew by about 0.5–0.65 MiB (real collectors: 35 956 and 35 796 KiB against 35 244 KiB; `--fake`: 9 540 against 8 912 KiB). CPU did not change (real: 0.38 against 0.38 %). At 3 600 samples the daemon holds 12 212 KiB (`--fake`) and 38 620 KiB (real).

### Plan 2b: network links (`net.links`)

The `net.links` feed reads NetworkManager over the system bus, asks Tailscale for its status, and pings 1.1.1.1 from an unprivileged ICMP datagram socket. Release build, a throwaway harness on a current-thread runtime. CPU from `CLOCK_PROCESS_CPUTIME_ID`, wall from `Instant`. These figures predate the change of the ping socket to tokio's UDP socket.

| Operation | n | CPU median / mean / max | Wall median / mean / max |
|---|---|---|---|
| NetworkManager full re-read (system bus) | 50 | 476 / 503 / 755 µs | 1.79 / 1.87 / 2.58 ms |
| Tailscale status request | 30 | 109 / 110 / 291 µs | 0.44 / 0.42 / 0.61 ms |
| Ping 1.1.1.1 | 10 | 121 / 145 / 405 µs | 1.96 / 1.99 / 2.66 ms |

- Projection from these figures: 6 Tailscale requests and 12 pings a minute come to about 2.1 ms of CPU a minute, about 0.0035 % of one core. NetworkManager is re-read only on its events.
- Daemon over 60 s after a 20 s warm-up, with only `net.links` subscribed. `measure-cpu.sh` reads 0.00 % either way, because it counts 10 ms clock ticks. The summed per-thread `schedstat` gives 3.72 and 3.64 ms per 60 s (0.0061–0.0062 %) with no client and 5.20 ms (**0.0087 %**) with `net.links` wanted: about **+0.0025 %**.
- Rss rose by about **+650–700 KiB** (34 004 KiB before the first want, 34 688–34 700 KiB afterwards): the system-bus connection, zbus state and the lane tasks. The connection stays open after the first want.
- A live check on this machine returned `ping_ms` of 1.95–2.36 ms at the 5 s cadence.

### Plan 2b: the live install (2026-10-22 01:59, HEAD `f202ade`)

After the reinstall the shell was restarted once to warm the shader caches. The live services' `smaps_rollup` Rss afterwards was **194 964 KiB** for the shell and **34 536 KiB** for `ghostd`. The desktop was covered by a maximised window at the time.

## License

Copyright (C) 2026 Gabriel Mitelman Tkacz

GPL-3.0-or-later; see [LICENSE](LICENSE).

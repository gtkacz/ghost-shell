# ghost-shell

A Tron: Legacy / Cyberpunk 2077 / Ghost in the Shell desktop shell for KDE Plasma (Wayland): a Quickshell UI fed by a Rust daemon, `ghostd`.

## Build

Requires a recent stable Rust toolchain, Quickshell (`qs`, Qt 6) and `qsb-qt6` for the shaders.

```sh
cargo build --release
```

The daemon is `target/release/ghostd`. The shell's shaders are compiled with `make shaders`.

## Test

Requires Node for the shell's logic tests.

```sh
cargo test --workspace
node --test 'shell/tests/*.test.mjs'
```

## Install

```sh
scripts/install.sh
```

The script is idempotent and installs for the current user only:

- `ghostd` into `~/.local/bin/`;
- `shell/` (with its compiled shaders) and `widgets/` (without `broken-example`) into `~/.local/share/ghost-shell/`;
- the Kode Mono font into `~/.local/share/fonts/ghost-shell/`, if it is not installed yet;
- the user units `ghostd.service` and `ghost-shell.service` into `~/.config/systemd/user/`.

It stops with an error if `cargo`, `qsb-qt6`, `shell/` or the compiled shaders are missing. It does not enable or start anything, and it does not touch Plasma.

## Run

```sh
systemctl --user start ghostd ghost-shell
systemctl --user stop ghost-shell ghostd
```

Neither unit is enabled at login.

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
| Plus kernel threads skipped after first sight (current) | **2.56 / 2.51 ms** | 1.14 / 1.15 ms | 0.90 / 0.91 ms |

All figures are medians. The current version costs **≈ 0.25 % of a core** at the real 1 Hz cadence. Most of what remains is the kernel generating `stat` lines for about 340 user processes from cold caches.

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

- At 1 Hz a read costs **≈ 0.08 % of a core**.
- About 0.61 ms of each read is `fan_speed`. Reading the fan less often is the obvious lever if that's ever needed.
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
- **Budget.** On a quiet machine the daemon alone uses about half of the 1 % "nothing animating" budget. On a busy one it can use all of it. If the acceptance run confirms this, the next cuts are:
  1. refresh the top-process list every 2 samples;
  2. read the GPU fan every 5 samples;
  3. ~~cache the per-CPU frequency reads~~ — measured below: ≤ 0.01 % to gain, not worth doing.
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

#### Not yet measured

- The shell with real panels, and the shell and daemon together. The acceptance run at the end of plan 1 measures the combined total against the budgets above.

## License

Copyright (C) 2026 Gabriel Mitelman Tkacz

GPL-3.0-or-later; see [LICENSE](LICENSE).

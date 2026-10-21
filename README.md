# ghost-shell

A Tron: Legacy / Cyberpunk 2077 / Ghost in the Shell desktop shell for KDE Plasma (Wayland): a Quickshell UI fed by a Rust daemon, `ghostd`.

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

22 temperatures and 2 fans, first version (every chip read every sample):

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
- **Fix in progress: read Super-I/O chips every 5 samples.** The projected cost of the chip drops to ≈ 1.1 ms of CPU per second (≈ 0.11 %). *Not yet measured.*
- The NVMe and i2c reads block for milliseconds but use little CPU. The daemon therefore runs its collectors off its event loop.

#### Not yet measured

- CPU (`collect/cpu.rs`): `/proc/stat` plus 32 `scaling_cur_freq` reads per sample.
- GPU (NVML), drives, and the daemon and shell as a whole. The acceptance run at the end of plan 1 measures the combined total against the budgets above.

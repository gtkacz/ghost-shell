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

When an install changes a compiled shader, the script says so at the end. The shell's first start after that compiles the shaders into its cache and stays about 61 MiB above its steady state until it is restarted (see Performance). So start or restart `ghost-shell`, wait about 30 s, and run `systemctl --user restart ghost-shell` once.

### Retiring the Plasma panel

The bar and the launcher replace Plasma's panel and Kickoff, while `plasmashell` keeps running for the clipboard, the OSDs, notifications and KDE's applet popups. `scripts/install.sh` only prints this step; run it yourself, once, from the Plasma session, with `ghostd` and `ghost-shell` running:

```sh
scripts/plasma-panel.sh retire    # hand the panel over to the bar
scripts/plasma-panel.sh status    # two lines: "retired <stamp>" or "plasma panel in place", then "notifications retired <stamp>" or "plasma serving notifications"
scripts/plasma-panel.sh restore   # give it back
```

`retire` prints every change before making it. Before the first change it checks that `plasma-plasmashell.service` runs the live plasmashell and that none of the seven popup keys below is taken; if either fails, it refuses and names the problem. In order, it then:

1. backs up `plasma-org.kde.plasma.desktop-appletsrc`, `plasmashellrc` and `kglobalshortcutsrc` from `~/.config`, and the keys of plasmashell's "activate application launcher" and "activate task manager entry 1…10" shortcuts, into `~/.local/state/ghost-shell/plasma-backup/<UTC stamp>/` with a `manifest`, and marks the panel retired;
2. removes every Plasma panel, and puts a System Tray and a Digital Clock at the right end of the bar's edge, under the bar on the bar's screen (the top or the bottom edge, following `bar.position`), with the panel's tray and clock settings. Their position only decides where KDE's popups open;
3. restarts plasmashell once (`systemctl --user stop|start plasma-plasmashell.service`), giving the tray, the clock and the tray's network, Bluetooth, volume, KDE Connect and clipboard applets a global shortcut each (Meta+Ctrl+Alt+Shift+F1…F7), so that ghostd can open their popups, and writes which widget is which to `~/.local/state/ghost-shell/plasma-popups`;
4. through KGlobalAccel, takes bare Meta off "activate application launcher" (Alt+F1 and its other keys stay) and clears the keys of "activate task manager entry 1…10", which frees Meta and Meta+1…9 for ghost-shell. Meta+0 is KWin's "Zoom to actual size" and is left alone;
5. reconfigures KWin.

It refuses to run again until `restore` has run. If it fails or is interrupted part way, it starts plasmashell again if it had stopped it and prints the `restore` command, which undoes what it had already changed.

`restore` stops plasmashell, removes the popups' shortcuts and gives the 11 shortcuts their backed-up keys through KGlobalAccel, puts the three files back byte for byte, deletes `plasma-popups`, starts plasmashell and reconfigures KWin. The backups stay in `plasma-backup/`. The script reads `XDG_CONFIG_HOME`, `XDG_STATE_HOME` and `DBUS_SESSION_BUS_ADDRESS`, so `spikes/plasma-hidden/test-plasma-panel.sh` can run it against a sandboxed Plasma; that test checks the restore is byte-identical.

#### Handing the notification server to ghostd

```sh
scripts/plasma-panel.sh retire-notifications    # ghostd serves org.freedesktop.Notifications
scripts/plasma-panel.sh restore-notifications   # plasmashell serves it again
```

`retire-notifications` is separate from `retire` and can run with or without it. It prints every change before making it, and refuses if plasmashell is not run by `plasma-plasmashell.service` or if another component than ghost-shell holds `shortcuts.notifications` (default `Meta+N`; a value that is not `Meta+<letter>` is not checked, with a warning). In order, it:

1. backs up `plasmanotifyrc` (or notes that it is absent) into `~/.local/state/ghost-shell/plasma-backup/<UTC stamp>/` with a `notifications-manifest`, and writes `~/.local/state/ghost-shell/notifications-retired`;
2. writes the marker `~/.local/state/ghost-shell/notifications-server`, which makes ghostd request the name when it starts;
3. installs the drop-in `~/.config/systemd/user/plasma-plasmashell.service.d/ghost-shell-notifications.conf` (`After=ghostd.service`, `Wants=ghostd.service`, from `packaging/systemd/plasma-plasmashell-after-ghostd.conf`) so that ghostd holds the name from login, and runs `systemctl --user daemon-reload`;
4. stops plasmashell, sets `PopupPosition` in `plasmanotifyrc` to the corner away from the bar (`BottomRight` under a top bar, `TopRight` under a bottom bar), for the job popups Plasma still draws, restarts ghostd, then starts plasmashell;
5. waits until the name's owner is ghostd's main pid, and prints it.

`restore-notifications` removes the marker and the drop-in (and the drop-in's directory if it is then empty), reloads systemd, stops plasmashell, puts `plasmanotifyrc` back byte for byte (or deletes it if it did not exist), restarts ghostd only if it is running, starts plasmashell, checks that plasmashell owns the name, and removes `notifications-retired`. It works with ghostd stopped, and undoes everything `retire-notifications` did. As with `retire`, a failure part way starts plasmashell again and prints the restore command.

While nobody owns the name (ghostd crashed after the retire and has not been restarted by systemd yet), an app's notification call can stall for about 25 s and then fail: Plasma's activatable service file for the name starts a process that waits for it and never takes it. `ghostd.service` is `Type=notify` with a 15 s start timeout so that plasmashell starts only once ghostd has settled the name. The environment variables `GHOST_SYSTEMD_RELOAD`, `GHOST_GHOSTD_RESTART`, `GHOST_GHOSTD_TRY_RESTART` and `GHOST_GHOSTD_PID_CHECK` each name one executable that replaces the matching `systemctl --user` call, and `spikes/plasma-hidden/test-plasma-notifications.sh` uses them to run both subcommands in a sandboxed Plasma with a dev ghostd.

### Terminal theme (optional)

```sh
make install-term
```

`scripts/install-term.sh` themes Ghostty, Starship, `eza` and `fast-syntax-highlighting` from the shell's palette, which `scripts/install.sh` never does. It first backs up `~/.zshrc` and the existing terminal configs into `~/.config/term-backup-*.tar.gz`, checks the checksum, and stops without changing anything if that fails. Then it writes the configs under `~/.config/` and adds one marked block to `~/.zshrc`. It needs `uv`, Ghostty and Starship, and it is idempotent. `--dry-run` lists what it would write. The colours come from `crates/ghostd/src/config/default.toml` and `term/palette.toml`, and the palette is checked under a protan and deutan colour-vision simulation before anything is written. Run its tests with `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py`.

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

### Plan 2b: the kit globe (merged, shader)

`Globe` is the spike's design with the real Natural Earth land mask and land bits. Shell only, headless KWin (2560×1440, `QT_SCALE_FACTOR=1.15`), a fake release daemon, a 320 s history warm-up, a 15 s settle, then `measure-cpu.sh` for 60 s. Empty, V2 centre, V1 scope and a six-lane page, interleaved, one warm-up round discarded, three measured rounds. Load average 0.1–1.7, with other agents running. The renderer was the RTX 4080 SUPER through NVIDIA's EGL. The driver's shader cache was a directory made for the run.

| Mode | CPU, per run | Median | Rss | Over empty | Frames/s |
|---|---|---|---|---|---|
| Empty | 0.20, 0.17, 0.18 % | 0.18 % | 122.8–123.5 MiB | — | 0 |
| V2 centre (608×330) | 0.70, 0.78, 0.65 % | 0.70 % | 126.3–126.4 MiB | +3.0 MiB | 14.92 |
| V1 scope (276×146) | 0.85, 1.03, 0.63 % | 0.85 % | 126.4–126.6 MiB | +3.1 MiB | 14.92 |

- **Against the spike:** V2 reads 0.70 % against the spike's 0.57 % (0.53–0.77). This session's floor was higher (empty 0.18 against 0.08), so the fair comparison is the cost over empty: +0.52 points against +0.49. The globe's own cost did not change. The 1.03 % V1 run coincided with a load average of 1.74.
- **Memory:** +3.0 MiB against the spike's +33 MiB, because the shader reads 34 words of land bits (1 423 B) instead of sampling a 2048×1024 mask. VRAM read 24 MiB against 36 MiB for the spike. The stop rule (V2 ≤ 0.77 %, Rss ≤ +5 MiB) passed.
- **Creation:** 1–2 ms against the spike's 23–28 ms (no PNG decode). The cold first frame took 68 ms; the warm one 2 ms.
- **Fix round (exact arithmetic for the land-bit lookup):** one measured round after one discarded round gave V2 0.78 %, V1 0.80 % and empty 0.17 %, with +3.2 and +3.1 MiB over empty. V2 sits inside the first round's 0.65–0.78 %. A single round, so it shows that nothing moved, not a new figure.
- Output against a brute-force reference at a frozen frame: 2 pixels differ by more than 2/255 at V2 (largest 11/255) and 19 at V1 (largest 10/255), all at the rim. The fix round's grabs are pixel-identical to the first round's.

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

### Plan 2b: network rates (`net`)

The `net` topic reads `/proc/net/dev` and `/proc/net/route` once a second, only while it is wanted. Release build, a throwaway harness, load average 0.2–1.3, desktop running. CPU from `CLOCK_THREAD_CPUTIME_ID`, wall from `Instant`.

| Spacing | n | Wall median / mean / max | CPU median / mean / max |
|---|---|---|---|
| 1 s, round 1 | 60 | 165.7 / 175.3 / 276.8 µs | 164.9 / 174.6 / 254.8 µs |
| 1 s, round 2 | 60 | 141.1 / 139.9 / 220.4 µs | 140.8 / 140.1 / 219.4 µs |
| 10 ms (warm) | 1000 | 72.1 / 74.8 / 144.0 µs | 72.3 / 74.8 / 131.0 µs |

- At 1 Hz that is about 0.14–0.17 ms of CPU a second, about 0.015 % of one core. Wall time is roughly equal to CPU time, so the cost is on-CPU work.
- Daemon over 60 s after a 20 s warm-up, two rounds on fresh daemons, no providers. `measure-cpu.sh` reads 0.02 % idle and 0.03 % with `net` wanted. The summed per-thread `schedstat` gives 4.0 and 4.7 ms per 60 s idle and 19.3 and 17.9 ms with `net` wanted: about **+0.022–0.025 %**. About 9 ms of that is the collector; the rest is the tick task, the history push, the JSON and the socket write.
- Rss rose by **+96 KiB** in both rounds (34 384 → 34 480 and 34 224 → 34 320 KiB). A message is about 285–290 B.

### Plan 2b: uplink feed (daemon)

The `uplink` feed lists the machine's remote endpoints, with GeoIP, ASN labels and, for endpoints with a process override, the owning process found by walking `/proc/<pid>/fd`. Release build, its own socket, no session bus, a temporary config with no labels. Per-cycle times come from an instrumented build that was not committed (`CLOCK_THREAD_CPUTIME_ID` around the cycle); Rss and CPU come from the clean release binary.

| Measure | Value |
|---|---|
| Cycle, no process override (13 cycles) | wall 1.21–2.35 ms, median about 1.40 ms; CPU about equal |
| First cycle with a process override (walks the fds, 4 runs) | wall 12.0–13.4 ms |
| Later cycles with an override (cache hits) | wall 1.24–1.68 ms |
| fd walk | 10.6–11.8 ms, 5 187–5 198 fds read, of about 6 087 held by this uid in 720 processes |
| Daemon CPU (`measure-cpu.sh`, 60 s) | 0.02 % unwanted, 0.03 % wanted |

- The walk takes about 11 ms, so the cycle runs in `spawn_blocking`; on the daemon's single-threaded runtime it would stall every other task. Unfound sockets are cached, so the walk ran once in about 60 s.
- **GeoIP memory.** The two databases are memory-mapped (DB-IP City Lite 126 998 165 B, ASN Lite 9 566 363 B). Before the fix the mappings stayed after the topic was released:

| Moment | Rss (`smaps_rollup`) |
|---|---|
| Before the first want | 34 472 KiB |
| 30 s wanted | 40 312 KiB |
| 10 min wanted | 42 284 KiB (**+7 812 KiB**) |
| 60 s after release | 42 284 KiB |

- After the unmap-on-release fix (one fresh daemon, a 60 s tap): **34 276 → 40 984 → 34 536 KiB** (before the want, about 55 s into the want, 10 s after release). While wanted, the city file held 4 088 kB resident and the ASN file 2 368 kB, with the GeoLite2 fallback not mapped. After release there were no mmdb mappings, and the cost was +260 KiB over the baseline. While the globe is shown the cost is still about 6.5 MiB, plus about 1.4 MiB more over 10 minutes (the earlier run); the 10-minute figure was not repeated after the fix.

### Plan 2b: disk health, batteries, CPU package power and spectrum retry (daemon)

Release builds on this machine. Wall time from `Instant`; on-CPU time from `/proc/thread-self/schedstat` (current-thread runtime) for the two D-Bus feeds, and from 20 000 iterations of a bench file that was not committed for the RAPL reads.

| Feed | Operation | Wall | On-CPU | Cadence |
|---|---|---|---|---|
| `disks.health` | One refresh: `GetManagedObjects` and 3 × `SmartGetAttributes` (5 drives, 7 reads) | 2.8–3.6 ms, median 3.1 ms | 1.10–1.37 ms, median 1.13 ms | 600 s while wanted, plus one read per wanted period |
| `batteries` | KDE Connect poll: `devices(bb)` and 5 `Properties.Get` (n = 12) | 460–943 µs, median 720 µs | 93–246 µs, median 194 µs | 60 s while wanted |
| `batteries` | UPower read, 0 devices present (n = 5) | 206–432 µs | 40–102 µs | per wanted period and per signal |
| `cpu` | `energy_uj` read while unreadable (EACCES) | 1.0 µs | | each sample |
| `cpu` | Readable powercap file (a 0444 file as a proxy for the udev rule) | 1.8 µs | | each sample |
| `cpu` | DRM scan for the iGPU | 3.8 µs | | each sample |
| `cpu` | Whole `CpuCollector::sample()`, 32 cpufreq reads and `/proc/stat` included | 81.7 µs | | 1 Hz |

- `disks.health` at 600 s is about 1.1 ms per 600 s, about 0.0002 % of a core. `measure-cpu.sh` over 60 s read 0.02 % with nothing wanted and 0.00 % with it wanted, both below its resolution. Daemon Rss: 34 200 KiB before the first want, 36 276 KiB after the first read, 36 312 KiB after 8 reads. The **+2 076 KiB** is almost all the first system-bus connection (zbus) and the first `GetManagedObjects`; `net.links` also connects to the system bus, and the shared cost of the two together was not measured.
- `batteries` at 60 s is about 0.2 ms per 60 s, about 0.0003 %. `measure-cpu.sh` read 0.00 % unwanted and 0.02 % wanted (other feeds share the process). Rss: 34 564 KiB before the first want, 34 724 KiB after the first 190 s wanted period, 34 748 KiB at the end (+160 KiB, then +24 KiB more). The cost of a present UPower device (6 `Properties.Get`) was not measured live, because the DualSense was disconnected; the controller's cost is a projection: comparable to KDE Connect's 5 gets, so under about 1 ms.
- The `cpu` additions make the whole topic about 0.008 % of one core at 1 Hz, below what `measure-cpu.sh` resolves, so it was not run. The RAPL reading itself was untested until the udev rule was installed; see the live install below.
- **Spectrum retry.** With PipeWire restarted on the live session (one approved restart, a dev daemon with its own socket), the capture reconnected about **1.0 s** after PipeWire disconnected it (disconnect at 07:00:17.34, the retried capture's first dark frame at 07:00:18.446, 100 ms after it started). The first lit frame after the restart was not observed, because the user's Spotify did not survive the restart and no audio flowed. Recovery of the bars is therefore shown to the reconnect, not past it.

### Plan 2b: widgets and the budget states

All headless KWin at 2560×1440, `QT_SCALE_FACTOR=1.15`, a real release `ghostd` (not `--fake`) on its own socket with no session bus, a 15 s settle, then `measure-cpu.sh` on the shell and its daemon for 60 s, frames from `QSG_RENDER_TIMING`, one warm-up run discarded, base and variant interleaved. `Rss` is `smaps_rollup` at the end of the window. The daemon's share is stated in each case. These are the combined figures of the budget table (the shell and the daemon, averaged over 60 s).

| Page | Base, combined | With the widget, combined | Delta | Load | Budget |
|---|---|---|---|---|---|
| MONITOR, six telemetry lanes (median of 5 kept runs, range 0.98–1.13 %, one 1.77 % outlier) | 0.65 % | **1.06 %** | +0.41 | 0.08–3.69 | 1.00 % |
| MONITOR, process tree (mean of 3 pairs) | 0.69 % | 0.84 % | +0.15 | not logged | 1.00 % |
| V2, network panel (median of 3 kept pairs) | 0.75 % | 0.93 % | +0.18 | 0.06–0.34 | 2.00 % |
| V2, silicon (mean of 5 runs, range 1.86–2.17 %, three base runs) | 0.74 % | **2.02 %** | +1.28 | not logged | 2.00 % |

**Telemetry lanes on MONITOR.** The shell reads 0.61 % against 0.28 % for the base (+0.33), the daemon 0.45 % against 0.35 % (+0.10), and the shell Rss +10.6 to +15.4 MiB (median about +13 MiB) in most runs, with two higher runs at +23 and +51 MiB (NVIDIA's random 4 MiB chunks). The daemon's rise is the `net` sampler and the `net.links` feed, which the lanes subscribe to. Daemon Rss +1.0 MiB. Creation took 67–80 ms against 58–62 ms (+8 to +20 ms). Frames were 2.50–2.70/s against 1.75–2.00/s. One lanes run read 1.30 % shell at 6.23 frames/s and was not explained; it did not recur in six later runs.
- **Outcome: 1.06 % against the 1.00 % target.** The user accepted MONITOR up to 1.25 % on 2026-10-22, so this is inside the accepted band, and still over the design target.
- Projection, not measured: adding the lanes' delta to Plan 2a's base of 0.76 % gives about 1.17 %.

**Process tree on MONITOR.** Shell 0.26 → 0.38 % (+0.12), daemon 0.43 → 0.46 % (+0.03). Shell Rss **+14.3 MiB** (193 239 → 207 920 KiB mean, range +11.2 to +16.1), daemon +0.3 MiB. A rebuild per `procs.tree` message takes 0.26 ms for 40 processes (fake) and 2.12 ms for 321 (real), about 0.02 % of a core at one message every 10 s.
- **Outcome: the CPU is within budget, and the Rss is flagged.** +14.3 MiB is past the +10 MiB flag and uses most of the 18 772 KiB left for all of Plan 2b. The absolute shell Rss stayed at 205–210 MB in this rig, under 300 MiB.
- Edges are one `PathSvg` inside a `Shape`, not one `PathQuad` per edge. Interleaved, two runs each: one `PathQuad` per edge read 0.58 and 0.63 % shell and about +15 MiB, the single path 0.40 and 0.40 % and about +13 MiB (an earlier per-edge A/B read +17 to +23 MiB). About 6 MiB of the remaining Rss is the CurveRenderer's stroke geometry. The default renderer saves it, but leaves the 0.6 px edges aliased.
- The lanes and the tree were not measured together for the combined figure. With both on MONITOR, the shell alone read 0.72 % and the daemon 0.32–0.40 % in the empty-frame fix runs below.

**Network panel on V2.** Shell 0.33 → 0.50 % (+0.17), daemon 0.42 → 0.45 % (+0.02, noisy), frames 2.07 → 2.68–2.88/s. Shell Rss +1.4 MiB (range +0.8 to +6.3). Daemon Rss about +0.6 MiB. The scope style measures **109 design px** tall (also 108.65 at 1.739), so the V1 column has 179 left for the tabs.

**Silicon on V2.** Shell 0.39 → 1.68 % (+1.29), daemon unchanged at about 0.35 %, frames 2.2 → 14.9/s. Shell Rss +8.4 MiB (per pair +2.5 to +11.2).
- **Outcome: 2.02 % against 2.00 %, just over; flagged for Task 19.** The cost is the 15 frames/s loop over the whole window: with only the two packets drawn the shell already reads 1.38 %, about 1.0 of the 1.28 points. Drawing the tiles' 16 shaders adds about 0.1–0.3. Repacking the tile graphs on each `cpu` tick is not measurable: the frozen-graph runs (1.63 %) read the same as the live ones within noise. The one lever is the animation's frame rate, which is a design decision.
- The diagnostic variants are single runs and one read an unexplained 282 MB of shell Rss; it was not chased.
- The narrow form's later fix was not re-measured, because it adds no per-frame work.

### Plan 2b: the empty second frame (rows and the alert layer)

Task 7 and Task 8 saw about 0.7 extra frames a second. They are not a second publish: each daemon tick reaches the shell in one read and one `publish()`. They come from Qt's basic render loop (the headless KWin's) drawing an empty frame after a `Row` re-lays out a child whose size changed (0–2 ms later, no dirty nodes), and, on MONITOR, from the alert box's content layer re-rendering each time its countdown changed.

Two commits on a branch of their own: `InlineRow` (a kit item that places its children from a binding, replacing `Row` in the network and telemetry-lane widgets and in `Digits`' proportional cells), and `AlertBox`'s layer, now enabled only while the box holds the FrameClock. Real release daemon, no session bus, 20 s settle, 60 s window, four interleaved rounds with the first discarded, medians of rounds 2–4:

| View | Frames/s, before → after | Shell CPU, before → after | Shell Rss (KiB), before → after |
|---|---|---|---|
| V2 with the network panel | 2.75 → **2.00** (2.70–2.77 → 2.00–2.03) | 0.45 → 0.42 % (**−0.03**) | 201 928 → 205 820 |
| MONITOR (six lanes and the tree) | 3.58 → **2.20** (3.57–3.72 → 2.15–2.20) | 0.73 → 0.72 % (**−0.01**) | 224 072 → 224 824 |

- The frame count fell by 27 and 39 %, but the CPU saving of 0.01–0.03 points is inside run noise: the empty frames rendered in 0 ms. Task 7's estimate was 0.05–0.1 %. The daemon did not change (0.32–0.40 % in both arms), and the Rss changes are inside the ±4 MiB noise.
- These runs ran `qs` without a session bus and without a warm history client, so their absolute figures are not comparable with the tables above. Base and fix share the setup.
- All of this is the basic render loop. The live NVIDIA desktop may use the threaded loop, where the polish-driven frame may not occur; it was not measured there.
- While an alert is held, the layer's removal changes about 125 antialiasing pixels per alert (up to 0.96 on single edge pixels); the row changes are pixel-identical. The default snapshots are identical.

### Plan 2b: the live install (2026-10-22 01:59, HEAD `f202ade`)

After the reinstall the shell was restarted once to warm the shader caches. The live services' `smaps_rollup` Rss afterwards was **194 964 KiB** for the shell and **34 536 KiB** for `ghostd`. The desktop was covered by a maximised window at the time.

At about 12:24 on 2026-10-22, with the RAPL udev rule installed, the live daemon's `package_w` read **38.9 W** and then **41.3 W**. These are two readings from the live desktop, with no load control, so they show that the counter is readable and plausible, not a power figure for any state.

### Plan 2b: uplink widget (V1 and V2, UPLINK tab)

Headless KWin at 2560×1440, `QT_SCALE_FACTOR=1.15`, a release `ghostd --fake` on its own socket with the history warmed for at least 300 s, default appearance. A new `qs` per run, a 15 s settle, then `measure-cpu.sh` on the shell and the daemon for 60 s. Frames from `QSG_RENDER_TIMING`, `Rss` from `smaps_rollup` at the end of the window. Views interleaved, one warm-up run of each discarded.

| View | Combined, median (range) | Shell | Daemon | Frames/s | Shell Rss (KiB) |
|---|---|---|---|---|---|
| V2 centre, UPLINK | **1.34 %** (1.32–1.50) | 1.32 % | 0.02 % | 14.92 | 233 524 |
| V1 scope, UPLINK | **1.57 %** (1.55–1.59) | 1.55 % | 0.02 % | 15.22 | 241 032 |

- **With the real daemon** (release, not `--fake`, one run each, V2): a placeholder in the tab reads 0.38 % shell and 0.32 % daemon at 2.00 frames/s; UPLINK reads 1.42 % shell and 0.37 % daemon at 14.92 frames/s, **1.79 %** combined. The daemon's Rss goes from 41 284 to 48 624 KiB, **+7.3 MiB**, while the topic is wanted: the GeoIP pages the lookups touch and the 5 s cycle. They are unmapped once the tab is left.
- The shell's cost is the 15 frames/s loop over the whole window, not the globe alone: the gallery's globe-only page cost +0.52 to 0.6 points, the full V2 +1.0 to 1.2. The labels' share was not separated.
- Shell Rss moved by ±30 MiB from run to run in both arms, so no Rss delta for the widget could be resolved.
- After the fix that stops and dims a stale globe, one check on V2 (fake release daemon, warm-up discarded) read **1.45 %** (1.40 % shell, 0.05 % daemon) at 14.98 frames/s and 234 152 KiB, inside the first round's range. With the daemon stopped, the frame-clock hold dropped and the topic want stayed.
- **Outcome: under 2.00 % and under 307 200 KiB.** The real-daemon V1 was not measured here. Its fake shell figure plus the real daemon's 0.37 % gave an estimate of about 1.9 %; see the acceptance run below.

### Plan 2b: every text scrambles on its first write in a showing

A text now scrambles on its first write in each showing of its widget: labels and values alike, numbers and the clock included. Headless debug fake. Each run switches V1 to MONITOR and measures the 2 s after the switch with `measure-cpu.sh <qs pid> 2`, frames from `QSG_RENDER_TIMING`, three runs a build:

| Build | Frames in 2 s | Shell CPU over 2 s | Scrambles |
|---|---|---|---|
| Titles only (before) | 43–45 | 6.0–7.0 % | 8 |
| Every first write (after) | 42–45 | 7.0–9.0 % | 99–103 |

- **Frames** are flat: the print swap already holds the frame clock through a switch, and the scrambles ride on its ticks. The CPU is about +1.3 points over the 2 s, about 25 ms of CPU per switch.
- The holds peak at 95–99 and are back to the baseline 0.72–0.74 s after the switch, the length of the longest scramble.
- A still MONITOR afterwards (10 s) reads 0.4–0.5 % before and 0.5–0.6 % after, unchanged within noise.
- **Rss** (settle, then MONITOR and V1 three times each, `smaps_rollup` 12 s after each switch):

| Build | V1 at start (MiB) | Round 3, MONITOR | Round 3, V1 |
|---|---|---|---|
| Titles only | 209.0 / 211.5 | 239.2 / 237.3 | 240.3 / 238.8 |
| Every first write | 221.7 / 221.1 | 254.2 / 254.9 | 257.0 / 254.9 |

- About **+12 to 17 MiB** at steady state, decomposed roughly (run noise about ±2 MiB) into the per-text objects (about +4 MiB), glyph atlases for weights and sizes that never scrambled before (about +4 MiB), and the katakana of the scramble alphabet, which Kode Mono lacks, so they load Noto Sans Mono CJK and its atlases (about +7 MiB). Headless, under software GL; on the NVIDIA desktop the atlases may sit in VRAM instead.
- **Outcome: accepted by the user on 2026-10-22**, the full katakana alphabet included.

### Plan 2b: MONITOR's right rail

The process tree left the default views (user decision, 2026-10-22). MONITOR's right rail is now storage above sensors:

- **Storage, 188 design px:** all five drives (three NVMe, two SATA) and the R/W line, with no spare space.
- **Sensors, the rest:** 350 design px at the default 1256×666 area, of which its rows use about 270.

V1 and V2 keep two tabs, UPLINK and SILICON. The CPU figure for the new MONITOR is in the acceptance run below.

### Plan 2b acceptance: the budget states (2026-10-22, installed build)

The installed `ghostd` and shell (installed at 14:36 from HEAD `a907735`), each run in its own headless KWin at 2560×1440, `QT_SCALE_FACTOR=1.15`, with the service's environment (the unit's import path and atlas size, the NVIDIA drop-in, a per-run copy of the shader cache). The shell and a real `ghostd` shared a private bus; the system bus was real and read-only. PipeWire was shut out (`PIPEWIRE_REMOTE=/nonexistent` and an empty `PIPEWIRE_RUNTIME_DIR`), and the shell's state lived in a temporary directory. The config was the defaults with São Paulo's location and no alert provider, and each state was chosen by config: V1 with UPLINK, V2 with SILICON, and MONITOR reached from V1 through `Activate("monitor")`. `measure-cpu.sh` ran on the shell and its daemon at once for 60 s from 26 s after the start; frames counted from 2 s after the window opened. One warm-up run per state discarded, then three rounds with the states interleaved. Default appearance; the user's own settings were not measured.

| State | Combined, median (range) | Frames/s | Rss (KiB), shell + daemon | Target | Outcome |
|---|---|---|---|---|---|
| V1 animating, UPLINK | **1.60 %** (1.56–1.68) | 14.92 | 176 884 + 49 412 = 226 296 | ≤ 2.00 %, ≤ 307 200 KiB | pass |
| V2, SILICON | **1.87 %** (1.67–1.88) | 14.93 | 181 356 + 42 136 = 223 492 | ≤ 2.00 % | pass, thin margin |
| MONITOR, with the process tree | 1.05 % (1.02–1.24) | 2.20 | 205 512 + 42 380 = 247 892 | ≤ 1.00 % | over the target |
| MONITOR, storage in its place (HEAD `2f170f7`) | **0.80 %** (0.80–0.81) | 2.00 | 209 152 + 42 356 = 251 508 | ≤ 1.00 % | pass |
| Covered | **0.04 %** (0.04–0.05) | 0 | 175 792 + 42 124 | 0 frames, ≤ 0.50 % | pass |

- **V1** shell 1.18–1.30 %, daemon 0.37–0.38 %. The real-daemon estimate of about 1.9 % from the uplink measurement above is not reproduced.
- **V2 SILICON** shell 1.32–1.55 %, daemon 0.33–0.37 %. The 2.02 % mean of the silicon measurement above is not reproduced; the margin is 0.13 points.
- **MONITOR with the tree:** the 1.24 % run had the load average rising from 2.05 to 3.98 from something else on the machine; the two quiet runs read 1.02 and 1.05 %. The user had accepted MONITOR up to 1.25 % on 2026-10-22.
- **MONITOR now** (reinstalled from HEAD `2f170f7`, with storage instead of the process tree and every first write scrambling): shell 0.43–0.48 %, daemon 0.32–0.37 %, load average below 0.3 in every run. That is 0.25 points below the earlier reading, all of it in the shell. **Outcome: 0.80 %, inside the 1.00 % target**; the 1.25 % acceptance is no longer needed.
- **Covered:** both the installed-build runs and `scripts/measure-covered.sh 60` (three kept runs, repo build) read 0.04–0.05 % at 0 frames. The daemon's GeoIP mappings went from 2 before the cover to 0. Its read syscalls fell to exactly 240 a minute, the 1 Hz topics the covered shell keeps, against about 490 a second with V1 visible. This suggests no system-bus traffic for `uplink`, `net.links`, `disks.health` or `batteries`, but it is not proof: the bus itself was not captured.
- Shell Rss here (171–210 MB) is below the live service's, which holds more NVIDIA chunks; the live figure is not part of this reading.

### Plan 2c: the greeting (login, unlock, wake)

- **First frame**, from the shell's start to the overlay's first swapped frame, headless KWin at 2560×1440 (`make smoke-greeting`): 226–287 ms with a real `ghostd`, once 469 ms at load average ≈ 10; 175 ms with no daemon.
- **Idle CPU:** 1.00–1.05 %, unchanged from before the plan.
- **Shell Rss:** +0.5–1.1 MiB with the greeting off (`GHOST_GREETING=off`); 10 s after a greeting has played, +1.3 MiB, within the run-to-run spread.
- **Locked session** (installed build, HEAD `f5ad921`, live desktop, 2026-10-23 10:51, `measure-cpu.sh` on both processes for 60 s from 5 s after the lock): shell **0.02 %**, `ghostd` **0.03 %**; Rss 149 092 + 34 556 KiB. The overlay rests under the lock and nothing ticks, well inside the covered budget (≤ 0.50 %). 15 s after the unlock: shell 0.02 %, `ghostd` 0.00 %; what covered the screen then was not recorded.

### Plan 3a: the bar and the launcher

Headless KWin at 2560×1440, `QT_SCALE_FACTOR=1.15`, private buses, measured 2026-10-23 and 2026-10-24. Per-change deltas come from identical arms that differ only in the thing measured, 4 interleaved 60 s runs per arm unless noted.

**Budget states** (layout A, top, mono, release `ghostd` in the sandbox, shell and daemon combined, median of 3 × 60 s, no history warm-up; `[bar.clock] seconds = "covered"`):

| State | Combined, median (runs) | Rss, shell + daemon (KiB) | Target | Outcome |
|---|---|---|---|---|
| V1 animating, UPLINK | **2.50 %** (2.51, 2.17, 2.50) | 273 128 | ≤ 2.00 %, ≤ 307 200 KiB | over; accepted by the user |
| V2, SILICON | **2.65 %** (2.58, 2.77, 2.65) | 274 048 | ≤ 2.00 % | over; accepted by the user |
| MONITOR | **1.45 %** (1.45, 1.47, 1.44) | 287 268 (one run 348 124, an NVIDIA chunk jump) | ≤ 1.25 % (the user's accepted figure) | over; accepted by the user |
| Covered, with the bar | **0.29 %** (0.31, 0.29, 0.29) | | ≤ 0.50 % | pass |
| Covered, layout B, ticker idle | 0.46 % (0.46, 0.62, 0.45) | | ≤ 0.50 % | pass |
| Covered, layout B, ticker scrolling (fake daemon) | 0.55 % (0.47, 0.55, 0.62) | | ≤ 0.50 % | over; with the real daemon's ≈ 0.22 % about 0.75 % (estimated, not measured) |

- On 2026-10-24 the user approved these overages and asked for them to be recorded. The bar's own share was not separated from baseline drift: with the bar off, the same headless V1 already read 2.14–2.30 % on 2026-10-23, against Plan 2b's 1.60 %, and the user's own desktop was running during every run.
- The bar clock with `seconds = "always"` (the user's own setting) adds about +0.63 % (V1) and +0.18 % (MONITOR): each bar repaint is a cold wake-up of one more layer-shell surface.

**Per-change deltas:**

| Change | CPU | Rss | Notes |
|---|---|---|---|
| Window bridge (KWin script long-poll) | < 0.01 % idle | 0 | click-to-act median 0.5 ms (n = 20, two runs) |
| Bar with brand, view chip and clock, `seconds = "covered"` | V1 2.14 → 2.18 %, MONITOR 0.92 → 0.95 %, covered 0.04 → 0.14 % (65 frames/60 s) | +3–5 MiB | the covered frames are the clock's seconds |
| Task icons, 11 pins | none | **+19.0 MiB** (was +21.6 MiB through `image://icon`) | a one-time cost of the icon theme, paid once by any widget that shows app icons: ≈ +14 MiB for the first icon, ≈ +0.6 MiB each after; accepted by the user |
| Tabs instead of icons (layout C) | +0.075 % | −5.0 MiB | |
| Ticker, 5 steps/s on its own timer (layout B, covered) | +0.095 % | −0.6 MiB | at the frame clock's 15 fps it cost +0.59 % |
| Tray | +0.09 % | +3.9 MiB | |
| Launcher, never opened | none | +0.35 MiB | |
| Launcher, open | | **+35.0 MiB** | panel-sized surface; the full-screen one read +42–44 MiB |
| Launcher, after the first close | | **+18.9 MiB** | one-time and flat over repeated opens (NVIDIA driver mappings and the list's caches are not returned); accepted by the user |

**Daemon feeds** (live, read-only): the ticker's failed-units count ≈ 0 s, firewalld state 0.01 s, PackageKit `GetUpdates` 0.82 s wall time.

**Known limits of layout B** (left as they are; layout A is the default): the dock reserves no screen space, so maximised windows run under it; and its 58 px band leaves V1 too short for MEMORY's process rows and STORAGE's second and third drives.

### Project 2: the lock screen, style A (installed, 2026-10-24)

`kscreenlocker_greet` running the gate's prompt, idle and locked on the M27Q, measured over 60 s from `/proc/<pid>/stat` (budget ≤ 1 %, spec §8.4):

| CPU | Memory |
|---|---|
| **0.40 %** | 512 MiB `VmRSS` |

The greeter is non-dumpable, so `smaps_rollup` is unreadable and `scripts/measure-cpu.sh` cannot run on it. `VmRSS` counts NVIDIA's `/dev/nvidiactl` mappings twice and includes the greeter's own Plasma imports, so it overstates the gate's memory.

## License

Copyright (C) 2026 Gabriel Mitelman Tkacz

GPL-3.0-or-later; see [LICENSE](LICENSE).

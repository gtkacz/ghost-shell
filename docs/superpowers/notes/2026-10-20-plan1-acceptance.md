# Plan 1 acceptance: install, user services, budgets, live config

Date: 2026-10-21, 04:20–04:46 local time. Task 18 of plan 1, run on the user's live Plasma session while they were away, within the actions they approved beforehand.

## Environment

- Ryzen 9 9950X3D, RTX 4080 SUPER (driver 615.78.08) plus the CPU's Radeon 610M, 96 GB RAM, Fedora 44, kernel 7.2.11, KDE Plasma 6.7 Wayland, Quickshell 0.2.1, Qt 6.11.2. One 2560×1440 monitor (M27Q) at 170 Hz and scale 1.15, so one design px is 2 physical px.
- Base commit `7e3ebba`. Before the run, `make shaders && make test` passed: node 60/60 and cargo 190/190.
- The machine was quiet throughout (load average 0.01–0.23). `plasmashell`, `kwin_wayland` and the user's other sessions stayed up. conky was not running before or after the run. The `storm-alert` timer and the gridlock-keeper KWin script were not touched.

## Install and start

1. Gridlock: `ps -o pid,args -p 783628` showed `qs -c gridlock`, so it was stopped with `kill 783628`. Its `probe.sh` child (783646) exited with it.
2. `scripts/install.sh` exited 0 and printed the seven `==>` steps:
   - it built and installed `~/.local/bin/ghostd` (byte-identical to `target/release/ghostd`);
   - it copied `shell/` (with the four `*.frag.qsb`) and `widgets/` (graphics, memory, placeholder, processor, sensors, storage; no `broken-example`) into `~/.local/share/ghost-shell/`;
   - it downloaded `KodeMono[ROND,wdth,wght].ttf` and `OFL.txt` into `~/.local/share/fonts/ghost-shell/`; `fc-match 'Kode Mono'` now resolves to that file;
   - it installed both units into `~/.config/systemd/user/` and ran `daemon-reload`.
3. `systemctl --user start ghostd ghost-shell`, then 5 s later `is-active`: `active`, `active`. `is-enabled`: `disabled`, `disabled`. Nothing was enabled.
4. The journal at startup was clean: `ghostd: listening socket=/run/user/1000/ghostd.sock config=…/config.toml`, then `ghostd connected` and `Configuration Loaded` from the shell. The daemon had no config file at that point, so it used the defaults.

## Budgets

Each run used `scripts/measure-cpu.sh <MainPID> <seconds>` on both service pids **concurrently**, with the default config. The dead WD Green's crit alert animates the whole time. The budget is ≤ 2.00 % of one core and ≤ 307 200 KiB RSS combined.

| Run | Window | Shell (`qs`) CPU / RSS | Daemon CPU / RSS | Combined CPU | Combined RSS | Result |
|---|---|---|---|---|---|---|
| 1 | 60 s, 30 s after start | 0.95 % / 281 396 KiB | 0.37 % / 32 208 KiB | **1.32 %** | **313 604 KiB** | CPU pass, **RSS fail** (+6 404 KiB) |
| 2 | 60 s, 27 s after a shell restart | 1.50 % / 221 160 KiB | 0.42 % / 32 720 KiB | **1.92 %** | **253 880 KiB** | pass |
| 3 | 300 s, straight after run 2 | 1.49 % / 222 212 KiB | 0.40 % / 32 724 KiB | **1.89 %** | **254 936 KiB** | pass |

### The RSS overrun in run 1

- Run 1's shell carried a single anonymous mapping of 75 776 KiB (`rw-p`, 60 096 KiB resident). Its anonymous memory was about 119 MiB, and that mapping accounted for 59 MiB of it. File-backed memory was 148 MiB, of which 133 MiB was shared-clean library pages (PSS 146 MiB). Without the mapping, its footprint matches the other instances. Its RSS was flat at about 282 MB from minute 2 to minute 12, so the mapping is not a leak.
- It is **not** the font. Run alone, four short-lived instances sat at 218–222 MB with no such mapping: two with Kode Mono and two with fontconfig rejecting only the Kode Mono directory, so they used the fallback.
- It is **not** shader compilation. Every instance had NVIDIA's compiler (`libnvidia-gpucomp`) resident, with or without the mapping. Neither NVIDIA's GL cache nor Qt's pipeline cache (`~/.cache/quickshell/qtpipelinecache-*/qqpc_opengl`, last written 2026-10-20 21:09) is updated by these runs.
- It is intermittent. Across the 13 instances of the same build whose memory was read in this session, it appeared 3 times: the first service instance, and two test instances that ran next to other test instances. One more instance had an intermediate +13 MB. It never appeared in an instance running alone next to the service.
- With the mapping, the shell plus daemon exceed the budget by about 2 %. Without it they are at about 254 MB, 83 % of the budget. Its origin is still open (a GL or Wayland buffer path is the likeliest, since the mapping is about five 2560×1440 RGBA buffers). See Known gaps.
- systemd's accounting for the first shell instance reported a 131.3 MB memory peak (charged pages), against 282 MB RSS. Most of the RSS difference is shared library pages, so the RSS budget is the conservative measure.

### CPU

- The shell measured 0.95 % in run 1 and 1.49–1.50 % in runs 2 and 3, against 1.03–1.25 % in Task 17 (fallback font, started from a terminal). The combined margin in runs 2 and 3 is 0.08–0.11 points.
- systemd reported 13.353 s of CPU over 11 min 58 s for the first shell instance, an average of 1.86 %. That lifetime includes startup and about 5 minutes with up to three diagnostic instances drawing over it, so it is not a clean figure. It does, however, agree with the 1.5 % runs rather than with run 1.
- The "nothing animating" budget (≤ 1 %) cannot be measured on this machine with the default view, because the dead drive's alert always animates. Task 16 measured ≈ 0.9 % for that state.

### Frames

- The service was started without `QSG_RENDER_TIMING`, so its frames were not counted directly.
- A reference instance of the same install, run at the same time with `QSG_RENDER_TIMING=1`, rendered **15.04 frames/s**: 980 frames over 65.2 s, mean `perWindowFrameDelta` 66.5 ms, none above 80 ms. CPU-side frame time was 0–2 ms.
- As a proxy, the service's GUI thread and Wayland frame-callback thread woke 3461 and 2760 times in run 1's 60 s, against 3221 and 2576 for the reference instance. That is consistent with the same ≈ 15 frames/s.

## Live config

The config was written with an atomic rename. The evidence is the daemon/shell journal, plus snapshots from a **short-lived second shell** (`GHOST_SNAPSHOT=… qs -p ~/.local/share/ghost-shell/shell`, stopped by its pid once it logged `snapshot saved`). The panel colour equals the ground colour (`#0a0f11`), so `panel_fill` shows only as how much of the dot grid comes through a panel. It was measured as the brightest 0.5 % of pixels inside the empty CLOCK panel divided by the same figure in the open strip above the panels.

| Step | Write → `config reloaded` | Evidence | Result |
|---|---|---|---|
| a. `ghostd default-config` + `display.model = "M27Q"` + São Paulo (`city`, `lat = -23.55`, `lon = -46.63`, `[location.alerts] provider = "inmet"`, `area = "3550308"`) | 154 ms | journal `config reloaded`, no problem. `provider = "inmet"` is accepted, so the `provider = ""` fallback was not needed. Snapshot `fill55.png`: see-through 0.436 (expected 0.45) | pass |
| b. `panel_fill = 30` | 154 ms | snapshot `fill30.png`: see-through **0.711** (expected 0.70), against 0.436 at 55 | pass |
| c. `warn = "orange"` | 154 ms | daemon: `config not applied; keeping the last good one error=appearance.colors.warn: expected #rrggbb, got "orange"`. The service shell logged `ghostd config problem: appearance.colors.warn: expected #rrggbb, got "orange"`. The session bus carried one `Notify` call: app `ghost-shell`, icon `dialog-warning`, summary `ghost-shell config`, body `appearance.colors.warn: expected #rrggbb, got "orange"`. Snapshot `warn-orange.png`: still fill 30 (0.711), zero orange (`#ffa500`) pixels, crit-red pixel count identical to `fill30.png` (180) | pass |
| d. `warn` restored, `storage` and `sensors` slots swapped in `layout.views.v1` (each slot keeps its `size`) | 154 ms | snapshot `swapped.png`: SENSORS now sits under UPLINK (184 design px) and STORAGE at the bottom (the leftover 158), every row and the dead-drive alert intact | pass |
| e. Restore the approved final config (step a) | 153 ms | journal `config reloaded`, no problem line. `config.toml` is byte-identical to the step-a file. Snapshot `final.png`: default arrangement, see-through 0.436 | pass |

The ≈ 153 ms latency is the watcher's debounce. The only warnings in the whole journal since the services started are the three lines from step c.

## Snapshots

All are 2560×1440 and live under `/tmp/gs-accept/` (not committed): `fill55.png`, `fill30.png`, `warn-orange.png`, `swapped.png`, `final.png`, plus the diagnostic `ab-kode-mono.png` and `ab-fallback.png`.

- **V1 geometry** (`final.png`) matches the default tree at 2 px per design px, to within the 2–3 px precision of reading the image:
  - the left column (x 24–624) holds PROCESSOR (250), GRAPHICS (204) and MEMORY (the rest, down to y 1416);
  - the middle column (x 648–1912) holds CLOCK (262) over a row of MEDIA (290) and NOTIFICATIONS;
  - the right column (x 1936–2536) holds UPLINK (300), STORAGE (the leftover 158) and SENSORS (184);
  - margins and gutters are 12 design px, and the content starts at y 84 (margin + 30).
  CLOCK, MEDIA, NOTIFICATIONS and UPLINK are placeholder panels (Plan 2).
- **Dead WD Green**: STORAGE ("5 DRIVES") lists the XPG GAMMIX S70 BLADE (/, /boot, /boot/efi…, 41 %), the Kingston SKC2500M8250G (75 %) and a red striped crit row: `危険 WDC WDS960G2G0C-00AJM0 · nv… DEAD`. SENSORS lists the same drive as `--`.
- **Panels**: PROCESSOR has two CCD block grids (V-CACHE and FREQ) with load, Tctl, frequency and CPU fan. GRAPHICS shows the RTX 4080 SUPER at 1 % load. MEMORY shows 17.5 / 91.9 GiB and a top-process table. SENSORS shows 12 probes in two columns.
- **Font**: every Latin glyph renders in Kode Mono. The running service shell has `~/.local/share/fonts/ghost-shell/KodeMono[ROND,wdth,wght].ttf` mapped, as well as `NotoSansCJK-VF.ttc` (for the kanji accents 演算, 描画, 記憶, 記録, 温度, 危険) and `GoogleSans_VariableFont…ttf`. Google Sans is presumably the Plasma default font, which Qt loads as the application font; no text in the snapshot uses it.

## Deviations from the plan

- **The shell service was restarted once** (`systemctl --user restart ghost-shell`, 04:35:56) to get a second sample after run 1's RSS overrun. The daemon was not restarted.
- **Diagnostic instances.** Besides the five snapshot instances, eleven more short-lived `qs -p` instances ran for ≤ 90 s each to find the RSS cause. Each was stopped by its own pid, and none survived. They connected to the service daemon and drew on the desktop layer while they ran.
- **Frames** were not counted on the service itself (see Frames).
- **Notification evidence** came from a read-only `dbus-monitor` filtered to `org.freedesktop.Notifications.Notify`, stopped by its pid. Only ghostd's call was kept.
- **Order in Step 3.** `warn` was set back to its hex value in the same save as the slot swap. While the file is invalid, the daemon refuses every change in it, so the swap would otherwise never apply.
- **Gridlock stays stopped** and the services keep running, at the user's request. Step 4's "stop the services" was therefore not done.
- **No push.**

## Known gaps / parked items

Collected from the SDD ledger's parked and carry-forward notes. The cleanup batch's 18 items and its fix round (watcher re-arm, validation gaps, AQI wording, percent-encoded sources, discovery problems, vcache rule, mem prune, natural sort, provider process groups, watch-before-discover, socket flock, install.sh XDG and qsb-qt6, `After=graphical-session.target`, `probe mem`) are excluded.

### New in this run

- An intermittent ~74 MiB anonymous mapping in the shell put run 1 over the RSS budget. Cause unknown.
- The shell's CPU is 1.5 % rather than Task 17's 1.0–1.25 %, which leaves a 0.1-point combined margin.
- The ≤ 1 % still budget cannot be measured with the default view while a drive is dead.

### Desktop integration

- ghost-shell needs its own KWin keeper script before gridlock-keeper is retired (R46, Plan 2a Task 4). Today gridlock-keeper keeps ghost-shell above Plasma's wallpaper.
- Covered surfaces keep rendering (R10). Occlusion detection is Plan 2, and the "covered ≤ 0.5 %, zero frames" budget is unmeasured.
- Enabling at login and retiring gridlock/conky are Plan 4.

### Daemon

- **Config:**
  - TOML parse errors are multi-line and lack a path prefix (P4).
  - A symlinked `config.toml` is not seen (P7).
  - A deleted config dir silently ends the watch; `Q_OVERFLOW` rescans are dropped; the Access filter also drops `Close(Write)`.
  - `ConfigError` has no "cleared" message (P13).
  - A `write_all` failure exits 1 without a message.
- **Collectors:**
  - The pid-keyed kthread cache can hide a recycled pid's user process.
  - `hidepid=1` causes one failed open per foreign pid per tick.
  - Static CPU topology (an SMT toggle halves the CCD mean).
  - A fanless GPU reads as absent (R24). NVML is never retried or reopened after a failure, and its error causes are discarded (P11).
  - Duplicate drive serials give duplicate ids. LUKS/LVM/md/multi-device btrfs mounts are not attributed. `statvfs` on a hung FUSE mount stalls the disk sample. A recovered drive keeps a cached `used_pct` for up to 60 samples.
  - A hwmon chip is dropped when neither its device link nor its dir canonicalizes. The ordinal counts sensorless twins.
- **Server:**
  - No publish rate limit.
  - A provider fast-reconnect race.
  - The recursive widget watch has no path filter (`.git` churn re-sends the Layout).
  - A deleted and recreated widget dir is not re-watched until a reload.
  - A tiny `--tick-ms` makes History exceed the 256 KiB outbox.
  - `reconcile` discards a provider task's `JoinError`.
  - The `<socket>.lock` file stays on disk.
- **History:** a series that stops (gpu_load while the GPU is absent) loses time alignment (P12). Plan 2 may add gap markers.
- **Tests not written:**
  - topic-name edge cases and `ServerMsg` round-trips;
  - several validation rules and the `Config` wire form;
  - watcher removal, unrelated-file and coalescing cases;
  - CPU edge cases (no index3, malformed or backwards counters, offline CPU);
  - hwmon ordinal stability;
  - `StatvfsFs` and elapsed wiring;
  - sampler wiring in `main.rs`.

### Shell

- **Data:** only a global stale flag exists, not per-topic staleness (P15). Suspending the FrameClock freezes the blink step, and Data publishes immediately while suspended (P16). A JSON array line passes the non-object guard.
- **Widgets:**
  - BlockGraph needs a shader/scene-graph version for Plan 2's full-width lanes; the stress layout costs 2.42 % / 297 MB (P17).
  - Tab strip placement versus the mockup (P18).
  - The Sensors two-column grid needs another look (P19).
  - Storage ignores per-sensor overrides and its count doesn't show dropped rows.
  - THERM lacks GPU CORE/GPU FAN.
  - °C vs ° on paired lines; unmounted rows use a dim colour rather than opacity .5; the overflow warning repeats on each Layout.
- **Formatting edge cases:**
  - `withAlpha(null)` gives `#8CNULL`; NaN alpha and 3-digit hex are not handled;
  - `gib(-1)` gives "-0.0" and `Fmt.pct(-x)` gives "-0%";
  - `rate` has no GB tier and rounds half-up on exact tenths;
  - `cellCount(NaN)`;
  - `DEFAULT_V1_FOR_TEST` is a copy of the Rust default tree.
- The dot radius is 1 design px, against "1 px dots" in the spec.

### Scripts

- `measure-cpu.sh ""` passes its pre-check, because `/proc//stat` resolves to `/proc/stat`.
- The smoke allow-pattern check doesn't reject non-empty catch-alls such as `.` or `.+`, and the smoke run leaves `/tmp/ghost-smoke.sock.lock` behind.

### Docs

- `docs/widgets.md` (the widget contract) does not exist yet.

## State left for the morning

- `ghostd.service` (pid 1587581) and `ghost-shell.service` (pid 1592217) are **active** and **not enabled**.
- Gridlock is **stopped**. To get it back: `systemctl --user stop ghost-shell ghostd`, then `dash` or `qs -c gridlock &`.
- `~/.config/ghost-shell/config.toml` holds the `ghostd default-config` output plus `display.model = "M27Q"` and São Paulo (`city = "São Paulo"`, `lat = -23.55`, `lon = -46.63`, `[location.alerts] provider = "inmet"`, `area = "3550308"`).
- Kode Mono is installed in `~/.local/share/fonts/ghost-shell/`.

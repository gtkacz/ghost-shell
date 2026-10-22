# ghost-shell Plan 2 — Part 2b: Machine Depth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** By the end of Part 2b:
- every machine panel the spec lists has a real widget: network, the six MONITOR telemetry lanes, the process tree, the uplink globe, the silicon schematic;
- storage shows drive wear and power-on hours, sensors shows device batteries, and the processor shows CPU package power;
- full-width graphs draw as one shader each, as the kit's `BlockGraph` does, so no view pays one `Rectangle` per cell;
- the default V1, V2 and MONITOR views lose their last machine placeholders;
- the spectrum capture retries after PipeWire restarts;
- every new collector and feed states its cadence and is measured, and the budget states are measured again.

**Architecture:**
- **Daemon (`ghostd`):**
  - three new **sampled** built-ins on the 1 Hz tick, so they share the tick `seq`, gap nulls and history (Plan 2a R5):
    - `net`: `/proc/net/dev` rates;
    - `procs`: the process scan split out of `mem`;
    - `procs.tree`: the full table, every fifth scan;
  - four new **feeds**:
    - `net.links`: NetworkManager, Tailscale's local API, ICMP ping;
    - `uplink`: `/proc/net/*` sockets, offline GeoIP and ASN, owning processes, Tailscale peers;
    - `disks.health`: UDisks2 cached SMART;
    - `batteries`: UPower and KDE Connect;
  - the history gains per-core load, memory, disk I/O, network and three temperature series;
  - CPU package power comes from RAPL, through a user-approved udev rule.
- **Shell:**
  - the kit adds `MirrorGraph`, `HeatGraph` and `Globe`, drawn by shaders with packed uniforms (`BlockGraph` already is one, 2a Task 15b);
  - tab strips move into the panel header row (P18);
  - layout splits may set their own `gutter`.
- **Widgets:**
  - new: `telemetry-lane`, `network`, `process-tree`, `uplink`, `silicon`;
  - changed: `memory` (reads `procs`), `processor` (PKG), `storage` (wear, hours), `sensors` (batteries).

**Tech Stack:**
- Rust 1.98. New crates:
  - `maxminddb` with its `mmap` feature, so only the pages a lookup touches become resident, and they count in `smaps_rollup` Rss (R50);
  - nothing else. NetworkManager, UPower, UDisks2 and KDE Connect go through the existing zbus 6. Tailscale's local API is plain HTTP/1.0 over `tokio::net::UnixStream`. Ping uses `nix` sockets.
- Quickshell 0.2.1, QtQuick.Shapes, ShaderEffect (`qsb-qt6`, `.frag` and `.vert`).
- Node 24 for `.mjs` tests.
- `uv run --no-project --with pyshp --with pillow --with numpy python -I` for the land-mask and lattice generator.
- ImageMagick `compare` for pixel checks.

**Spec:** `docs/superpowers/specs/2026-10-20-ghost-shell-design.md`: §3 the machine, §4 stack, §5 visual language, §6.2–6.3 layouts and panels, §12 data sources, §13 extensibility, §15 performance. The mockup `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html` is the visual truth, and its drawing functions (`blocks`, `mirror`, `heat`, `drawGlobe`, `drawDie`, `buildTree`/`drawTree`, the `C.lane`/`C.net`/`C.scope` templates) are the reference for every renderer below. Read it together with:
- Plan 2a, `docs/superpowers/plans/2026-10-21-ghost-shell-plan-2-dashboard.md`, which this plan's outline closes;
- the Plan 2a ledger `.superpowers/sdd/2026-10-21-ghost-shell-plan-2-dashboard/progress.md`, rulings R1–R51 and carry-forwards C1–C27.

**Prerequisite:** Plan 2a is complete at `4e3e099`, so Tasks 15–18 are merged (media widget, audio rows, default V2 and MONITOR, install), and:
- this plan edits the trees 2a Task 17 wrote;
- it uses 2a Task 15's public `ClickArea` (R13);
- it uses the `fake/` split that R4(g) foresaw.

This plan was written against `main` at `fbc6a86`, with 2a Task 15 in progress, and amended at `4e3e099`. Part 2b runs on branch `plan-2b`, in the worktree `.claude/worktrees/plan-2b`.

## Decisions (user, 2026-10-21)

The four open questions from the Plan 2a outline, plus one on the renderer:

1. **Endpoint labels:**
   - the default is the endpoint's **ASN organisation** from the offline DB-IP ASN Lite file;
   - `[uplink.labels]` in config.toml maps an organisation name (case-insensitive) or `AS<number>` either to fixed text or to the special value `"process"`, which shows the local process that owns the socket;
   - the `/proc/*/fd` walk runs only for endpoints whose label resolves to `"process"`.
2. **Land mask:** a repo script rasterises Natural Earth 1:50m land into a PNG. **Both the script and the PNG are committed.** Install needs no network.
3. **RAPL:** **world-readable** `energy_uj` (0444), set by a udev rule the user installs with sudo. The rule file states the PLATYPUS (CVE-2020-8694) trade-off in its comment.
4. **Tailscale peers:** a peer with a **direct** connection is located by GeoIP of its current public endpoint (`CurAddr` in Tailscale's status) and labelled `TAILNET:<HOST>`. A relayed (DERP) peer is left off the globe and still counted in the VPN line.
5. **Renderer (P17):** shaders with packed uniforms, built as the kit's shader `BlockGraph` (user decision R49, 2026-10-21, which replaces the native C++ module). No devel packages are needed.

## Scope and calls

Calls marked **[call]** are mine. The user may override them.

1. **[call] Sockets come from `/proc/net/{tcp,tcp6,udp,udp6}`, not netlink `sock_diag`** (the spec's own [call]):
   - the text tables carry the remote address, the uid and the inode the process lookup needs;
   - parsing them needs no new crate;
   - the table is a few hundred rows on this desktop, read every 5 s and only while the globe is wanted.

   Task 10 measures the cost.
2. **[call] GeoIP files:**
   - DB-IP City Lite and ASN Lite (CC BY 4.0) are downloaded by `scripts/fetch-geoip.sh`, **run only with the user's consent**, into `$XDG_DATA_HOME/ghost-shell/geoip/`;
   - the city database falls back to Fedora's `/usr/share/GeoIP/GeoLite2-City.mmdb`, which is present here but dated 2019;
   - the globe footer carries the licence attribution.
3. **[call] Topic split.** `net` (sampled) carries rates only. `net.links` (a feed) carries links, Tailscale and ping, so the rates keep the tick `seq` and their history gaps.
4. **[call] Silicon animates.** Spec §6.3 and the mockup move packets along the Infinity Fabric links, but §15 names "the SILICON tab with no alerts" as its "nothing animating" example. The mockup wins (spec §1). So the still budget (≤ 1 %) is measured on MONITOR, as in Plan 2a. SILICON is measured as animating (≤ 2 %). This changes the outline's item 12.
5. **[call] The silicon schematic draws only what the daemon knows.** The mockup's `AM5`, `DDR5`, `PCIE 5 X16` and `6 NM` are facts about this machine that no topic carries, and spec §13 wants generic defaults. So:
   - the title is `<MODEL> · <C>C / <T>T`;
   - the I/O-die blocks read `MEM`, `GPU`, `NVME` and `IGPU`;
   - the iGPU block reads `OFF` when no AMD DRM device reports `gpu_busy_percent`, which is the case on this machine.
6. **[call] Telemetry lanes follow the configured thresholds**, not the mockup's literals:
   - `Theme.loadWarn`/`loadCrit` for CPU, GPU and memory;
   - `Theme.tempWarn`/`tempCrit` for thermals.

   The spec's thresholds are binding (§5.2).
7. **[call] The V1 uplink scope is two panels:** a `tabs` node (UPLINK | SILICON | TREE) above a headerless `network {style="scope"}`, 12 px apart. The mockup draws one panel. One widget per slot is the contract (§13).
8. Notifications stay in Plan 3, so the V1/V2 `placeholder{Notifications}` slots stay.

## Global Constraints

Plan 2a's Global Constraints all still apply: pluggable contract, design units, palette, colour-for-attention thresholds, panels, segmented bars, font, Japanese, motion, budgets, daemon behaviour, D-Bus in tests, formats, config, commits. The rules below add to them.

- **Citations:** `P<n>` in a task's text is a ruling of the Part 2b pre-flight scan, except `(P17)` in Decision 5, `(P18)` in Task 13's heading and `(P19)` in Task 17's heading, which are Plan 2a's. `R<n>`, `C<n>`, `M<n>` and `FR` are entries of the Plan 2a ledger and final review; `Q<n>` is a user answer of 2026-10-21; `fx C<n>` is an entry of the fixtures' contradiction list.
- **Process rules** (Plan 2a ledger `process-rules.md`, binding for every agent):
  - never `rm -rf` (use `rm -r --`), `pkill`/`killall`, or `git reset`/`rebase`/`push`/`branch -D`, and never check out another branch;
  - stage by explicit path;
  - Python only as `uv run --no-project python -I …`;
  - private buses only as `dbus-run-session --config-file=<repo>/scripts/private-bus.conf -- …`;
  - dev daemons use their own socket, `--config <temp>` and `--widgets-dir "$repo/widgets"`;
  - never touch the running `ghostd`/`ghost-shell` services or `~/.config/ghost-shell/config.toml`, except in Task 19;
  - implementers never install on the user's desktop: after each task passes review, the controller reinstalls the branch on the user's live desktop.
- **Branch and base (P1):** Part 2b runs one task at a time on branch `plan-2b`, in the worktree `.claude/worktrees/plan-2b`. Each brief re-reads the files it edits at its own base, and cites the Plan 2a ledger through R51 and C27.
- **Live facts ("fx"):** `/home/gtkacz/Codes/ghost-shell/.superpowers/sdd/2026-10-21-ghost-shell-plan-2b-machine-depth/fixtures/` holds read-only captures of this machine (NetworkManager, Tailscale, `/proc/net/*`, UDisks2, UPower, KDE Connect, RAPL), anonymised. `fx/<file>` names one. Fixtures that a test needs are copied into the repo's `crates/ghostd/tests/fixtures/` by the task that uses them; the directory itself is git-ignored and is never committed.
- **Shader drawing:**
  - every graph and the globe are `ShaderEffect`s in `shell/kit/GhostKit/`, with `.frag`/`.vert` sources in `shaders/` compiled by `make shaders`; widgets never use a shader directly, only the kit's components;
  - the per-sample work stays in a `lib/*.mjs` module with node tests, and the shader restates no colour or alpha that `segments.mjs` or `lib/globe.mjs` defines: they reach it as uniforms or packed bytes;
  - the spike's kit notes are binding (P3; `docs/superpowers/notes/2026-10-21-blockgraph-shader-spike.md` and `shell/kit/GhostKit/shaders/blockgraph.frag`):
    - counts bound to uniforms are `real` (an `int` reaches a float uniform as ≈ 0);
    - colour properties arrive premultiplied;
    - ShaderEffect ignores array uniforms, so data travels in named `vec4` words picked by comparison;
    - `qsb --qt6` also bakes GLSL ES 100: no `round`, `texelFetch`, `%` on ints, array constructors or variable vector indexing; use `floor`, `mod` and comparisons, and unroll lists;
    - packed components are integers below 2^24, divided only by powers of two (or index + 0.5);
    - pixel grabs pass an explicit size;
    - the 1/512 px edge rule of `blockgraph.frag:56–58, 166–170` holds, under the OpenGL y-up rasteriser assumption of C22, which stays open;
  - a new sample repacks the words in JavaScript and renders one frame; nothing renders between samples except the globe, which follows `FrameClock`.
- **Headless runs (P10):** every `qs` run in 2b (smokes, snapshots, measurements, click checks) renders in its own headless KWin: `scripts/headless-kwin.sh <name>` (Task 8b commits it; before that, `.superpowers/sdd/2026-10-21-ghost-shell-plan-2-dashboard/headless-kwin.sh`), stopped by the pid it prints. Each task sets `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t<N>.sock`, takes snapshots at `QT_SCALE_FACTOR=1.15`, and waits for C21's settle before counting frames. The plan's "R52" references read "Plan 1 R52, run in the headless KWin". An agent window on the user's screen breaks their show-desktop (2a 305).
- **Budgets and measurements (P7, P9, Q1):**
  - the budgets are tight: playing measures 1.83–1.90 % of 2 %, and memory 282 of 300 MiB (`smaps_rollup` Rss), which leaves 18 772 KiB for everything in 2b (2a 370);
  - every task that adds shell items or a collector reports its CPU and Rss delta, and the controller keeps a running total against that headroom;
  - every pass mark is measured at the default appearance; the user's own settings (spectrum 30 fps, stripe step 0.01 s, attention 600 s) are recorded beside it, not judged;
  - if Task 7's or Task 9's MONITOR reading goes over 1.00 % combined, the task stops and reports the numbers, and the controller asks the user. Nothing is slowed or re-budgeted unilaterally.
- **Holds, wants and binding order (handoff, binding):**
  - release a `FrameClock` hold through `Qt.callLater`, never inside a binding's own update;
  - every new holder gets a test that the hold is released on hide, on cover and on destruction: Task 8b's `make smoke-holds` is that test for any widget placed in the config it runs, and the task that adds a holder says so in its own steps;
  - QML applies bindings in source order: an object that starts working when it gets its input must get that input only when it is meant to run;
  - split `widgets/media/Widget.qml` or `crates/ghostd/src/fake.rs` before adding to either (no 2b task edits `widgets/media`; Task 2 Step 1 splits `fake.rs`).
- **Carried 2a rules (P20):** the 2a settings that touch 2b widgets apply unchanged: R36/R40/R47 attention, R43 stripes, and C6 (`hostControl` writes, `textFormat: Text.PlainText` on every string the shell does not own such as process names, organisations, hostnames and battery models, `Fmt.readingByLabel`).
- **Tests assert (P11):** a test whose body is empty or only a comment is rejected in review. Every test named in this plan asserts what its name states.
- **Parked (P12):** M9's media half, M4 (the level meters) and C24 go to Plan 3: no 2b task edits `widgets/media` or `audio.mjs`. M7/C27 is Task 17b. C22 stays open.
- **History keys:**
  - `load`, `ccd0`, `ccd1`, `core<N>` (one per physical core) belong to `cpu`;
  - `gpu_load`, `temp_gpu` belong to `gpu`;
  - `mem_used` (GiB) belongs to `mem`;
  - `temp_cpu` belongs to `sensors`;
  - `disk_read`, `disk_write` (B/s summed over drives with `state == DriveState::Live`) and `temp_nvme` (hottest live NVMe, °C) belong to `disks`;
  - `net_down`, `net_up` (B/s on the default-route interface) belong to `net`.

  A key is pushed only on ticks its own topic is published, so the shell's appended series (Plan 2a R5) and the daemon's stay identical.
- **Network access added by this plan:**
  - ICMP echo to `network.ping` (default `1.1.1.1`, spec §12) every 5 s, only while `net.links` is wanted;
  - Tailscale's local socket (`/run/tailscale/tailscaled.sock`);
  - DB-IP downloads only through `scripts/fetch-geoip.sh`, which the user approved on 2026-10-21 for Task 10;
  - the Natural Earth download only while generating the land mask (Task 11, approved by the user on 2026-10-21).

  No reverse DNS. Tests never touch the network or the system bus.
- **Do not wake hardware (the R34 pattern):**
  - UDisks2 is read from cached properties and `SmartGetAttributes` on NVMe controllers only, never `SmartUpdate`, and never a method call on an ATA drive;
  - KDE Connect battery values are read, and no refresh is ever requested from the phone.
- **Services that come and go (the R30 pattern):** every D-Bus feed reads the full state when its service's name gains an owner (`NameOwnerChanged`), not only at start. An absent service publishes an empty value and waits. A feed never ends (Plan 2a R4(d)).
- **Long cadences use the wall clock (R25):** `disks.health` (10 min) and the KDE Connect poll (60 s) decide due-ness on wall-clock seconds, and every sleep is capped at 60 s while the topic is wanted.
- **Values the user confirmed (2026-10-21):** Tailscale status every 10 s; 2 s timeouts for Tailscale and ping; the process tree every 10 s; at most 8 globe endpoints; Task 1's targets (page creation ≤ 100 ms, Rss ≤ 30 MiB, CPU ≤ 0.30 %); 1000 processes laid out in under 20 ms; the 1024 × 512 lattice map; the spectrum retry delays of Task 17b (1 s, doubling, 30 s cap). The task that uses each one marks it.
- **Measure every new collector and feed:**
  - its cadence;
  - wall and on-CPU time per run;
  - its RSS delta (`smaps_rollup`);
  - for `uplink`, the process-walk cost with an override in force.

  The controller records the figures in README's Performance section (user rule).
- **Ask the user before:** anything in Task 19 that touches the live desktop beyond Plan 2a R17's procedure, and restarting PipeWire (Task 17b's live check). Approved already (user, 2026-10-21): the Natural Earth download (Task 11), running `scripts/fetch-geoip.sh` (Task 10) and Task 17's read-only `busctl --user introspect`. The RAPL udev rule install with sudo is Task 19's (P14).
- **Files ≤ 800 lines.** `crates/ghostd/src/fake.rs` (743 today) becomes a `fake/` module in Task 2, before any 2b fake is added (Plan 2a R4(g), M9). `main.rs` (588), `sampler.rs` (577), `crates/ghostd/tests/config_load.rs` (635) and `crates/ghost-proto/src/config.rs` (567) are split first, in their own refactor commit (feed construction to `feeds/mod.rs`, history recording to `history.rs`), when a task would take them past 800 (P19).

## Review Focus

1. **Counters that jump:**
   - `/proc/net/dev` counters reset when an interface goes down and up;
   - a VPN interface appears or disappears between samples;
   - RAPL `energy_uj` wraps at `max_energy_range_uj`.

   A rate must never read negative or as a burst of exabytes. *Tests: Tasks 5 and 14.*
2. **Endpoints with nothing to show:**
   - loopback, RFC 1918, link-local, CGNAT `100.64/10`, multicast, documentation ranges and ULA are never shown, and an IPv4-mapped IPv6 address is judged as its IPv4 address;
   - an address missing from the database, or no database at all, still yields a working globe and a label chain that ends at the IP. *Tests: Task 10.*
3. **Process churn:**
   - a pid reused between scans;
   - a collapsed tree node whose process exits;
   - an orphan whose parent is not in the table;
   - a thousand processes.

   The tree stays consistent and its labels stay readable. *Tests: Tasks 2 and 9.*
4. **D-Bus services that are absent, late or restarting:** NetworkManager, UPower, UDisks2 and KDE Connect each degrade to an empty value and are read in full when they (re)appear. A missing Tailscale socket reads `None`. *Tests: Tasks 6, 16 and 17.*
5. **Suspend and sleeping disks:**
   - the 10-minute health cadence is due on wall-clock time after a resume;
   - unmounted HDDs (`sda`, `sdb`) are never woken;
   - the dead WD NVMe's row shows no wear or hours, without breaking the rows of the other drives. *Tests: Task 16.*

---

### Task 1: Shader mirror and heat graphs (`MirrorGraph`, `HeatGraph`)

**Files:**
- Create:
  - `shell/kit/GhostKit/MirrorGraph.qml`, `shell/kit/GhostKit/HeatGraph.qml`, `shell/kit/GhostKit/HeatRow.qml` (internal);
  - `shell/kit/GhostKit/shaders/mirrorgraph.frag`, `shell/kit/GhostKit/shaders/heatrow.frag`;
  - `shell/kit/GhostKit/lib/mirrorgraph.mjs`, `shell/kit/GhostKit/lib/heatgraph.mjs`;
  - `shell/tests/mirrorgraph.test.mjs`, `shell/tests/heatgraph.test.mjs`.
- Modify:
  - `shell/kit/GhostKit/lib/blockgraph.mjs` (export `pack` and `COMPONENTS_PER_WORD`; no behaviour change, P4);
  - `shell/kit/GhostKit/lib/segments.mjs` and `shell/tests/segments.test.mjs` (mirror and heat colour rules);
  - `shell/kit/GhostKit/qmldir` (`MirrorGraph 1.0`, `HeatGraph 1.0`, `internal HeatRow`);
  - `shell/host/Gallery.qml` (`GHOST_GALLERY_LANES=mixed|heat|fixed`).
- Not touched: `BlockGraph.qml`, `blockgraph.frag`, `Makefile` (its `*.frag` wildcard covers both new shaders), `scripts/install.sh`, `scripts/qml-smoke.sh`, `README.md` Build.

**Interfaces:**
- Produces:
  - **`segments.mjs`** (mockup `mirror` and `heat`):
    - `MIRROR_UNLIT_ALPHA_PCT = 5`, `MIRROR_UP_FLOOR_PCT = 45`, `MIRROR_UP_SPAN_PCT = 40`, `MIRROR_DOWN_FLOOR_PCT = 45`, `MIRROR_DOWN_SPAN_PCT = 50`;
    - `mirrorCellColor(upper, row, lit, rows, theme)`: a lit upper cell is `hi` at (45 + 40·row/rows) %, a lit lower cell `signal` at (45 + 50·row/rows) %, an unlit cell `signal` at 5 %; `row` counts out from the axis;
    - `mirrorAlphaByte(upper, row, rows)`: that cell's alpha byte, read back from `withAlpha()` as `rampAlphaByte` does;
    - `HEAT_FLOOR_PCT = 7`, `HEAT_SPAN_PCT = 80`;
    - `heatCellColor(fraction, cellLevel, theme)`: `crit` and `warn` solid; `ok` is `signal` at (7 + 80·clamp(fraction, 0, 1)) %; a non-finite fraction is `signal` at `UNLIT_ALPHA_PCT` (6 %);
    - `heatAlphaByte(fraction)`: the `ok` cell's alpha byte, read back from `withAlpha()`.
  - **`lib/mirrorgraph.mjs`:**
    - `MIRROR_HEADROOM = 1.1` (mockup `nmax`/`imax`), `MAX_HALF_ROWS = 63`, `COLUMN_WORDS = 48`, `ROW_WORDS = 11`, `MAX_COLUMNS = 384`;
    - `mirrorMax(up, down, floor)` = max(floor, every finite sample of both) × 1.1;
    - `mirrorLit(value, max, rows)` = round(√clamp(value / max, 0, 1) × rows), and 0 for a non-finite value;
    - `fittedHalfRows(height, cellHeight, gapSize)` = `cellCount(floor(height / 2) − gapSize, cellHeight, gapSize)`. The mockup's `(half + g)` can place the top row one gap above the item; the kit keeps it inside;
    - `drawnHalfRows(rows)` = min(rows, 63). A taller graph draws the 63 rows nearest the axis on each side, with the values scaled to them, and warns once (R44b, P21);
    - `mirrorColumnWords(up, down, columns, rows, { max, live })`: a column's 12-bit field is upLit × 64 + downLit, two to a component, oldest drawn column first; a series shorter than the graph fills it from the right; a missing sample, or `live: false`, lights nothing on its side; columns past 384 are the oldest and stay unlit;
    - `mirrorRowWords(rows)`: fields 0–62 are the upper ramp's alpha bytes for rows 0…drawn − 1 (0 beyond), fields 63–125 the lower ramp's, three bytes to a component.
  - **`lib/heatgraph.mjs`:**
    - `HEAT_COLUMN_WORDS = 32`, `HEAT_MAX_COLUMNS = 384` (one byte per column, three to a component: the spike's first packing);
    - `CELL_MISSING = 0`, `CELL_WARN = 254`, `CELL_CRIT = 255`; an `ok` cell's byte is its `heatAlphaByte`, which lies in 18–222 and so never collides with the three codes;
    - `heatCellByte(sample, { lo, hi, warnAt, critAt, live })`;
    - `heatRowWords(series, columns, graph)`: one row's words, filled from the right like `BlockGraph`;
    - `heatRowHeight(height, rows, gapSize)` = (height − (rows − 1)·gapSize) / rows (mockup `heat`).
  - **`MirrorGraph { up; down; floor; cellW: 4; cellH: 2; gap: 1; sourceLive }`**, public, one `ShaderEffect`:
    - `readonly live` as `BlockGraph`'s; columns fit `cellW` into the width with the leftover on the left, newest on the right;
    - the axis is at `floor(height / 2)`; upper row r spans `[axis − gap − (r + 1)·cellH − r·gap, axis − gap − r·(cellH + gap)]`, lower row r starts at `axis + gap + r·(cellH + gap)` (mockup `mirror`, which has `gap = 1`);
    - uniforms: `itemSize`, `firstColumnX`, `columnWidth`, `gapSize`, `cellHeight`, `axisY`, `columnCount`, `halfRowCount`, `skippedColumns`, `pixelRatio`, `unlitColor`, `upColor` (`hi` at alpha 1), `downColor` (`signal` at alpha 1), `columnWord0…47`, `rowWord0…10`.
  - **`HeatGraph { rows; lo: 0; hi: 100; warnAt: Theme.loadWarn; critAt: Theme.loadCrit; cellW: 4; gap: 1; sourceLive }`**, public: an `Item` holding a `Repeater { model: rows.length }` of internal `HeatRow`s, row `i` at `y = i·(heatRowHeight + gapSize)`, top row first. `rows` is an array of series.
  - **`HeatRow`** (internal), one `ShaderEffect` per row: uniforms `itemSize`, `firstColumnX`, `columnWidth`, `gapSize`, `columnCount`, `skippedColumns`, `pixelRatio`, `unlitColor` (6 %), `rampColor` (`signal` at alpha 1), `warnColor`, `critColor`, `columnWord0…31`. The shader colours a byte 0 as `unlitColor`, 254 as `warnColor`, 255 as `critColor`, and any other byte b as `rampColor·b/255` (premultiplied).

- [ ] **Step 1: Record the baseline**

In a headless KWin (P10), at `QT_SCALE_FACTOR=1.15`, with a release `ghostd run --fake` on its own socket and the history full (wait 320 s, as the spike's `run.sh` does):
- `GHOST_VIEW=gallery` without a lanes page: `scripts/measure-cpu.sh <qs pid> 60` and `smaps_rollup` Rss, after C21's settle.

- [ ] **Step 2: Write the failing tests**

```js
// segments.test.mjs
test('mirror cells', () => { assert.equal(mirrorCellColor(true, 0, 3, 10, THEME), withAlpha(THEME.hi, 45)); assert.equal(mirrorCellColor(false, 5, 6, 10, THEME), withAlpha(THEME.signal, 70)); assert.equal(mirrorCellColor(true, 7, 3, 10, THEME), withAlpha(THEME.signal, 5)); });
test('heat cells', () => { assert.equal(heatCellColor(0.5, 'ok', THEME), withAlpha(THEME.signal, 47)); assert.equal(heatCellColor(0.95, 'crit', THEME), THEME.crit); assert.equal(heatCellColor(NaN, 'ok', THEME), withAlpha(THEME.signal, 6)); });
test('alpha bytes read back from withAlpha', () => { for (const r of [0, 3, 9]) { assert.equal(mirrorAlphaByte(false, r, 10), byteOf(withAlpha(THEME.signal, 45 + 50 * r / 10))); } assert.equal(heatAlphaByte(0), 18); assert.equal(heatAlphaByte(1), 222); });
```

```js
// mirrorgraph.test.mjs — unpacks with blockgraph.test.mjs's f32 fieldIn
test('max is the floor or the largest sample, with headroom', () => { assert.ok(Math.abs(mirrorMax([10, null], [40], 50) - 55) < 1e-9); assert.ok(Math.abs(mirrorMax([100], [0], 50) - 110) < 1e-9); });
test('square-root scale', () => { assert.equal(mirrorLit(25, 100, 10), 5); assert.equal(mirrorLit(1, 100, 10), 3); assert.equal(mirrorLit(0, 100, 10), 0); assert.equal(mirrorLit(null, 100, 10), 0); assert.equal(mirrorLit(200, 100, 10), 10); });
test('half rows fit above the axis gap', () => { assert.equal(fittedHalfRows(40, 2, 1), 6); assert.equal(drawnHalfRows(200), 63); });
test('a column packs up and down lit counts', () => { const w = mirrorColumnWords([0, 100], [100, 25], 2, 10, { max: 100, live: true }); assert.deepEqual(unpackColumn(w, 0), { up: 0, down: 10 }); assert.deepEqual(unpackColumn(w, 1), { up: 10, down: 5 }); });
test('a short series fills from the right; not live lights nothing', () => { const w = mirrorColumnWords([50], [50], 4, 10, { max: 100, live: true }); assert.deepEqual([0, 1, 2].map((c) => unpackColumn(w, c)), Array(3).fill({ up: 0, down: 0 })); assert.equal(mirrorColumnWords([50], [50], 1, 10, { max: 100, live: false }).flat().every((v) => v === 0), true); });
test('row words carry both ramps', () => { const w = mirrorRowWords(10); assert.equal(rowByte(w, 0), mirrorAlphaByte(true, 0, 10)); assert.equal(rowByte(w, 63 + 9), mirrorAlphaByte(false, 9, 10)); assert.equal(rowByte(w, 10), 0); });
test('limits and exact components', () => { assert.equal(MAX_COLUMNS, 384); assert.equal(COLUMN_WORDS + ROW_WORDS, 59); /* every component of a 384 × 63 full graph is an integer below 2^24 that f32 keeps */ });
```

```js
// heatgraph.test.mjs
test('cell bytes', () => { const g = { lo: 0, hi: 100, warnAt: 0.75, critAt: 0.9, live: true }; assert.equal(heatCellByte(50, g), heatAlphaByte(0.5)); assert.equal(heatCellByte(80, g), CELL_WARN); assert.equal(heatCellByte(95, g), CELL_CRIT); assert.equal(heatCellByte(null, g), CELL_MISSING); assert.equal(heatCellByte(50, { ...g, live: false }), CELL_MISSING); });
test('ok bytes never collide with the codes', () => { for (let f = 0; f < 0.75; f += 0.001) { const b = heatAlphaByte(f); assert.ok(b > CELL_MISSING && b < CELL_WARN, String(f)); } });
test('a row fills from the right and skips past 384', () => { const w = heatRowWords([10, 20], 3, G); assert.deepEqual([0, 1, 2].map((c) => unpackByte(w, c)), [CELL_MISSING, heatAlphaByte(0.1), heatAlphaByte(0.2)]); assert.equal(unpackByte(heatRowWords(Array(400).fill(50), 400, G), 0), heatAlphaByte(0.5)); });
test('row height', () => assert.equal(heatRowHeight(70, 16, 1), 55 / 16));
```

(`withAlpha(signal, 6)` rather than a literal because `heatCellColor` must reuse `UNLIT_ALPHA_PCT`; `byteOf`, `unpackColumn`, `rowByte` and `unpackByte` are the tests' own f32 unpackers, as in `blockgraph.test.mjs:13–30`.)

- [ ] **Step 3: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL (missing exports and modules); every existing test still passes.

- [ ] **Step 4: Implement the colour rules and the packing, then run the tests**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: PASS.

- [ ] **Step 5: Implement the shaders, the QML and the gallery page**

- `GHOST_GALLERY_LANES=mixed`: six full-width lanes 1256 × 70 design px, 6 px apart: two `BlockGraph` (4 × 3), two `MirrorGraph`, two 16-row `HeatGraph`, fed at 1 Hz by a synthetic series (the gallery's own Timer, only while the page is shown). `heat`: six 16-row `HeatGraph`s. `fixed`: one of each kind on a fixed series (every level, nulls, empty and full columns, a mirror column at 25 % and one at 100 %) that writes each graph's `grabToImage` with an explicit size to `$GHOST_GALLERY_GRAB_DIR`.
- Run: `make shaders && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t1.sock scripts/qml-smoke.sh && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t1.sock make smoke-layout`, both inside the headless KWin.
- Expected: `smoke: ok` twice; no `ShaderEffect` or `qsb` warning in the shell log.

- [ ] **Step 6: Check the pixels**

On the `fixed` page at scale 1.15: for each listed cell (the top and bottom lit and the first unlit cell of every fixed column, both mirror halves, every heat code), `magick <grab> -format '%[pixel:p{X,Y}]' info:` at the cell's centre, computed from the layout above, equals the node colour function's premultiplied colour within 1/255. A view at the mockup's `u` against the mockup's `L-mon` lanes looks the same (record a montage).

- [ ] **Step 7: Measure**

Same setup as Step 1, 60 s each, `mixed` and `heat` pages, three runs each. Targets for `mixed` (user-confirmed 2026-10-21):
- page creation ≤ 100 ms;
- Rss ≤ 30 MiB above Step 1;
- shell CPU ≤ 0.30 % above Step 1;
- exactly one rendered frame per second, after C21's settle.

Report both pages per kind, with the CPU and Rss delta against Step 1, for README, so Task 7 can predict MONITOR's reading (P8).

- [ ] **Step 8: Commit**

```bash
git add shell/kit/GhostKit/lib/blockgraph.mjs shell/kit/GhostKit/lib/segments.mjs shell/kit/GhostKit/lib/mirrorgraph.mjs shell/kit/GhostKit/lib/heatgraph.mjs shell/kit/GhostKit/MirrorGraph.qml shell/kit/GhostKit/HeatGraph.qml shell/kit/GhostKit/HeatRow.qml shell/kit/GhostKit/shaders/mirrorgraph.frag shell/kit/GhostKit/shaders/heatrow.frag shell/kit/GhostKit/qmldir shell/tests/segments.test.mjs shell/tests/mirrorgraph.test.mjs shell/tests/heatgraph.test.mjs
git commit -m "feat(kit): add shader mirror and heat graphs"
git add shell/host/Gallery.qml
git commit -m "feat(shell): add the gallery's lane pages"
```

---

### Task 2: `procs` and `procs.tree` topics, split out of `mem`

**Files:**
- Create:
  - `crates/ghostd/src/collect/procs.rs`, holding the process scan moved out of `mem.rs`, its constants and its tests;
  - `crates/ghostd/src/fake/{mod.rs,sampled.rs,feeds.rs}`, from `fake.rs` (743 lines; no behaviour change, its own commit first; this is M9's daemon half);
  - `crates/ghostd/tests/procs.rs`;
  - `scripts/topic-tap.py`, a UI client that prints the `data` messages of the given topics for N seconds, for every later live check.
- Modify:
  - `crates/ghostd/src/collect/{mem.rs,mod.rs}`, `crates/ghost-proto/src/{snapshot.rs,lib.rs}`, `crates/ghostd/src/sampler.rs`, `crates/ghostd/src/main.rs` (`host_sampler`, `probe procs`);
  - `crates/ghostd/tests/{mem.rs,common/mod.rs}` (`FakeProcess::ppid`), `crates/ghost-proto/tests/wire.rs`;
  - `widgets/memory/{manifest.toml,Widget.qml}`.

**Interfaces:**
- Produces:
  - `topics::PROCS = "procs"`, `topics::PROCS_TREE = "procs.tree"`, both added to `BUILTIN`.
  - `MemSnapshot` loses `top`. This is a wire change, called out in its commit body.
  - `ProcsSnapshot { top: Vec<ProcUsage> }`.
  - `ProcTree { kernel_threads: u32, procs: Vec<ProcNode> }`.
  - `ProcNode { pid: u32, ppid: u32, name: String, cpu: f32, rss_kib: u64 }`. `cpu` is the share since the previous scan.
  - `pub struct ProcSample { pub top: Vec<ProcUsage>, pub tree: Option<ProcTree> }`.
  - `ProcCollector::new(root: &SysRoot, top_n: usize) -> Self`, which implements `Source<ProcSample>`:
    - it scans every `PROCESS_SCAN_EVERY = 2` samples, as `mem` did;
    - it builds `tree` on scans 0, 5, 10, … (`TREE_EVERY_SCANS = 5`, so every 10 s; spec: 5–20 s; user-confirmed 2026-10-21);
    - kernel threads are counted, not listed;
    - processes with RSS 0 are in the tree but not in `top`;
    - `resume()` re-baselines, as `MemCollector::resume` did.
  - `Sampler::new(cpu, mem, sensors, gpu, disks, procs)`, where `procs` is `Measure::SincePrevious`. It is sampled when `procs` or `procs.tree` is wanted; `procs` publishes `top`, and `procs.tree` publishes when wanted and `tree` is `Some`.
  - `scripts/topic-tap.py <socket> <seconds> <topic>…` prints one JSON line per `data` message.

- [ ] **Step 1: Move `fake.rs` to `fake/` and commit it alone**

Run: `cargo test --workspace`
Expected: PASS, unchanged counts.

```bash
git add crates/ghostd/src/fake.rs crates/ghostd/src/fake
git commit -m "refactor(ghostd): split the fakes into sampled and feed modules"
```

- [ ] **Step 2: Write the failing tests**

```rust
#[test] fn the_tree_comes_on_every_fifth_scan() { let root = fake_9950x3d(); root.write_process(&FakeProcess::new(1, "systemd").rss_kib(100)); let mut c = ProcCollector::new(&root.root(), 6); let with_tree: Vec<usize> = (1..=11).filter_map(|i| c.sample().tree.map(|_| i)).collect(); assert_eq!(with_tree, [1, 11]); }
#[test] fn the_tree_keeps_rss_free_processes_and_counts_kernel_threads() { /* 1 systemd rss 100; 2 kthreadd kernel thread; 5 zypak-sandbox rss 0 ppid 1 */ let t = first_tree(..); assert_eq!(t.kernel_threads, 1); assert_eq!(pids(&t), [1, 5]); assert_eq!(node(&t, 5).ppid, 1); }
#[test] fn top_leaves_out_rss_free_processes() { /* same machine, 5 busy */ assert!(c.sample().top.iter().all(|p| p.pid != 5)); }
#[test] fn a_recycled_pid_starts_its_share_from_zero() { /* pid 7 exits, pid 7 reappears with a later start_time and high ticks */ assert_eq!(node(&tree, 7).cpu, 0.0); }
#[test] fn procs_and_tree_publish_only_when_wanted() {
    /* a Sampler over a counting ProcCollector, driven for 11 ticks (scans on ticks 1, 3, …, 11) per demand */
    assert_eq!(published(&run(&["procs"]), "procs.tree"), 0);
    assert!(published(&run(&["procs"]), "procs") > 0);
    assert_eq!(published(&run(&["procs.tree"]), "procs"), 0);
    assert_eq!(published(&run(&["procs.tree"]), "procs.tree"), 2); // scans 1 and 11
    assert_eq!(run(&[]).scans(), 0); // neither wanted: the source is never sampled
}
```

```rust
// ghost-proto tests/wire.rs
#[test] fn mem_no_longer_carries_top() { assert!(!serde_json::to_string(&MemSnapshot::default()).unwrap().contains("top")); }
```

The existing `top` tests in `tests/mem.rs` move to `tests/procs.rs` unchanged.

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p ghostd --test procs --test mem && cargo test -p ghost-proto --test wire`
Expected: FAIL (no `ProcCollector`).

- [ ] **Step 4: Implement**

- The scan reads `ppid`, field 4 of `/proc/<pid>/stat`.
- The memory widget's manifest says `requires = ["mem", "procs"]`, and the widget reads `Data.topic("procs").value.top`.

- [ ] **Step 5: Run the tests and the smokes**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t2.sock make smoke` (the smoke inside the headless KWin).
Expected: PASS, then `smoke: ok`.

- [ ] **Step 6: Measure**

- Release daemon on its own socket, with `topic-tap.py` subscribed to `procs.tree` for 60 s.
- Report:
  - scan wall and CPU time, before (Plan 2a figure) and after;
  - tree serialisation size in bytes;
  - the daemon's CPU % with `[procs]` and with `[procs, procs.tree]`.
- Crop the memory panel from a V1 snapshot taken at this task's base commit (before Step 2; no Plan 2a crop was saved, P18), then again after the change. `compare -metric AE` between the two crops gives 0.
- Report the shell's and the daemon's CPU and Rss delta.

- [ ] **Step 7: Commit**

```bash
git add crates scripts/topic-tap.py widgets/memory
git commit -m "feat(ghostd): publish the process list and tree as their own topics"
```

---

### Task 3: History for cores, memory, disk I/O and temperatures

**Files:**
- Modify:
  - `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/collect/cpu.rs` (topology from `cpuN/topology/thread_siblings_list`), `crates/ghostd/src/sampler.rs`, `crates/ghostd/src/server/state.rs`;
  - `crates/ghostd/src/fake/sampled.rs`, `crates/ghostd/tests/{cpu.rs,server_handshake.rs}`, `crates/ghostd/tests/common/mod.rs` (`fake_9950x3d` writes `cpuN/topology/thread_siblings_list` for every cpu, pairing cpu `i` with `i + 16`; the fake root has no topology files today, P15);
  - `shell/kit/GhostKit/lib/history.mjs`, `shell/tests/history.test.mjs`.

**Interfaces:**
- Produces:
  - `CpuSnapshot.cores: Vec<CoreLoad>`, with `CoreLoad { cpus: Vec<u16>, load: f32 }`: one entry per physical core, ordered by its lowest cpu id; `load` is the mean of its threads (spec §6.2: "SMT pairs averaged").
  - **`sampler::history_topic(key: &str) -> Option<&'static str>`** replaces `HISTORY_KEYS`. It maps every key in Global Constraints to its topic. `core<N>` matches any decimal `N`, and `net_*` returns `net` (its recorder arrives in Task 5).
  - `State::subscribe` sends every series whose `history_topic` is subscribed.
  - Recorders:
    - `temp_cpu` is the first reading whose id is in `CPU_PACKAGE_SENSORS = ["k10temp/Tctl", "coretemp/Package id 0"]`;
    - `temp_gpu` is `gpu.temp_c`, `None` while the GPU is absent;
    - `temp_nvme` is the maximum `temp_c` over drives with `state == DriveState::Live` whose `kernel_name` starts with `nvme`, or `None`;
    - `mem_used` is `used_kib / 1048576`;
    - `disk_read`/`disk_write` sum `read_bps`/`write_bps` over drives with `state == DriveState::Live`. `history.mjs` applies the same test (`state === 'live'`), so the two series stay identical.
  - **`history.mjs`:**
    - `topicOfKey(key) -> string | null` mirrors `history_topic`;
    - `withData` records the same samples for `cpu`, `gpu`, `mem`, `sensors`, `disks` (and `net` from Task 5);
    - `fromHistory` marks a topic as replaying when any of its keys has samples.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn cores_average_their_smt_siblings() { /* fake_9950x3d: cpu0 load 10, cpu16 load 30 */ assert_eq!(snapshot.cores[0], CoreLoad { cpus: vec![0, 16], load: 20.0 }); assert_eq!(snapshot.cores.len(), 16); }
#[test] fn every_key_has_one_topic() { for (k, t) in [("load","cpu"),("core15","cpu"),("gpu_load","gpu"),("temp_gpu","gpu"),("mem_used","mem"),("temp_cpu","sensors"),("disk_read","disks"),("temp_nvme","disks"),("net_up","net")] { assert_eq!(history_topic(k), Some(t)); } assert_eq!(history_topic("corex"), None); }
#[test] fn temp_cpu_is_recorded_only_when_sensors_is_published() {
    /* fake_9950x3d with k10temp/Tctl at 58 °C; the sampler is ticked once per demand */
    let disks_only = tick_wanting(&["disks"]);
    assert_eq!(disks_only.sensors_sampled(), true); // drive temperatures need it
    assert_eq!(disks_only.history("temp_cpu"), Vec::<Option<f32>>::new());
    assert_eq!(tick_wanting(&["sensors"]).history("temp_cpu"), [Some(58.0)]);
}
#[test] fn temp_nvme_is_the_hottest_live_nvme() {
    /* drives: nvme0 live 38, nvme1 Dead with temp_c None, sda live 30 */
    assert_eq!(tick_with(&[nvme("nvme0", Live, Some(38.0)), nvme("nvme1", Dead, None), sata("sda", Live, Some(30.0))]).history("temp_nvme"), [Some(38.0)]);
    assert_eq!(tick_with(&[nvme("nvme1", Dead, None), sata("sda", Live, Some(30.0))]).history("temp_nvme"), [None]);
}
#[test] fn a_subscriber_to_mem_gets_mem_used_history() {
    /* server_handshake: after three ticks, subscribe to [mem] */
    assert_eq!(history_keys(&reply), ["mem_used"]);
    assert_eq!(reply.series("mem_used").len(), 3);
}
```

```js
test('every key maps to one topic', () => { assert.equal(topicOfKey('core3'), 'cpu'); assert.equal(topicOfKey('mem_used'), 'mem'); assert.equal(topicOfKey('nope'), null); });
test('cpu data appends one sample per core', () => { const t = withData(fromHistory({}), { topic: 'cpu', seq: 1, data: { load: 10, ccds: [], cores: [{ cpus: [0, 16], load: 20 }] } }, 300); assert.deepEqual(t.series.core0, [20]); });
test('disks data sums live drives and keeps the hottest nvme', () => { const drive = (name, state, read_bps, temp_c) => ({ kernel_name: name, state, read_bps, write_bps: 0, temp_c }); const t = withData(fromHistory({}), { topic: 'disks', seq: 1, data: { drives: [drive('nvme0n1', 'live', 1e6, 38), drive('nvme1n1', 'live', 2e6, 41), drive('nvme2n1', 'dead', 9e6, null)] } }, 300); assert.deepEqual([t.series.disk_read, t.series.temp_nvme], [[3e6], [41]]); });
test('a gap before mem data is filled with nulls', () => { const at = (seq, used_kib) => ({ topic: 'mem', seq, data: { used_kib } }); const t = withData(withData(fromHistory({}), at(1, 1048576), 300), at(4, 2097152), 300); assert.deepEqual(t.series.mem_used, [1, null, null, 2]); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement**

The server's History message now carries about 30 series × 300 samples.

- [ ] **Step 4: Run every check, then measure the History message**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && node --test 'shell/tests/*.test.mjs' && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t3.sock make smoke` (the smoke inside the headless KWin).
Expected: PASS and `smoke: ok`.

Report two figures:
- the History line size after 5 minutes of every topic;
- the shell's CPU for one resend (C8: every want/release resends it).

- [ ] **Step 5: Commit**

```bash
git add crates shell/kit/GhostKit/lib/history.mjs shell/tests/history.test.mjs
git commit -m "feat(ghostd): record per-core, memory, disk and temperature history"
```

---

### Task 4: Per-split `gutter`

**Files:**
- Modify:
  - `crates/ghost-proto/src/{config.rs,layout.rs,node_de.rs}`, `crates/ghostd/src/widgets/resolve.rs`;
  - `crates/ghost-proto/tests/layout_node.rs`, `crates/ghostd/tests/config_load.rs`;
  - `shell/kit/GhostKit/lib/layout.mjs`, `shell/tests/layout.test.mjs`.

**Interfaces:**
- Produces:
  - `LayoutNode::Split` and `ResolvedNode::Split` gain `gutter: Option<f32>`. TOML key `gutter`, next to `direction`, `size` and `children`.
  - It spaces **that split's own children only**. Nested splits fall back to `layout.gutter`, not to their parent's override.
  - A negative or non-finite value is a config error, validated through `check_non_negative("<node path>.gutter", g)` like the node's `size`: `layout.views.v9.gutter: must be a non-negative number, got -1`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_split_reads_its_gutter() { let n: LayoutNode = toml::from_str("direction='column'\ngutter=6\nchildren=[{widget='a'},{widget='b'}]").unwrap(); assert!(matches!(n, LayoutNode::Split { gutter: Some(g), .. } if g == 6.0)); }
#[test] fn a_negative_gutter_names_its_node() { let e = load_str_with_user("[layout.views.v9]\ndirection='row'\ngutter=-1\nchildren=[]", "u").unwrap_err(); assert_eq!(e.message, "layout.views.v9.gutter: must be a non-negative number, got -1"); }
```

```js
test('a split gutter spaces only its own children', () => { const tree = { direction: 'column', gutter: 6, children: [{ widget: 'a' }, { direction: 'row', children: [{ widget: 'b' }, { widget: 'c' }] }] }; const r = computeRects(tree, { x: 0, y: 0, w: 100, h: 106 }, 12); assert.equal(pick(r, 'a').h, 50); assert.equal(pick(r, 'b').y, 56); assert.equal(pick(r, 'c').x, 56); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghost-proto --test layout_node && cargo test -p ghostd --test config_load && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check**

Run the same commands plus `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t4.sock make smoke-layout` (inside the headless KWin).
Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Commit**

```bash
git add crates shell/kit/GhostKit/lib/layout.mjs shell/tests/layout.test.mjs
git commit -m "feat(config): let a layout split set its own gutter"
```

---

### Task 5: `net` sampled topic (rates) and its history

**Files:**
- Create: `crates/ghostd/src/collect/net.rs`, `crates/ghostd/tests/net.rs`
- Modify:
  - `crates/ghostd/src/collect/{mod.rs,disk.rs}` (move `bytes_per_second` to `mod.rs` as `pub(crate)`), `crates/ghost-proto/src/{snapshot.rs,lib.rs}`;
  - `crates/ghostd/src/{sampler.rs,main.rs,fake/sampled.rs}`;
  - `shell/kit/GhostKit/lib/history.mjs`, `shell/tests/history.test.mjs`.
- Fixtures: `crates/ghostd/tests/fixtures/proc-net-dev.txt` and `proc-net-route.txt`, copied from fx (two header lines and leading spaces before the names in `/proc/net/dev`; the default route is `enp14s0` at metric 100, and `tun0` has split routes only).

**Interfaces:**
- Produces:
  - `topics::NET = "net"`, added to `BUILTIN`.
  - `NetSnapshot { default_iface: Option<String>, rx_bps: u64, tx_bps: u64, interfaces: Vec<NetIface> }` and `NetIface { name: String, oper_up: bool, rx_bps: u64, tx_bps: u64 }`.
    - The headline `rx_bps`/`tx_bps` belong to the default-route interface. That is the `/proc/net/route` entry with destination and mask `00000000`, the `RTF_UP` flag and the lowest metric.
    - `lo` is skipped.
    - `oper_up` is `sys/class/net/<if>/operstate == "up"`, or `"unknown"` with `IFF_UP` (bit `0x1`) set in `sys/class/net/<if>/flags`: tunnels and loopback report `unknown` while carrying traffic (P27, fx C2).
  - `NetCollector::new(root: &SysRoot) -> Self` implements `Source<NetSnapshot>` (`Measure::SincePrevious`). `sample_after(&mut self, elapsed: Duration) -> NetSnapshot` exists for tests, as on `DiskCollector`.
    - A counter that decreased reads 0 for that sample; that is a reset, not a wrap.
    - An interface that is new since the previous sample reads 0.
  - The sampler records `net_down = rx_bps` and `net_up = tx_bps`. `history.mjs` mirrors that.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn rates_follow_the_default_route() { /* enp14s0 rx 0→125_000_000 in 1 s, tun0 idle, default route enp14s0 */ assert_eq!(s.default_iface.as_deref(), Some("enp14s0")); assert_eq!(s.rx_bps, 125_000_000); }
#[test] fn the_lowest_metric_default_route_wins() { /* tun0 metric 50, enp14s0 metric 100 */ assert_eq!(s.default_iface.as_deref(), Some("tun0")); }
#[test] fn a_counter_that_goes_backwards_reads_zero_not_a_spike() { /* rx 9e9 → 1e3 */ assert_eq!(iface(&s, "enp14s0").rx_bps, 0); }
#[test] fn an_interface_that_appears_reads_zero_first() { /* tun0 absent then present with 5e9 bytes */ assert_eq!(iface(&s, "tun0").rx_bps, 0); }
#[test] fn no_default_route_leaves_the_headline_at_zero() { assert_eq!((s.default_iface, s.rx_bps, s.tx_bps), (None, 0, 0)); }
#[test] fn a_tunnel_reporting_unknown_but_up_is_up() { /* tun0 operstate `unknown`, flags `0x1091` */ assert!(iface(&s, "tun0").oper_up); }
#[test] fn a_tunnel_reporting_unknown_with_up_clear_is_down() { /* tun0 operstate `unknown`, flags `0x1090` */ assert!(!iface(&s, "tun0").oper_up); }
```

```js
test('net data records down and up', () => { const t = withData(fromHistory({}), { topic: 'net', seq: 1, data: { rx_bps: 10, tx_bps: 2, interfaces: [] } }, 300); assert.deepEqual([t.series.net_down, t.series.net_up], [[10], [2]]); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test net && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && node --test 'shell/tests/*.test.mjs'`
Expected: PASS.

- [ ] **Step 4: Live check and measure**

Run `ghostd probe net` (add `net` to `probe`). It prints `enp14s0` as the default interface.

Report the wall and CPU time per sample, and the daemon's CPU % with `[net]` for 60 s.

- [ ] **Step 5: Commit**

```bash
git add crates shell/kit/GhostKit/lib/history.mjs shell/tests/history.test.mjs
git commit -m "feat(ghostd): add network rates and their history"
```

---

### Task 6: `net.links` feed (NetworkManager, Tailscale, ping)

**Files:**
- Create:
  - `crates/ghostd/src/feeds/links/{mod.rs,nm.rs,ping.rs}` and `crates/ghostd/src/feeds/tailscale.rs` (shared with Task 10);
  - `crates/ghostd/tests/links.rs`, `crates/ghostd/tests/fixtures/tailscale-status.json` (fx/tailscale-status.json: a synthetic direct, online `phone` and a real-shaped offline, relayed `laptop`, with addresses in documentation ranges; P29).
- Modify:
  - `crates/ghost-proto/src/{snapshot.rs,lib.rs,config.rs}`, `crates/ghostd/src/config/default.toml`, `crates/ghostd/src/config/load.rs`;
  - `crates/ghostd/Cargo.toml` (the `nix` features gain `socket` for the ICMP socket, and `uio` if `recvmsg` is used; ghostd's features are `feature, fs, hostname, signal` today) and `Cargo.lock` (P17);
  - `crates/ghostd/src/{main.rs,feeds/mod.rs,fake/feeds.rs}`, `crates/ghostd/tests/config_load.rs`.

**Interfaces:**
- Produces:
  - `topics::NET_LINKS = "net.links"`, added to `BUILTIN`.
  - The snapshot types:
    - `LinksSnapshot { lan: Vec<LanLink>, vpns: Vec<VpnLink>, tailscale: Option<TailscaleLink>, ping_ms: Option<f32> }`;
    - `LanLink { iface: String, address: Option<String> /* "192.168.15.5/24" */, speed_mbps: Option<u32>, state: LinkState }`;
    - `VpnLink { name: String /* NM connection id */, kind: String /* "openvpn", "wireguard" */, iface: Option<String>, state: LinkState }`;
    - `TailscaleLink { address: Option<String>, peers_online: u32, peers_total: u32, state: LinkState }`;
    - `enum LinkState { Up, Connecting, Down }`, snake_case on the wire.
  - **Config:** `[network] ping = "1.1.1.1"`. `""` disables ping. Anything else must be an IPv4 literal: `network.ping: "x" is not an IPv4 address`.
  - **`feeds::tailscale`:**
    - `TailscaleClient::new(socket: PathBuf)`;
    - `async fn status(&self) -> Option<TailscaleStatus>`: HTTP/1.0 `GET /localapi/v0/status` with `Host: local-tailscaled.sock`, a 2 s timeout (user-confirmed 2026-10-21), and `None` when the socket is missing or refused;
    - `TailscaleStatus { backend_state: String, self_ips: Vec<IpAddr>, peers: Vec<TailscalePeer> }`, where `peers` is built from the values of the status's `Peer` object (a JSON object keyed by node key; the keys are ignored);
    - `TailscalePeer { host_name: String, online: bool, cur_addr: Option<SocketAddr> }`, where `CurAddr == ""` (no direct path) or an unparsable value is `None` (P29, fx C3).
  - **`feeds::links::ping`:** `trait Pinger: Send + Sync { fn ping(&self, host: Ipv4Addr) -> BoxFuture<Option<Duration>> }`. `IcmpPinger` opens an unprivileged `SOCK_DGRAM`/`IPPROTO_ICMP` socket via `nix` in `tokio::io::unix::AsyncFd`, waits 2 s (user-confirmed 2026-10-21), and returns `None` on any error. (`net.ipv4.ping_group_range` is `0 2147483647` here, so gid 1000 may ping unprivileged.)
  - **`feeds::links`:**
    - `feed() -> Feed` (system bus, `/run/tailscale/tailscaled.sock`, `IcmpPinger`);
    - `feed_on(bus: zbus::Connection, tailscale: TailscaleClient, pinger: Arc<dyn Pinger>) -> Feed` for tests.
  - **Behaviour, all of it only while `net.links` is wanted (`until_wanted`/`until_unwanted`; nothing runs otherwise):**
    - **LAN:** NM `Devices` of type ethernet (1) or wifi (2) that NM manages. State 100 → `Up`, 40–90 → `Connecting`, anything else → `Down`. The address is `Ip4Config.AddressData[0]`. The speed is `Device.Wired.Speed`, read on wired devices only; an `UnknownInterface` error reads as `None`.
    - **VPNs:** every saved connection of type `vpn` or `wireguard` (`Settings.ListConnections` + `GetSettings`; `kind` is the last segment of `vpn.service-type`, or `wireguard`), merged with the active connection's state: 2 → `Up`, 1 → `Connecting`, otherwise `Down`. `iface` is the `Interface` of the device whose `Device.Ip4Config` equals the VPN active connection's `Ip4Config`, else `None` (`Devices[0]` is the parent NIC, not the tunnel; P28, fx C1). Type `tun` (Tailscale and other external tunnels) is excluded.
    - NM is re-read on its `PropertiesChanged` (ActiveConnections, Devices), on an active connection's `StateChanged`, and in full when `org.freedesktop.NetworkManager` gains an owner.
    - **Tailscale** is read every 10 s (user-confirmed 2026-10-21). `BackendState "Running"` → `Up`, `"Starting"` → `Connecting`, anything else → `Down`. `address` is the first `self_ips` entry.
    - **Ping** runs every 5 s.
    - It publishes only when the snapshot changes.
  - The fake publishes the mockup's links: `ENP14S0 192.168.15.5/24 1 GbE UP`, `OpenVPN Flxpoint tun0` up, Tailscale `100.122.159.93` with 0 of 1 peers online, and ping 12.
  - **M5 (its own commit first, P12):** `validate_i18n`'s neighbour in `config/load.rs` rejects `i18n.from` other than `pt-BR` while `location.alerts.provider = "inmet"`: `i18n.from: "en-US" must be "pt-BR" while location.alerts.provider is "inmet"` (INMET writes only pt-BR, and the provider otherwise translates nothing). With no provider the existing tag rules stand.

- [ ] **Step 1: Write the failing tests**

These run on the private dbus-daemon harness (`tests/common/bus.rs`) with a fake NM object server (zbus `#[interface]`) that mirrors fx/nm.json (the VPN active connection's `Devices[0]` is `enp14s0`; `tun0` has its own `tun`-type active connection and the same `Ip4Config` path as the VPN's), a fake Tailscale `UnixListener` serving the fixture, and a counting fake `Pinger`.

```rust
#[tokio::test] async fn lan_and_vpn_links_come_from_network_manager() { assert_eq!(snap.lan, [LanLink { iface: "enp14s0".into(), address: Some("192.168.15.5/24".into()), speed_mbps: Some(1000), state: LinkState::Up }]); assert_eq!(snap.vpns[0].kind, "openvpn"); assert_eq!(snap.vpns[0].iface.as_deref(), Some("tun0")); }
#[tokio::test] async fn a_saved_vpn_that_is_not_active_reads_down() { assert_eq!(vpn(&snap, "Work").state, LinkState::Down); }
#[tokio::test] async fn tun_connections_are_not_vpns() { /* NM lists tailscale0 as type tun */ assert!(snap.vpns.iter().all(|v| v.iface.as_deref() != Some("tailscale0"))); }
#[tokio::test] async fn a_vpn_whose_ip_config_matches_no_device_has_no_iface() { /* the fake's tun0 device carries another Ip4Config path */ assert_eq!(vpn(&snap, "OpenVPN Flxpoint").iface, None); }
#[tokio::test] async fn a_device_without_the_wired_interface_has_no_speed() { /* the fake answers UnknownInterface for Device.Wired */ assert_eq!(snap.lan[0].speed_mbps, None); }
#[tokio::test(start_paused = true)] async fn network_manager_appearing_later_is_read_in_full() { /* the fake NM claims its name 5 s after the feed starts */ assert!(before.lan.is_empty() && before.vpns.is_empty()); assert_eq!((after.lan.len(), after.vpns.len()), (1, 2)); }
#[tokio::test(start_paused = true)] async fn network_manager_absent_publishes_empty_lists_and_keeps_running() { /* no NM on the bus; 30 s pass */ assert!(snap.lan.is_empty() && snap.vpns.is_empty()); assert!(!feed_task.is_finished()); assert!(pinger.count() >= 5); }
#[tokio::test] async fn tailscale_status_counts_online_peers() { assert_eq!((t.peers_online, t.peers_total, t.state), (1, 2, LinkState::Up)); }
#[tokio::test] async fn a_missing_tailscale_socket_reads_none() { assert!(snap.tailscale.is_none()); }
#[tokio::test(start_paused = true)] async fn nothing_runs_while_unwanted() { /* demand never contains net.links; 60 s pass */ assert_eq!((nm.calls(), tailscale.requests(), pinger.count()), (0, 0, 0)); }
#[tokio::test(start_paused = true)] async fn an_empty_ping_host_never_pings() { assert_eq!(pinger.count(), 0); assert!(snap.ping_ms.is_none()); }
#[test] fn network_ping_must_be_an_ipv4_address_or_empty() { assert!(load_str_with_user("[network]\nping='x'", "u").unwrap_err().to_string().contains("network.ping: \"x\" is not an IPv4 address")); }
#[test] fn inmet_alerts_translate_only_from_pt_br() { // config_load.rs, committed on its own first
    let inmet = "[location.alerts]\nprovider = 'inmet'\narea = '3550308'\n";
    assert_eq!(rejected(&format!("{inmet}[i18n]\nfrom = 'en-US'\nto = 'pt-BR'\n")), "i18n.from: \"en-US\" must be \"pt-BR\" while location.alerts.provider is \"inmet\"");
    assert!(load_str_with_user(&format!("{inmet}[i18n]\nfrom = 'pt-BR'\nto = 'en-US'\n"), "u").is_ok());
    assert!(load_str_with_user("[i18n]\nfrom = 'en-US'\nto = 'pt-BR'\n", "u").is_ok());
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test links --test config_load`
Expected: FAIL (M5's test and the `links` tests; every other test passes).

- [ ] **Step 3: Implement, then run every check**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Live check and measure**

This step reads NM on the **system** bus and the Tailscale socket, and pings 1.1.1.1 (spec §12). NM is read-only through properties and `GetSettings`, which returns no secrets.

Run a dev daemon on its own socket with a temp config, plus `topic-tap.py` for 30 s on `net.links`. Expected:
- `enp14s0` is up with its address;
- `OpenVPN Flxpoint` shows its real state;
- Tailscale shows `100.122.159.93`;
- `ping_ms` is a number.

Report the CPU per NM re-read, per Tailscale request and per ping, and the daemon's CPU % with `[net.links]` over 60 s.

- [ ] **Step 5: Commit**

M5's validation and its test land first, as their own commit:

```bash
git add crates/ghostd/src/config/load.rs crates/ghostd/tests/config_load.rs
git commit -m "fix(config): reject an i18n source other than pt-BR while INMET supplies the alerts"
git add crates
git commit -m "feat(ghostd): add network links, Tailscale state and ping"
```

---

### Task 7: `telemetry-lane` widget

**Files:**
- Create: `widgets/telemetry-lane/{manifest.toml,Widget.qml,lanes.mjs}`, `shell/tests/lanes.test.mjs`
- Modify:
  - `shell/kit/GhostKit/lib/format.mjs` and `Fmt.qml` (`rateShort`; re-read at base, and leave 2a's code in them as it is);
  - `shell/tests/format.test.mjs`.
- Not touched: `Glyph.qml`. The widget uses `Glyph { name: "chevron-up" }` and `"chevron-down"`, which already carry the mockup's `IC.up` and `IC.down` paths (`Glyph.qml:14–21`, P13).

**Interfaces:**
- Consumes: `HeatGraph` and `MirrorGraph` (Task 1, shaders) and the shader `BlockGraph` (2a Task 15b); the history keys (Tasks 3 and 5); `net.links` (Task 6).
- Produces:
  - **Manifest:**
    - `id = "telemetry-lane"`, `name = "Telemetry lane"`, `kinds = ["panel"]`;
    - `requires = ["cpu", "gpu", "mem", "sensors", "disks", "net", "net.links"]`: one static union, so a lane's history keeps growing while another view is shown (the `requiresOf` rule). All of these are subscribed in every default view anyway;
    - setting `metric`: enum `cpu | gpu | mem | net | disk | therm`, default `"cpu"`.
  - **`format.mjs`:** `rateShort(bps)` uses `rate`'s 1000-byte units: `"25K"` below 1 MB/s (rounded), `"1.2M"` above, and `MISSING` for missing (mockup `short`).
  - **`lanes.mjs`** (strings from the mockup's `step()`):
    - `laneValue(metric, d) -> string`:
      - cpu: `52% · 58°`;
      - gpu: `9% · 41°`;
      - mem: `19.8 G`;
      - net: `{ down: '25K', up: '2K' }`, which the widget draws with chevrons;
      - disk: `R5 W2` (MB/s, rounded);
      - therm: `58° / 41°`.
    - `laneSub(metric, d, spanS) -> string`:
      - cpu: `HEAT · 16 CORES · ~150 S`;
      - gpu: `LOAD`;
      - mem: `/ 91.9 GiB`;
      - net: `VPN 2 UP` (the widget prefixes the `DOWN · UP` chevron labels);
      - disk: `READ · WRITE`;
      - therm: `CPU · GPU · NVME`.
    - `vpnUpCount(links)` counts `Up` VPNs, plus Tailscale when it is `Up`.
    - `spanSeconds(columns, stepMs)`.
  - **Widget:** a `Panel { header: false }` laid out like the mockup's `.lane`:
    - a label block 132 wide with padding 6/12:
      - the name at 9.5 px, weight 600, tracking .2 em, `hi`, uppercase, with its tag (`signal`, tracking .04 em, weight 500);
      - the value at 21 px bold `hi`, in `Digits`;
      - the sub line at 8.5 px `dim`, tracking .06 em;
    - the graph with padding 8 top and bottom, 14 right:
      - cpu: `HeatGraph`, rows `core0…coreN`;
      - gpu: `BlockGraph` (cellW 4, cellH 3, minLit 1);
      - mem: `BlockGraph` (0…total GiB, cellW 4, cellH 3);
      - net: `MirrorGraph` (up `net_up`, down `net_down`, floor `NET_FLOOR_BPS = 50_000`);
      - disk: `MirrorGraph` (up `disk_write`, down `disk_read`, floor `DISK_FLOOR_BPS = 20_000_000`);
      - therm: three `BlockGraph`s 2 px apart (`temp_cpu`, `temp_gpu`, `temp_nvme`; lo 20, hi 100, cellH 2, minLit 1, `warnAt = (Theme.tempWarn − 20) / 80`, `critAt = (Theme.tempCrit − 20) / 80`).
    - The tag keys are `cpu`, `gpu`, `mem`, `net`, `disk` and `sens` (mockup). The names are `CPU`, `GPU`, `MEMORY`, `NETWORK`, `DISK I/O` and `THERMALS`.

- [ ] **Step 1: Write the failing tests**

```js
test('lane values', () => { assert.equal(laneValue('cpu', { cpu: { load: 52.4 }, tctl: 58.2 }), '52% · 58°'); assert.equal(laneValue('mem', { mem: { used_kib: 20761804 } }), '19.8 G'); assert.equal(laneValue('disk', { read: 5.4e6, write: 2.1e6 }), 'R5 W2'); assert.equal(laneValue('therm', { tctl: 58, gpuT: 41 }), '58° / 41°'); });
test('lane subs', () => { assert.equal(laneSub('cpu', { cores: 16 }, 150), 'HEAT · 16 CORES · ~150 S'); assert.equal(laneSub('mem', { mem: { total_kib: 96364134 } }), '/ 91.9 GiB'); assert.equal(laneSub('net', { links: LINKS_TWO_UP }), 'VPN 2 UP'); });
test('missing data reads --', () => { assert.equal(laneValue('gpu', { gpu: { present: false } }), '--'); });
test('rateShort', () => { assert.equal(rateShort(25400), '25K'); assert.equal(rateShort(1234000), '1.2M'); assert.equal(rateShort(null), '--'); });
test('span', () => assert.equal(spanSeconds(150, 1000), 150));
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement**

- [ ] **Step 4: Run the tests, the smoke and a MONITOR snapshot**

1. Write `/tmp/gs-2b-t7.toml`: the default MONITOR with `placeholder{Telemetry}` replaced by `{ direction = "column", gutter = 6, children = [six telemetry-lane slots] }`.
2. Run `node --test 'shell/tests/*.test.mjs' && GHOST_CONFIG=/tmp/gs-2b-t7.toml GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t7.sock scripts/qml-smoke.sh` (headless).
   Expected: PASS and `smoke: ok`.
3. Snapshot MONITOR with `--fake` at the mockup's `u` and compare it with the mockup's `L-mon` lanes. Nothing overflows.
4. Measure default MONITOR with the six lanes, headless, defaults, 60 s, three runs (CPU of shell and daemon, `smaps_rollup` Rss, view creation time), and report the CPU and Rss delta against 2a's MONITOR figure (0.76 % combined still, daemon 0.48–0.53 %). Above 1.00 % combined, stop and report the numbers: the controller asks the user (P8, Q1). Nothing is slowed or re-budgeted here. This lifts C14 for lanes.

- [ ] **Step 5: Commit**

```bash
git add widgets/telemetry-lane shell/kit/GhostKit/lib/format.mjs shell/kit/GhostKit/Fmt.qml shell/tests/lanes.test.mjs shell/tests/format.test.mjs
git commit -m "feat(widgets): add the telemetry lane widget"
```

---

### Task 8: `network` widget (V2 panel and V1 scope)

**Files:**
- Create: `widgets/network/{manifest.toml,Widget.qml,network.mjs}`, `shell/tests/network.test.mjs`

**Interfaces:**
- Consumes: `net`, `net.links`, `MirrorGraph`, `Glyph up/down`, `Chip`, `Fmt.rate`.
- Produces:
  - **Manifest:** `id = "network"`, `requires = ["net", "net.links"]`, setting `style`: enum `panel | scope`, default `"panel"`.
  - **`network.mjs`:**
    - `metaLine(net, links)` → `ENP14S0 · 1 GbE`: the default interface uppercased, plus its speed as `GbE` at ≥ 1000 Mb/s (`10 GbE` at 10000) or `<n> Mb/s` below. With no default interface it returns `--`.
    - `linkRows(links)` → `[{ name, detail, state }]`:
      - `{ name: 'ENP14S0', detail: '192.168.15.5/24 · 1 GbE', state: 'UP' }`;
      - `{ name: 'OPENVPN', detail: 'Flxpoint · tun0', state: 'ACTIVE' }`: the name is `kind` uppercased, and the detail is the connection name without a leading `kind` word (case-insensitive), then `· iface`;
      - `{ name: 'TAILSCALE', detail: '100.122.159.93 · 0/1 peers', state: 'UP' }`.
      - States read `UP`/`ACTIVE` for `Up` (`ACTIVE` for VPNs), `CONNECTING`, and `DOWN` in `dim`.
    - `pingText(ms)` → `12 ms` or `--`.
    - `vpnLine(links)` → `OPENVPN · FLXPOINT`, the first `Up` VPN, or `--`.
    - `tsLine(links)` → `0/1 PEERS`, or `--`.
  - **Widget:**
    - `panel` (mockup `C.net`, V2 164 high): a header `Network`, tag `net`, meta `metaLine`; the `MirrorGraph` at 40 px (cellW 3); a 4-column kv row (up chevron + rate, down chevron + rate, `PING`, chip `online` when the default interface is up); then the link rows.
    - `scope` (mockup `C.scope`'s lower half, headerless): the `MirrorGraph` at 40 px, the kv row (chip `live`), and a 2-column kv row (`VPN` `vpnLine`, `TS` `tsLine`).
  - The scope style's natural height at mockup sizes goes in the task report. Task 18 uses it as the V1 slot size and pins it in a layout test.

- [ ] **Step 1: Write the failing tests**

```js
test('meta line', () => { assert.equal(metaLine(NET, LINKS), 'ENP14S0 · 1 GbE'); assert.equal(metaLine({ default_iface: null }, LINKS), '--'); });
test('link rows match the mockup', () => assert.deepEqual(linkRows(LINKS), [{ name: 'ENP14S0', detail: '192.168.15.5/24 · 1 GbE', state: 'UP' }, { name: 'OPENVPN', detail: 'Flxpoint · tun0', state: 'ACTIVE' }, { name: 'TAILSCALE', detail: '100.122.159.93 · 0/1 peers', state: 'UP' }]));
test('a down vpn and no tailscale', () => { const rows = linkRows({ lan: [], vpns: [{ name: 'Work', kind: 'openvpn', iface: null, state: 'down' }], tailscale: null }); assert.deepEqual(rows, [{ name: 'OPENVPN', detail: 'Work', state: 'DOWN' }]); });
test('scope lines', () => { assert.equal(vpnLine(LINKS), 'OPENVPN · FLXPOINT'); assert.equal(tsLine(LINKS), '0/1 PEERS'); assert.equal(pingText(null), '--'); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run the tests and a smoke with both styles**

Run `GHOST_CONFIG=<temp config> GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t8.sock scripts/qml-smoke.sh` (headless). The temp config places `network` 164 in V2's right column, and `network {style="scope"}` under a placeholder in V1's right column. Expected: PASS and `smoke: ok`.

Snapshot both and compare them with the mockup's `p-net` and `C.scope`.

- [ ] **Step 4: Commit**

```bash
git add widgets/network shell/tests/network.test.mjs
git commit -m "feat(widgets): add the network widget in panel and scope styles"
```

---

### Task 8b: Hold and want harness (FR M6)

Plan 2b adds many `FrameClock` holders and `Data.want` claims (the globe, the silicon packets, the process tree, the uplink), and a leaked hold costs about 0.5 % of a core in the still state, which only a manual measurement would notice. This task commits the harness the budgets rest on, before Task 9 adds the first of them.

**Files:**
- Create:
  - `scripts/headless-kwin.sh`, copied from `.superpowers/sdd/2026-10-21-ghost-shell-plan-2-dashboard/headless-kwin.sh` (git-ignored there). Only its `repo=` line changes, to `$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)`, because the script now sits one level below the repo root instead of three;
  - `scripts/smoke-holds.sh`.
- Modify:
  - `Makefile` (a `smoke-holds` target, listed in `.PHONY`, depending on `shaders`);
  - `shell/kit/GhostKit/FrameClock.qml` and `shell/kit/GhostKit/Data.qml` (their counts become readable under `GHOST_DEBUG=1` only).

**Interfaces:**
- Produces:
  - **`FrameClock`**, under `GHOST_DEBUG=1`: every acquire or release of the normal, fast or seconds hold logs `qml: holds normal=<n> fast=<n> seconds=<n>`. With the flag off nothing is logged and no extra property is read.
  - **`Data`**, under the same flag: every `want()` or `release()` logs `qml: wants {"<topic>":<count>,…}` (topics with a count above 0).
  - **`scripts/smoke-holds.sh`** (`make smoke-holds`), built the way `scripts/qml-smoke.sh` and `scripts/measure-covered.sh` are: its own headless KWin (`scripts/headless-kwin.sh`) and a private bus (`scripts/private-bus.conf`), a debug `ghostd --fake` on its own socket, and the shell with `GHOST_DEBUG=1`. `GHOST_CONFIG` selects the config (default: the built-in one), `GHOST_SMOKE_SOCKET` the socket, and the shell's own variables (`GHOST_VIEW`, `GHOST_GALLERY_*`) pass through, so a gallery page can be cycled too. After the shell has connected and C21's settle has passed it records the baseline counts, then runs these phases, each of which ends on the state it started from:
    1. cover, then uncover, the dashboard, reported the way the KWin script reports it (`measure-covered.sh` shows how);
    2. every view swap and back (`GHOST_SMOKE_ACTIONS`' mechanism);
    3. every tab of every tabs node, then back to the first;
    4. a live style change of a widget that has one (`network`'s `style`, once it exists), then back.

    After each phase it settles for 2 s and asserts that the latest `holds` and `wants` lines equal the baseline. It prints `holds: ok` and a table of the counts per phase, or `holds: FAIL (<phase>: <line> where the baseline is <line>)` with a non-zero exit. It stops the shell, the daemon and the KWin by their pids.

- [ ] **Step 1: Write the harness and watch it fail**

Write `scripts/smoke-holds.sh`, `scripts/headless-kwin.sh` and the Makefile target, without the logging.
Run: `make smoke-holds`
Expected: FAIL: no `holds` line was ever logged.

- [ ] **Step 2: Add the logging, then run the harness**

Run: `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t8b.sock make smoke-holds`
Expected: `holds: ok`, every phase back at the baseline.

- [ ] **Step 3: Prove that it can fail**

Comment out one `FrameClock.release()` (or `Data.release(…)`) in a widget the default config shows, run `make smoke-holds`, and expect `holds: FAIL` naming the phase. Restore the line, run it again for `holds: ok`, and check that `git diff --stat` lists only this task's files.

- [ ] **Step 4: Run the existing checks**

Run: `node --test 'shell/tests/*.test.mjs' && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t8b.sock make smoke`
Expected: PASS and `smoke: ok`, with no `holds` or `wants` line in the shell log (the flag is off).

- [ ] **Step 5: Commit**

```bash
git add scripts/headless-kwin.sh scripts/smoke-holds.sh Makefile shell/kit/GhostKit/FrameClock.qml shell/kit/GhostKit/Data.qml
git commit -m "test(shell): check that frame-clock holds and topic wants return to baseline"
```

Tasks 9, 12, 13, 15 run `make smoke-holds` (with `GHOST_CONFIG` set to their temp config) before committing; Tasks 9 and 13 do their click checks on this harness's headless KWin (P12).

---

### Task 9: `process-tree` widget

**Files:**
- Create: `widgets/process-tree/{manifest.toml,Widget.qml,tree.mjs}`, `shell/tests/tree.test.mjs`

**Interfaces:**
- Consumes: `procs.tree` (Task 2), `ClickArea` (2a Task 15, R13), `Theme`, `Panel`, `Data.want`/`Data.release`.
- Produces:
  - **Manifest:** `id = "process-tree"`, `requires = []`, `animates = false`. The mockup tree has no pulses. The widget calls `Data.want("procs.tree")` while it is visible and `Data.release("procs.tree")` when hidden or destroyed, as the clock does for weather: `requiresOf` (`lib/subscriptions.mjs:16–25`) subscribes every view's and every tab's `requires`, so a `requires` entry would build the tree every 10 s whenever the dashboard is visible, in any view (P6).
  - **`tree.mjs`** (constants named after the mockup's `buildTree`/`drawTree`):
    - **`buildTree(tree, collapsed: Set<string>) -> { root, nodes }`:**
      - nodes are keyed `"<pid>:<name>"`, so a recycled pid is a different node;
      - the root is pid 1, or a synthetic `"?"` root when pid 1 is absent;
      - a process whose `ppid` is missing from the table attaches to the root;
      - one synthetic child of the root, `kthreadd (<kernel_threads>)` with key `kthreads`, stands for every kernel thread;
      - a collapsed node keeps its own entry, hides its descendants, and carries `hidden: <count>`;
      - children are sorted by `name` (`localeCompare`).
    - **`layoutRadial(built, w, h)`** sets `angle`, `radius`, `x`, `y`:
      - leaf-weighted spans from −π/2 to 3π/2;
      - `R = min(w, h) / 2 − (min(w, h) < 300 ? 14 : 40)`;
      - `r = R · (depth / maxDepth)^0.75`.
    - **`dotSize(rssKib) = 1.6 + log10(1 + rssKib / 1024) · 1.1`.**
    - **`dotLevel(cpu)`:** `crit` above 20, `warn` above 5, `hi` above 1, else `ok` (`signal` at 55 %).
    - **`showsLabel(node, R)`:** `R < 120 ? cpu > 8 : (depth == 1 && leaves ≥ 3) || cpu > 4 || (depth == 2 && leaves ≥ 12)`. A collapsed node always shows its label as `<name> +<hidden>`.
    - **`labelText(node)`:** ` <name> <cpu>% ` when cpu > 1, else ` <name> `.
    - **`hitTest(nodes, x, y, slopPx) -> key | null`:** the nearest node within `max(dotSize, slopPx)`.
    - **`pruneCollapsed(collapsed, built) -> Set`:** drops keys no longer present.
  - **Widget:**
    - header `Process tree`, tag `tree`, meta `<n> PROCS`;
    - edges are **one** `Shape` with a `PathQuad` per edge (control point at `(cx + cos(a_child)·r_parent, cy + sin(a_child)·r_parent)`), `signal` at 20 %, width 0.6;
    - dots are a `Repeater` of `Rectangle`s;
    - labels are rotated `Text`s at 8 px weight 600 with `textFormat: Text.PlainText` (process names are foreign text, C6(b)), flipped when `cos(angle) < 0`;
    - the root is a 6 px `signal` square labelled `<NAME> · PID <pid>`;
    - all of it is rebuilt only on new `procs.tree` data, a collapse toggle or a resize;
    - the drawing area is one `ClickArea`; a click toggles `hitTest`'s node in the widget's own `collapsed` set (kept for the session, not persisted);
    - every node starts expanded.

- [ ] **Step 1: Write the failing tests**

```js
test('kernel threads collapse into one child of the root', () => { const b = buildTree({ kernel_threads: 212, procs: [P(1, 0, 'systemd')] }, new Set()); assert.equal(b.root.kids.find((k) => k.key === 'kthreads').name, 'kthreadd (212)'); });
test('an orphan attaches to the root', () => { const b = buildTree({ kernel_threads: 0, procs: [P(1, 0, 'systemd'), P(9, 4242, 'lost')] }, new Set()); assert.ok(b.root.kids.some((k) => k.pid === 9)); });
test('a collapsed node hides and counts its descendants', () => { const b = buildTree(TREE_3_DEEP, new Set(['2081:systemd'])); assert.equal(node(b, '2081:systemd').hidden, 4); assert.equal(b.nodes.has('2118:bash'), false); });
test('collapse state drops a vanished pid and survives a rebuild', () => { const s = pruneCollapsed(new Set(['2081:systemd', '777:gone']), buildTree(TREE_3_DEEP, new Set())); assert.deepEqual([...s], ['2081:systemd']); });
test('a recycled pid is a different node', () => assert.notEqual(key(P(7, 1, 'a')), key(P(7, 1, 'b'))));
test('radial layout puts the root at the centre and leaves on the rim', () => { const b = layoutRadial(buildTree(TREE_3_DEEP, new Set()), 400, 400); assert.deepEqual([b.root.x, b.root.y], [200, 200]); assert.equal(Math.round(deepest(b).radius), 160); });
test('dot size and level', () => { assert.equal(dotSize(0), 1.6); assert.equal(dotLevel(21), 'crit'); assert.equal(dotLevel(5), 'hi'); assert.equal(dotLevel(0.5), 'ok'); });
test('a small tree labels only busy nodes', () => { assert.equal(showsLabel({ cpu: 9, depth: 3, leaves: 1 }, 100), true); assert.equal(showsLabel({ cpu: 5, depth: 1, leaves: 9 }, 100), false); });
test('a thousand processes lay out once in under 20 ms (user-confirmed 2026-10-21)', () => { const t0 = performance.now(); layoutRadial(buildTree(SYNTHETIC_1000, new Set()), 600, 500); assert.ok(performance.now() - t0 < 20); });
test('hit test picks the nearest node within the slop', () => { assert.equal(hitTest(NODES, 203, 201, 6), '1:systemd'); assert.equal(hitTest(NODES, 50, 50, 6), null); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement**

- [ ] **Step 4: Run the tests, a smoke and the click check**

1. Run `node --test 'shell/tests/*.test.mjs'`, `GHOST_CONFIG=<temp> GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t9.sock scripts/qml-smoke.sh` and `GHOST_CONFIG=<temp> GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t9.sock make smoke-holds` (Task 8b) with a temp config that puts `process-tree` in MONITOR's right rail. `smoke-holds` must show the `procs.tree` want back at its baseline after every phase (the want is released on hide, on cover and on destruction).
2. Snapshot it with `--fake`, which has the 40-process synthetic tree, and compare it with the mockup's `p-tree`.
3. Run the click check on Task 8b's harness ("Plan 1 R52", in the headless KWin): a click on a node collapses it and a second click expands it.
4. Report the rebuild time per `procs.tree` message.
5. Repeat Task 7's MONITOR reading (headless, defaults, 60 s, three runs; CPU of shell and daemon, `smaps_rollup` Rss) with the tree in the right rail, and report the CPU and Rss delta against Task 7's. Above 1.00 % combined, stop and report the numbers: the controller asks the user (P8, Q1). Nothing is slowed or re-budgeted here.

- [ ] **Step 5: Commit**

```bash
git add widgets/process-tree shell/tests/tree.test.mjs
git commit -m "feat(widgets): add the traversable process tree"
```

---

### Task 10: `uplink` feed (sockets, GeoIP, labels, tailnet peers)

**Files:**
- Create:
  - `crates/ghostd/src/feeds/uplink/{mod.rs,sockets.rs,geo.rs,owners.rs}`;
  - `crates/ghostd/tests/uplink.rs`, `crates/ghostd/tests/fixtures/proc-net-{tcp,tcp6,udp,udp6}.txt` (fx/proc-net-*.txt, anonymised to documentation ranges, P30);
  - `scripts/fetch-geoip.sh`.
- Modify:
  - `Cargo.toml` and `crates/ghostd/Cargo.toml` (`maxminddb`, features `["mmap"]`);
  - `crates/ghost-proto/src/{snapshot.rs,lib.rs,config.rs}`, `crates/ghostd/src/config/{default.toml,load.rs}`;
  - `crates/ghostd/src/{main.rs,feeds/mod.rs,fake/feeds.rs}`, `crates/ghostd/tests/config_load.rs`.

**Interfaces:**
- Consumes: `feeds::tailscale::TailscaleClient` (Task 6), whose `peers` come from the status's `Peer` object and whose `cur_addr` is `None` for an empty `CurAddr`.
- Produces:
  - `topics::UPLINK = "uplink"`, added to `BUILTIN`.
  - `UplinkSnapshot { endpoints: Vec<Endpoint>, total: u32, attribution: String }`:
    - `attribution` is `""` when no city database is loaded;
    - `total` counts endpoints before the top-8 cut.
  - `Endpoint { label: String, lat: f32, lon: f32, sockets: u32, tailnet: bool }`.
  - **Config:**
    - `[uplink] geoip_dir = ""`, where `""` means `$XDG_DATA_HOME/ghost-shell/geoip`;
    - `[uplink.labels]` maps a key to a value. A key is an organisation name, matched case-insensitively, or `AS<number>`. A value is fixed text, or `"process"`;
    - an empty key or value is an error naming `uplink.labels`.
  - **`sockets.rs`:**
    - `fn parse_proc_net(text: &str, v6: bool, udp: bool) -> Vec<RemoteSocket>`;
    - `RemoteSocket { remote: IpAddr, uid: u32, inode: u64 }`;
    - TCP keeps state `01`; UDP keeps state `01` (connected) with a non-zero remote;
    - an IPv4-mapped IPv6 address becomes IPv4;
    - `fn is_public(ip: &IpAddr) -> bool` excludes loopback, unspecified, RFC 1918, link-local, CGNAT `100.64.0.0/10`, multicast, broadcast, documentation (`192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`, `2001:db8::/32`), ULA `fc00::/7` and `fe80::/10`. An IPv4-mapped IPv6 address is judged as its IPv4 address.
    - `trait SocketSource: Send + Sync { fn sockets(&self) -> Vec<RemoteSocket> }`, with `ProcNetSockets` reading the four tables for production: the feed takes its sockets from an injected source (a constructor parameter beside `Geo`), so its tests can use public addresses such as 1.1.1.1 and 9.9.9.9 (the fixtures cannot pass `is_public`).
  - **`geo.rs`:**
    - `trait Geo: Send + Sync`, with `fn city(&self, ip: IpAddr) -> Option<(f32, f32)>` and `fn asn(&self, ip: IpAddr) -> Option<(u32, String)>`;
    - `MmdbGeo::open(dir: &Path) -> MmdbGeo` maps `dbip-city-lite.mmdb` and `dbip-asn-lite.mmdb` (`Reader::open_mmap`);
    - the city falls back to `/usr/share/GeoIP/GeoLite2-City.mmdb`;
    - a file is reopened when its mtime changes;
    - `attribution()` returns `IP GEOLOCATION BY DB-IP`, `GEOLITE2 DATA BY MAXMIND` (the fallback) or `""`.
  - **`owners.rs`:** `Owners::new(root: SysRoot)`, with `fn names(&mut self, wanted: &[(u64, u32)]) -> HashMap<u64, String>`:
    - it maps inodes to processes via `readlink /proc/<pid>/fd/*` (`socket:[<inode>]`), only for processes whose uid matches;
    - it stops as soon as every wanted inode is found;
    - the cache keeps inode → `comm` and is pruned to the current inodes.
  - **`label_for(org: Option<&(u32, String)>, overrides, owner: Option<&str>, ip) -> String`.** The override is looked up by `AS<n>`, then by the organisation name:
    - fixed text is used as is;
    - `"process"` uses the owner's `comm`, uppercased, and falls back to the organisation when no owner is found;
    - with no override it is the organisation uppercased;
    - then `AS<n>`;
    - then the IP.
  - **Feed `uplink`, every 5 s while wanted:**
    1. parse the four tables;
    2. keep the public remotes;
    3. look up each remote's city and ASN;
    4. label each remote;
    5. group them by `(label, city)` into endpoints with a socket count; an endpoint with no city is counted in `total` and not shown;
    6. add each Tailscale peer that is online with a `cur_addr` as `TAILNET:<HOST_NAME uppercased>`, `tailnet: true`;
    7. sort by `sockets` descending, then by `label`;
    8. keep `MAX_ENDPOINTS = 8` (user-confirmed 2026-10-21);
    9. publish on change.

    Nothing is read while the topic is unwanted.
  - **`scripts/fetch-geoip.sh`:**
    - downloads `https://download.db-ip.com/free/dbip-{city,asn}-lite-YYYY-MM.mmdb.gz` for the current month, falling back to the previous month on a 404;
    - un-gzips them into `$XDG_DATA_HOME/ghost-shell/geoip/` through a temp file and `mv`;
    - prints the sizes;
    - never runs from `install.sh`.
  - `uplink::feed() -> Feed` (production sources) and `uplink::feed_on(sockets: Arc<dyn SocketSource>, geo: Arc<dyn Geo>, owners: Owners, tailscale: TailscaleClient) -> Feed` for tests (label overrides come from the config the feed context carries).
  - The fake publishes the mockup's six `PEERS`, labels included.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn established_tcp_and_connected_udp_are_kept() { let s = parse_proc_net(TCP_FIXTURE, false, false); assert_eq!(s.len(), 4); assert_eq!(s[0], RemoteSocket { remote: "192.0.2.10".parse().unwrap(), uid: 1000, inode: 17_634_145 }); }
#[test] fn a_v4_mapped_v6_remote_reads_as_v4() { assert!(parse_proc_net(TCP6_FIXTURE, true, false).iter().any(|s| s.remote == "203.0.113.9".parse::<IpAddr>().unwrap())); }
#[test] fn private_and_special_ranges_are_never_shown() { for ip in ["127.0.0.1","10.1.2.3","172.16.0.1","192.168.15.5","169.254.1.1","100.122.159.93","224.0.0.1","192.0.2.10","2001:db8::1","fd7a:115c::1","fe80::1","::1","::ffff:10.1.2.3","::ffff:192.168.15.5"] { assert!(!is_public(&ip.parse().unwrap()), "{ip}"); } assert!(is_public(&"1.1.1.1".parse().unwrap())); assert!(is_public(&"::ffff:1.1.1.1".parse().unwrap())); }
#[test] fn an_override_by_asn_beats_one_by_name() { assert_eq!(label_for(Some(&(16509, "Amazon.com, Inc.".into())), &ovr(&[("AS16509", "AWS"), ("amazon.com, inc.", "process")]), None, ip()), "AWS"); }
#[test] fn process_overrides_fall_back_to_the_org() { assert_eq!(label_for(Some(&(20940, "Akamai International B.V.".into())), &ovr(&[("AKAMAI INTERNATIONAL B.V.", "process")]), None, ip()), "AKAMAI INTERNATIONAL B.V."); assert_eq!(label_for(Some(&(20940, "Akamai International B.V.".into())), &ovr(&[("akamai international b.v.", "process")]), Some("spotify"), ip()), "SPOTIFY"); }
#[test] fn the_label_chain_ends_at_the_ip() { assert_eq!(label_for(None, &ovr(&[]), None, "9.9.9.9".parse().unwrap()), "9.9.9.9"); }
#[test] fn owners_are_found_through_fd_links_and_only_for_the_socket_uid() { /* FakeRoot: proc/42/fd/3 -> socket:[5123456] owned by 1000, proc/43 owned by 0 with the same link */ assert_eq!(owners.names(&[(5_123_456, 1000)])[&5_123_456], "spotify"); }
#[tokio::test(start_paused = true)] async fn endpoints_group_rank_and_cut_at_eight() {
    /* injected sockets: 12 organisations, organisation k with k sockets, each in its own city */
    assert_eq!((snap.endpoints.len(), snap.total), (8, 12));
    assert_eq!(snap.endpoints.iter().map(|e| e.sockets).collect::<Vec<_>>(), [12, 11, 10, 9, 8, 7, 6, 5]);
}
#[tokio::test(start_paused = true)] async fn a_direct_tailnet_peer_is_located_and_a_relayed_one_is_not() { assert!(snap.endpoints.iter().any(|e| e.tailnet && e.label == "TAILNET:PHONE")); assert_eq!(snap.endpoints.iter().filter(|e| e.tailnet).count(), 1); }
#[tokio::test(start_paused = true)] async fn no_database_still_publishes() { assert_eq!((snap.endpoints.len(), snap.attribution.as_str()), (0, "")); }
#[tokio::test(start_paused = true)] async fn nothing_is_read_while_unwanted() { /* demand never contains uplink; 60 s pass */ assert_eq!((geo.lookups(), sockets.reads()), (0, 0)); }
#[test] fn uplink_labels_reject_empty_values() { assert!(load_str_with_user("[uplink.labels]\n\"AS1\"=''", "u").unwrap_err().to_string().contains("uplink.labels")); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test uplink --test config_load`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Run `scripts/fetch-geoip.sh`, then check live and measure**

The script downloads two files from download.db-ip.com. The user approved running it on 2026-10-21.

Then run a dev daemon with a temp config that sets `uplink.labels` to one real organisation → `"process"`, plus `topic-tap.py` on `uplink` for 30 s. Report:
- the endpoints seen;
- per-cycle wall and CPU time, without and with the override;
- the fd-walk time and how many fds it read;
- the daemon's `smaps_rollup` Rss with the databases mapped, after 10 minutes of `topic-tap.py` on `uplink`, against the same daemon without it (only the pages a lookup touches become resident, and they count, R50; P7).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates scripts/fetch-geoip.sh
git commit -m "feat(ghostd): publish live connection endpoints with offline geolocation"
```

---

### Task 11: Land mask, lattice map and the shader globe

**Files:**
- Create:
  - `scripts/land-mask.py`;
  - `widgets/uplink/land-mask.png`, committed: 2048 × 1024, 1-bit, equirectangular, 180° W at the left, 90° N at the top (unchanged, Decision 2);
  - `widgets/uplink/globe-lattice.png`, committed: **1024 × 512** RGB (no alpha, no `gAMA`/`iCCP`/`sRGB` chunk), equirectangular like the mask; each texel holds the index of the lattice point nearest its centre and that point's land bit (P23). The size is user-confirmed (2026-10-21);
  - `shell/kit/GhostKit/Globe.qml` (public), `shell/kit/GhostKit/lib/globe.mjs`;
  - `shell/kit/GhostKit/shaders/globe.frag`, `shell/kit/GhostKit/shaders/arc.vert`, `shell/kit/GhostKit/shaders/arc.frag`;
  - `shell/tests/globe.test.mjs`.
- Modify:
  - `Makefile` (`%.vert.qsb: %.vert`, and `.vert` sources in `shaders`; P5), `scripts/install.sh` (refuse a missing `.vert.qsb` as for `.frag`);
  - `shell/kit/GhostKit/qmldir` (`Globe 1.0`);
  - `shell/host/Gallery.qml` (`GHOST_GALLERY_GLOBE=<lattice png path>`: the globe at V2-centre and V1-scope sizes with the fake's six endpoints and São Paulo home, holding `FrameClock` while shown).

**Interfaces:**
- Produces:
  - **`scripts/land-mask.py`**, run as `uv run --no-project --with pyshp --with pillow --with numpy python -I scripts/land-mask.py --mask <mask.png> --lattice <lattice.png>`:
    - downloads `https://naciscdn.org/naturalearth/50m/physical/ne_50m_land.zip` into a new temporary directory and checks a SHA-256 recorded in the script;
    - rasterises the polygons with pyshp and Pillow `ImageDraw.polygon` into the mask (unchanged from the plan);
    - computes `LATTICE_POINTS = 3200` points by the mockup's formula (`y = 1 − 2i/(N − 1)`, `θ = i·π(3 − √5)`), each point's land bit from the mask at the point, and for each lattice-map texel centre the nearest point (numpy, in chunks);
    - writes the texel as R = index mod 256, G = ⌊index / 256⌋ + 16·land, B = 0;
    - `--check <mask.png> <lattice.png>` (no network) asserts:
      - the plan's six points: land at (−23.55, −46.63), (51.5, −0.12), (−25.0, 135.0); sea at (−25.0, −44.5), (0.0, −30.0), (35.0, −40.0);
      - for every lattice point j, the texel containing j decodes to j, with j's land bit;
      - the PNG is 8-bit RGB without the three colour chunks, so its bytes reach the shader unchanged.
  - **`lib/globe.mjs`** (mockup `drawGlobe` constants, named and cited there): `LATTICE_POINTS = 3200`, `ROT_BASE = 0.6`, `ROT_PER_MS = 0.00009`, `TILT = 0.38`, `SMALL_BELOW = 240`, `MARGIN_SMALL = 10`, `MARGIN = 30`, `DOT_MIN = 1.3`, `DOT_PER_R = 1 / 70`, `LAND_ALPHA_FLOOR = 0.2`, `LAND_ALPHA_SPAN = 0.75`, `SEA_ALPHA_SPAN = 0.07`, `RING_ALPHA = 0.2`, `RING_WIDTH = 0.8`, `ARC_SEGMENTS = 40`, `ARC_LIFT_BASE = 0.08`, `ARC_LIFT_SPAN = 0.22`, `ARC_ALPHA = 0.5`, `PACKET_PER_MS = 0.00025`, `PACKET_STAGGER = 0.37`, `PACKET = 3`, `END = 4`, `HOME = 5`, `PULSE_MS = 1800`, `PULSE_FROM = 6`, `PULSE_GROW = 16`, `MAX_ARCS = 8` (= Task 10's `MAX_ENDPOINTS`); and:
    - `fibonacciPoint(i, n) -> { lat, lon }`, `unitVector({ lat, lon }) -> [x, y, z]`, `view(v, rot, tilt)`, `inverseView(v, rot, tilt)`, `slerp(a, b, t) -> { point, omega }`, `arcAltitude(omega, i, segments)` = 1 + (0.08 + 0.22·omega/π)·sin(π·i/segments);
    - `packetSegment(elapsedMs, k)` = ⌊((elapsedMs·0.00025 + k·0.37) mod 1)·40⌋;
    - `decodeLattice(r, g) -> { index, land }` (bytes, as the shader decodes them);
    - `LATTICE_NEIGHBOUR_OFFSETS`: the index offsets the shader also tests around the texel's point, derived by the coverage test below (Fibonacci numbers, both signs), not chosen;
    - `coveringIndex(px, py, frame, nearestAt)`: the shader's selection in JavaScript — the nearest point from `nearestAt(lat, lon)` plus the offsets, keeping the highest index whose square (land: `dot`, sea: `dot / 2`, centred on its projection, front hemisphere only) contains the pixel — so the node test can hold the shader's logic to a brute force;
    - `globeFrame({ width, height, unit, elapsedMs, hasHome, home, endpoints }) -> { small, radius, dot, rot, arcs: [{ home, end, omega }], ends: [{ x, y, visible }], packets: [{ x, y, visible }], home: { x, y, visible, pulse }, anchors }`: per frame, at most 2·`MAX_ARCS` + 1 projections. `anchors` is `labelAnchors`: one `{ x, y, visible }` per endpoint, then home. Visibility is the mockup's `q.z > 0 || hypot(q.x, q.y) > 1`.
  - **`globe.frag`** (dots and outline), uniforms: `itemSize`, `centreX`, `centreY`, `radius`, `rot`, `tilt`, `dot`, `ringWidth`, `pixelRatio`, `latticePoints`, `landAlphaFloor`, `landAlphaSpan`, `seaAlphaSpan`, `ringAlpha`, `dotColor` (`signal` at alpha 1); `sampler2D lattice` at binding 1, from a hidden `Image { smooth: false; mipmap: false }`. Per fragment:
    1. the outline: `signal` at 20 % within `ringWidth / 2` of radius `R + 1u`, with a one-pixel coverage edge;
    2. inside the disc, the view point (dx, dy, √(1 − dx² − dy²)) → `inverseView` → latitude and longitude → one `texture()` fetch → `decodeLattice`;
    3. the candidates are that index and each `index ± offset`, **unrolled** (GLSL ES 100 has no array constructors), each skipped outside `[0, latticePoints)`; a candidate's position comes from the Fibonacci formula, its land bit from one more fetch at its own position;
    4. the pixel takes the highest-index candidate whose screen square contains it: land `signal` at (0.2 + 0.75·z), sea `signal` at 0.07·z, premultiplied.
  - **`arc.vert` / `arc.frag`** (one `ShaderEffect` per arc, `mesh: GridMesh { resolution: Qt.size(ARC_SEGMENTS, 1) }`); both stages declare the **same** uniform block: `qt_Matrix`, `qt_Opacity`, `centreX`, `centreY`, `radius`, `rot`, `tilt`, `home` and `end` (`vector4d`, w unused), `omega`, `segments`, `liftBase`, `liftSpan`, `halfWidth`, `feather`, `arcColor` (`hi` at 50 %). The vertex shader takes `t = qt_MultiTexCoord0.x` and the side from `.y`; it places `slerp(home, end, t)·arcAltitude` through `view` and the projection, offset along the screen normal (from `t ± 1/segments`) by `±(halfWidth + feather)`, and passes `across` and a per-vertex `visible` (mockup rule). The fragment shader discards where the interpolated `visible` is below 1, so a segment with a hidden end is dropped as in the mockup, and shades `arcColor` by `clamp(halfWidth + feather − |across|, 0, 1)` coverage. `halfWidth` = `(small ? 0.7 : 1)·unit / 2`, `feather` = one device pixel.
  - **`Globe`**, public:
    - **inputs:** `latticeMap: url`, `elapsedMs`, `unit` (`Theme.u`), `hasHome`, `homeLat`, `homeLon`, `endpoints: var` (`[{ lat, lon }]`, the first `MAX_ARCS` used), `signalColor`, `hiColor`. (The plan's `landMask` and `points` go, P23.)
    - **children:** the `globe.frag` effect; a `Repeater { model: MAX_ARCS }` of arc effects, each visible while `hasHome` and its endpoint exists; `Repeater { model: MAX_ARCS }`s of packet (3u) and end (4u) `Rectangle`s in `hi`; the home `Rectangle` (5u, `signal`) and its pulse `Rectangle` (transparent, 1u border in `signal` at alpha 1 − p, side `6 + 16p` units). Every model is a number, never an array, so a frame never recreates a delegate; delegates read `frame.*[index]`. Rectangles snap to device pixels. Without a home there are no arcs, packets or home marker, only end squares.
    - **output:** `readonly labelAnchors: var`, as the plan states.
    - Nothing renders except on `elapsedMs`, new endpoints or a resize.

- [ ] **Step 1: Generate and check the mask and the lattice map** (the Natural Earth download is approved: user, 2026-10-21)

Run the generator, then `--check` on both files. Expected: the check prints `ok` and the six points. Record the archive's SHA-256 in the script and the two PNG sizes (bytes) in the report.

- [ ] **Step 2: Gate the vertex-shader route**

In a headless KWin, a throwaway gallery page draws one arc effect (fixed home and end, `GridMesh` 40 × 1) and the globe effect over the lattice map. Expected:
- `qsb-qt6 --qt6` compiles `arc.vert` with its default targets (GLSL ES 100 included);
- the arc and the dots render, and the shell log has no shader warning.

If either fails, stop with NEEDS_CONTEXT. The controller's fallback is the arcs as one `Shape` with a `PathPolyline` per arc, re-pathed per frame from `globe.mjs`, measured as in Step 7 (P24).

- [ ] **Step 3: Write the failing tests**

```js
test('lattice ends at the poles', () => { assert.equal(fibonacciPoint(0, 3200).lat, 90); assert.equal(fibonacciPoint(3199, 3200).lat, -90); });
test('view and its inverse', () => { const v = unitVector({ lat: -23.55, lon: -46.63 }); assert.ok(dist(inverseView(view(v, 0.6, 0.38), 0.6, 0.38), v) < 1e-12); assert.deepEqual(view(unitVector({ lat: 0, lon: 0 }), 0, 0).map(round9), [0, 0, 1]); });
test('slerp São Paulo–London', () => { const { point, omega } = slerp(unitVector({ lat: -23.55, lon: -46.63 }), unitVector({ lat: 51.5, lon: -0.12 }), 0.5); assert.ok(Math.abs(norm(point) - 1) < 1e-9); assert.ok(omega > 1.4 && omega < 1.6); });
test('arc altitude', () => { assert.ok(Math.abs(arcAltitude(Math.PI, 20, 40) - 1.3) < 1e-9); assert.equal(arcAltitude(1, 0, 40), 1); });
test('packets', () => { assert.equal(packetSegment(0, 1), 14); assert.equal(packetSegment(4000, 0), 0); });
test('frame sizes', () => { const big = globeFrame(FRAME(400, 300, 1)); assert.deepEqual([big.small, big.radius], [false, 120]); const small = globeFrame(FRAME(200, 200, 1)); assert.deepEqual([small.small, small.radius], [true, 90]); assert.equal(small.dot, Math.max(1.3, 90 / 70)); });
test('lattice texels decode', () => { assert.deepEqual(decodeLattice(127, 12 + 16), { index: 3199, land: true }); assert.deepEqual(decodeLattice(0, 0), { index: 0, land: false }); });
test('the shader selection matches a brute force', () => { /* nearestAt = brute-force nearest point to the 1024 × 512 texel centre containing (lat, lon); radii 90 and 200; rot 0.6, 2.2, 4.1; every pixel in the outer 10 % of the disc plus every 8th pixel inside */ assert.equal(mismatches, 0); });
test('the shader unrolls exactly the derived offsets', () => { const source = readFileSync(GLOBE_FRAG, 'utf8'); assert.deepEqual(offsetsIn(source), [...LATTICE_NEIGHBOUR_OFFSETS].sort((a, b) => a - b)); });
test('a home behind the globe has a hidden anchor', () => { const f = globeFrame({ ...FRAME(400, 400, 1), elapsedMs: HOME_BEHIND_MS }); assert.equal(f.anchors.at(-1).visible, false); });
```

(`HOME_BEHIND_MS` is computed in the test from `ROT_BASE`/`ROT_PER_MS` so that São Paulo's view z is negative. The coverage test must run in under 5 s; it sets `LATTICE_NEIGHBOUR_OFFSETS` to the smallest set of Fibonacci offsets that gives zero mismatches.)

- [ ] **Step 4: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 5: Implement `globe.mjs`, the three shaders, `Globe.qml`, the build lines and the gallery page, then run everything**

Run: `node --test 'shell/tests/*.test.mjs' && make shaders && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t11.sock scripts/qml-smoke.sh` (headless).
Expected: PASS and `smoke: ok`. `scripts/install.sh` run against a temporary target refuses a tree whose `arc.vert.qsb` is removed (a harness, not the user's install dir).

Hold test (the gallery globe page is a `FrameClock` holder): run `GHOST_VIEW=gallery GHOST_GALLERY_GLOBE=widgets/uplink/globe-lattice.png GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t11.sock make smoke-holds` (Task 8b). Expected: `holds: ok`; the page releases its hold when hidden, when the dashboard is covered, and when destroyed.

- [ ] **Step 6: Check the geography and the look**

On the gallery globe page at the mockup's `u` and scale 1.15, snapshots 2 s apart at both sizes:
- the São Paulo marker sits on Brazil's south-east coast, and the sea just south-east of it has no land dots (spec 234);
- the continents are unmistakable in each snapshot while the globe turns;
- dots, arcs, packets, end squares and the pulse match the mockup's `drawGlobe` at the same size (record a montage beside the mockup's canvas).

- [ ] **Step 7: Measure**

The gallery globe page at V2-centre size, 60 s, three runs, against the gallery idle: frames/s (≤ 15.9 after C21's settle: the spec's 15 fps cap), shell CPU, `smaps_rollup` Rss delta, and the GPU time per frame from `QSG_RENDER_TIMING`. Expected from the Global Constraints' spike figure: ≈ 0.53 % for 15 frames/s plus the per-frame JavaScript. No pass mark here; Task 12 checks V1 against the 2 % budget.

- [ ] **Step 8: Commit**

```bash
git add scripts/land-mask.py widgets/uplink/land-mask.png widgets/uplink/globe-lattice.png
git commit -m "feat(scripts): rasterise Natural Earth land and the globe's dot lattice"
git add Makefile scripts/install.sh shell/kit/GhostKit/Globe.qml shell/kit/GhostKit/lib/globe.mjs shell/kit/GhostKit/shaders/globe.frag shell/kit/GhostKit/shaders/arc.vert shell/kit/GhostKit/shaders/arc.frag shell/kit/GhostKit/qmldir shell/host/Gallery.qml shell/tests/globe.test.mjs
git commit -m "feat(kit): draw the dot globe and its arcs with shaders"
```

---

### Task 12: `uplink` widget

**Files:**
- Create: `widgets/uplink/{manifest.toml,Widget.qml,uplink.mjs}`, `shell/tests/uplink.test.mjs`
- Modify: none in the kit (`Globe` comes from Task 11).

**Interfaces:**
- Consumes: `uplink` (Task 10), `host`, `Data.config.location`, `Globe` and `widgets/uplink/globe-lattice.png` (Task 11), `FrameClock`.
- Produces:
  - **Manifest:** `id = "uplink"`, `requires = ["host"]`, `animates = true`, no settings (the plan's `dots` setting goes with the baked lattice, P23).
  - **Data:** the widget calls `Data.want("uplink")` while it is visible and `Data.release("uplink")` when hidden or destroyed, as the clock does for weather (P6), so the 5 s socket and GeoIP cycle runs only while a globe is shown.
  - **`uplink.mjs`:** `footerText(snapshot)`, `homeLabel(host)`, `hasHome(config)`, as the plan states.
  - **Widget:**
    - a header `Uplink`, tag `scope`; the `Globe` fills the body with `latticeMap: Qt.resolvedUrl("globe-lattice.png")`;
    - endpoint labels are `Text`s (`textFormat: Text.PlainText`, C6(b): organisation and host names are foreign text), weight 600, 9 px (8 px when small), `hi` at 75 %, at `labelAnchors[i]` + (6, 3), shown only while that anchor is visible;
    - the home label at the last anchor + (9, −6) in `signal`;
    - the footer in `dim` 9 px at (10, h − 10), hidden when small;
    - without a location there is no home, no arcs and no packets; endpoints show as end squares only;
    - while visible it holds `FrameClock.acquire()` and binds `Globe.elapsedMs` to `FrameClock.elapsedMs`; it releases when hidden or destroyed.

- [ ] **Step 1: Write the failing tests**

```js
test('footer', () => { assert.equal(footerText({ total: 6, attribution: 'IP GEOLOCATION BY DB-IP' }), '6 ACTIVE ENDPOINTS · IP GEOLOCATION BY DB-IP'); assert.equal(footerText({ total: 0, attribution: '' }), 'NO GEO-IP DATABASE · SEE README'); });
test('home', () => { assert.equal(homeLabel({ hostname: 'tkacz-workstation' }), 'TKACZ-WORKSTATION'); assert.equal(hasHome({ location: { lat: -23.55, lon: null } }), false); assert.equal(hasHome({ location: { lat: -23.55, lon: -46.63 } }), true); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run the tests, a smoke and snapshots**

All `qs` runs headless (P10). The temp config sets the São Paulo `lat`, `lon` and `city` and no `location.alerts` (R12(b)), and puts `uplink` in V2's centre tabs and V1's right-column tabs. Run `node --test 'shell/tests/*.test.mjs' && GHOST_CONFIG=<temp> GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t12.sock scripts/qml-smoke.sh && GHOST_CONFIG=<temp> GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t12.sock make smoke-holds`.
Expected: PASS, `smoke: ok` and `holds: ok`. The second is this widget's hold test: its `FrameClock` hold and its `uplink` want return to their baselines after the tab is hidden, after a cover and after the view is destroyed.

Snapshot both sizes with `--fake` at the mockup's `u` and scale 1.15, three snapshots 2 s apart. Check that:
- the São Paulo marker sits on Brazil's south-east coast;
- the continents are unmistakable in all three;
- the small size shows endpoint labels (spec 192), and none overflows the panel.

Check with the daemon log at debug that `uplink` is subscribed only while the UPLINK tab is shown.

- [ ] **Step 4: Measure**

Release daemon `--fake` on its own socket, defaults, 60 s each, C21's settle: V2 with the UPLINK tab and V1 with the UPLINK tab. Report frames/s, shell and daemon CPU, and `smaps_rollup` Rss of both. V1 is Task 19's animating pass mark (≤ 2.00 % combined, ≤ 307 200 KiB): if either is over, stop and report rather than commit.

- [ ] **Step 5: Commit**

```bash
git add widgets/uplink/manifest.toml widgets/uplink/Widget.qml widgets/uplink/uplink.mjs shell/tests/uplink.test.mjs
git commit -m "feat(widgets): add the uplink globe"
```

---

### Task 13: Tab strip in the panel header row (P18)

**Files:**
- Modify: `shell/host/TabsHost.qml`, `shell/host/WidgetHost.qml`, `shell/host/LayoutView.qml` (input rects), `shell/kit/GhostKit/Panel.qml`

**Interfaces:**
- Produces:
  - **`Panel.headerReserve: real`** (logical px, default 0). The meta elides before the reserved width. Widgets never set it.
  - **`WidgetHost`** sets `headerReserve` on a loaded item that has the property. That is only the case inside a tabs node.
  - **`TabsHost`** draws its strip right-aligned **inside** the active widget's header row (mockup `.ph` tabs: 10 px text, tracking .14 em, the selected tab filled `signal` with `ground` text), instead of above the body. The body then fills the whole node.
  - The strip's rects stay part of `inputRects`.

- [ ] **Step 1: Record the before snapshot** of V2's centre tabs (with Tasks 9 and 12's widgets in a temp config).

- [ ] **Step 2: Implement**

- [ ] **Step 3: Run the smokes and the headless tab-click test** (Task 8b's harness, "Plan 1 R52", in the headless KWin; `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t13.sock`, and `make smoke-holds` before committing). Expected: `smoke: ok`, `holds: ok`, and the tab test passes.

Snapshot V1's scope and V2's centre and compare them with the mockup's `C.scope`/`C.centre` header rows:
- the label and tag sit on the left, the tabs on the right, on one line;
- nothing overlaps at 1280 or at the mockup's `u`.

- [ ] **Step 4: Commit**

```bash
git add shell/host shell/kit/GhostKit/Panel.qml
git commit -m "feat(shell): put a tabs node's strip in its panel header row"
```

---

### Task 14: CPU package power, iGPU load and the processor's PKG

**Files:**
- Create: `packaging/udev/70-ghost-shell-rapl.rules`
- Modify:
  - `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/collect/cpu.rs`, `crates/ghostd/src/fake/sampled.rs`, `crates/ghostd/tests/cpu.rs`;
  - `widgets/processor/Widget.qml`, `README.md` (Install: the rule and its trade-off).

**Interfaces:**
- Produces:
  - `CpuSnapshot.package_w: Option<f32>`. The collector:
    - finds the powercap zone whose `name` is `package-0` under `sys/class/powercap/*` at build time, skipping any entry without a readable `name` (the control-type `intel-rapl` entry has none; P33, fx C8);
    - reads `energy_uj` each sample;
    - computes W = ΔµJ / Δµs;
    - wraps with the zone's own `max_energy_range_uj` when the counter decreased (65 532 610 987 on this machine);
    - returns `None` when the file is unreadable (logged once at info) or on the first sample.
  - `CpuCollector::sample_after(elapsed: Duration) -> CpuSnapshot`, as `DiskCollector` has (`collect/disk.rs:232`), for the tests that need a known interval (P14).
  - `CpuSnapshot.igpu_busy_pct: Option<f32>`, read from the first `sys/class/drm/card*/device` with `vendor` `0x1002` that has `gpu_busy_percent`, else `None`. On this machine it stays `None`: the only DRM card is the NVIDIA GPU, and the Radeon 610M has no driver bound (fx C7).
  - **The udev rule:** `ACTION=="add|change", SUBSYSTEM=="powercap", ATTR{name}=="package-0", RUN+="/usr/bin/chmod 0444 /sys%p/energy_uj"`. Its comment says why world-readable was chosen (user decision 2026-10-21) and names PLATYPUS / CVE-2020-8694.
  - **Processor:** a fifth `Kv` `PKG` (`Fmt.watts`), between `FREQ` and `CPU FAN`, only while `package_w` is non-null. The row re-divides. The mockup has four; spec §6.3 adds package power.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn package_power_from_the_energy_delta() { /* energy 1_000_000 → 46_000_000 µJ over sample_after(1 s) */ assert_eq!(s.package_w, Some(45.0)); }
#[test] fn a_wrapped_counter_still_reads_positive() { /* max 262_143_328_850; before max − 1_000_000, after 44_000_000, 1 s */ assert_eq!(s.package_w, Some(45.0)); }
#[test] fn a_zone_without_a_name_is_skipped() { /* powercap holds `intel-rapl` (no name file) and `intel-rapl:0` (name `package-0`) */ assert_eq!(s.package_w, Some(45.0)); }
#[test] fn an_unreadable_counter_hides_package_power() { /* no energy_uj */ assert_eq!(s.package_w, None); }
#[test] fn the_first_sample_has_no_package_power() { assert_eq!(first.package_w, None); }
#[test] fn no_amd_drm_device_means_no_igpu() { assert_eq!(s.igpu_busy_pct, None); }
#[test] fn an_amd_igpu_reports_its_busy_percent() { /* card0 vendor 0x1002 gpu_busy_percent 7 */ assert_eq!(s.igpu_busy_pct, Some(7.0)); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test cpu`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check and the smoke**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t14.sock make smoke` (headless).
Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Write the rule file and check the unreadable case**

Write `packaging/udev/70-ghost-shell-rapl.rules` and the README note. Do not install it: the install and the live reading are Task 19's (P14).
Check with `ghostd probe cpu` that `package_w` is `null` while `/sys/class/powercap/intel-rapl:0/energy_uj` is 0400 (today's state), that `igpu_busy_pct` is `null`, and that the daemon logs the unreadable counter once.

- [ ] **Step 5: Commit**

```bash
git add packaging/udev crates widgets/processor README.md
git commit -m "feat(ghostd): report CPU package power and iGPU load"
```

---

### Task 15: `silicon` widget

**Files:**
- Create: `widgets/silicon/{manifest.toml,Widget.qml,silicon.mjs}`, `shell/tests/silicon.test.mjs`

**Interfaces:**
- Consumes: `cpu` (`ccds`, `cores`, `threads`, `package_w`, `igpu_busy_pct`, `freq_mhz`, `model`), `sensors` (`CPU TCTL`, `CCD0`, `CCD1` by label), `mem`, `gpu`, `disks`, `BlockGraph`, `SegMeter`, `FrameClock`.
- Produces:
  - **Manifest:** `id = "silicon"`, `requires = ["cpu", "sensors", "mem", "gpu", "disks"]`, `animates = true`.
  - **`silicon.mjs`:**
    - `headerMeta(cpu, gpu)` → `PKG 45 W · SYS ≈ 99 W` (SYS = package + `gpu.power_w`, both rounded). It returns `""` without a package figure, so SYS is never shown alone.
    - `titleLine(cpu)` → `AMD RYZEN 9 9950X3D · 16C / 32T`, from the model uppercased (`Fmt.cpuModelShort` applies to the narrow form `9950X3D · 16C / 32T`), the core count and the thread count.
    - `ccdTiles(cpu)` → per CCD, 8 tiles (or however many cores it has) `{ name: 'C<n>', loads: [a, b], level }`:
      - `n` counts cores across CCDs in CCD order;
      - the loads are the core's threads in cpu order;
      - `level` is the hotter thread's level by `Theme.loadWarn`/`loadCrit`.
    - `l3Text(ccds, ccd, narrow)` → `L3 32 MB + 3D V-CACHE 64 MB` (narrow `L3 32M + V-CACHE 64M`) for a V-Cache CCD, where the extra is its `l3_kib` minus the smallest non-V-Cache CCD's, else `L3 32 MB`.
    - `iodBlocks(mem, gpu, disks, cpu)` → four blocks:
      - `MEM` `19.8 / 92G`;
      - `GPU` `RTX 4080S 12%` (`Fmt.gpuNameShort`);
      - `NVME` `R 120 MB/s`, the sum of the live NVMe read rates; its fraction is that rate over `NVME_FULL_BPS = 400_000_000` (mockup);
      - `IGPU` `7%`, or `OFF` when `igpu_busy_pct` is null.

      Each block carries a 0–1 fraction.
    - `packetPhase(elapsedMs, loadPct, k) = (elapsedMs·(0.0004 + loadPct·0.00003) + k·0.5) % 1` (mockup).
  - **Widget** (layout from the mockup's `drawDie`, `small = width < 380` design px):
    - a static `Shape` for the chamfered package, CCD and I/O-die outlines;
    - per core tile, a 1 px outline in its level colour and one `BlockGraph` with `cols: 2` for the two threads;
    - the L3 strip, with a static 5 px hatch `Shape` on the V-Cache CCD;
    - dashed Infinity Fabric links with one 3 px `hi` packet each, moved on `FrameClock`;
    - the I/O-die blocks with a `SegMeter` each.

    The mockup title's `AM5` and the blocks' `DDR5`/`PCIE 5 X16`/`6 NM` are omitted (call 5).

- [ ] **Step 1: Write the failing tests**

```js
test('header meta', () => { assert.equal(headerMeta({ package_w: 45.2 }, { present: true, power_w: 54.1 }), 'PKG 45 W · SYS ≈ 99 W'); assert.equal(headerMeta({ package_w: null }, { present: true, power_w: 54 }), ''); });
test('title', () => assert.equal(titleLine(FAKE_CPU), 'AMD RYZEN 9 9950X3D · 16C / 32T'));
test('tiles pair each core with its threads', () => { const [ccd0] = ccdTiles(FAKE_CPU); assert.equal(ccd0.length, 8); assert.deepEqual(ccd0[0].loads, [FAKE_CPU.threads[0], FAKE_CPU.threads[16]]); assert.equal(ccdTiles(FAKE_CPU)[1][0].name, 'C8'); });
test('l3 text', () => { assert.equal(l3Text(FAKE_CPU.ccds, FAKE_CPU.ccds[0], false), 'L3 32 MB + 3D V-CACHE 64 MB'); assert.equal(l3Text(FAKE_CPU.ccds, FAKE_CPU.ccds[1], true), 'L3 32M'); });
test('iod blocks', () => { const b = iodBlocks(MEM, GPU, DISKS, { igpu_busy_pct: null }); assert.deepEqual(b.map((x) => x.name), ['MEM', 'GPU', 'NVME', 'IGPU']); assert.equal(b[3].value, 'OFF'); });
test('packet phase', () => { assert.equal(packetPhase(0, 0, 1), 0.5); assert.ok(Math.abs(packetPhase(1000, 10, 0) - 0.7) < 1e-9); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run the tests, a smoke and snapshots**

All `qs` runs headless (P10), with `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t15.sock`. Snapshot V2's centre (SILICON tab) and V1's scope (narrow) with `--fake`, and compare them with the mockup's `drawDie` at both widths.

Hold test: run `GHOST_CONFIG=<temp config with silicon in a tab> GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t15.sock make smoke-holds` (Task 8b) before committing. Expected: `holds: ok`; the packets' `FrameClock` hold returns to its baseline when the tab is hidden, when the dashboard is covered and when the view is destroyed.

Measure V2 with SILICON over 60 s (animating, ≤ 2 % with the daemon; call 4; defaults, three runs). The 16 tile `BlockGraph`s each repack 70 words per `cpu` tick: report their share of the shell's CPU (by measuring the same view with the tiles' graphs frozen, or by `QSG_RENDER_TIMING`), and the CPU and `smaps_rollup` Rss delta against V2 without SILICON.

- [ ] **Step 4: Commit**

```bash
git add widgets/silicon shell/tests/silicon.test.mjs
git commit -m "feat(widgets): add the silicon schematic"
```

---

### Task 16: `disks.health` feed and the storage widget's wear and hours

**Files:**
- Create: `crates/ghostd/src/feeds/health.rs`, `crates/ghostd/tests/health.rs`
- Modify:
  - `crates/ghost-proto/src/{snapshot.rs,lib.rs}`, `crates/ghostd/src/{main.rs,feeds/mod.rs,fake/feeds.rs}`;
  - `widgets/storage/{manifest.toml,Widget.qml}`, `shell/kit/GhostKit/lib/labels.mjs` and `Fmt.qml` (`serialOf`), `shell/tests/labels.test.mjs`.

**Interfaces:**
- Produces:
  - `topics::DISKS_HEALTH = "disks.health"`, added to `BUILTIN`.
  - `DiskHealth { drives: Vec<DriveHealth> }` and `DriveHealth { serial: String, wear_pct: Option<u8>, power_on_hours: Option<u32> }`.
  - `health::feed() -> Feed` (system bus) and `health::feed_on(bus: zbus::Connection, clock: WallClock) -> Feed`.
  - **Behaviour:**
    - while wanted, it reads UDisks2 `GetManagedObjects` when first wanted and then every `HEALTH_EVERY_S = 600` (spec), due-ness on the wall clock with sleeps capped at 60 s (R25), and in full when `org.freedesktop.UDisks2` gains an owner;
    - drives are joined on UDisks2's `Drive.Serial`, which equals the sysfs serial at the end of ghostd's `{model}#{serial}` drive id (`collect/disk.rs:340`); never on model, which sysfs truncates for SATA drives (P31, fx C9);
    - **NVMe** (`org.freedesktop.UDisks2.NVMe.Controller`): `power_on_hours` comes from `SmartPowerOnHours`, and `wear_pct` from `SmartGetAttributes({})["percent_used"]` (a D-Bus byte, `y`), both only when `SmartUpdated > 0`;
    - **ATA** (`Drive.Ata`): `power_on_hours = SmartPowerOnSeconds / 3600` when `SmartUpdated > 0`, and `wear_pct` is `None`. No method is called;
    - **never `SmartUpdate`**;
    - a `NotAuthorized` or any other error leaves that field `None`, logged once per drive.
  - **`labels.mjs`:** `serialOf(driveId)` returns the text after the last `#`.
  - **Storage widget:** `requires = ["disks", "disks.health"]`. A drive's second line ends with a `dim` `W 3% · 12345 H` when either value is known, and only on rows whose `disks` state is `live`: the sysfs-dead WD stays the crit alert line, with no wear or hours, although UDisks2 still reports that controller with an older cached reading (P31, fx C5). It is never coloured: the spec sets no wear threshold. The mount text elides first.

- [ ] **Step 1: Write the failing tests**

These run on the private bus with a fake UDisks2 object server that counts method calls per interface.

```rust
#[tokio::test(start_paused = true)] async fn wear_and_hours_come_from_the_cache() { assert_eq!(drive(&snap, "2O412LA45EG7"), &DriveHealth { serial: "2O412LA45EG7".into(), wear_pct: Some(3), power_on_hours: Some(12_345) }); }
#[tokio::test(start_paused = true)] async fn smart_update_is_never_called() { assert_eq!(udisks.calls("SmartUpdate"), 0); }
#[tokio::test(start_paused = true)] async fn ata_drives_are_read_from_properties_only() { assert_eq!(udisks.method_calls_on("org.freedesktop.UDisks2.Drive.Ata"), 0); assert_eq!(drive(&snap, "WD-BLUE").power_on_hours, Some(20_000)); }
#[tokio::test(start_paused = true)] async fn a_drive_never_read_reports_none() { /* a synthetic fake drive: SmartUpdated 0, serial NEVER-READ */ let d = drive(&snap, "NEVER-READ"); assert_eq!((d.wear_pct, d.power_on_hours), (None, None)); }
#[tokio::test(start_paused = true)] async fn not_authorized_leaves_wear_unknown_and_keeps_hours() { /* the fake answers SmartGetAttributes with org.freedesktop.UDisks2.Error.NotAuthorized; SmartPowerOnHours is 12_345 */ let d = drive(&snap, "2O412LA45EG7"); assert_eq!((d.wear_pct, d.power_on_hours), (None, Some(12_345))); assert_eq!(snap.drives.len(), 2); }
#[tokio::test(start_paused = true)] async fn a_resume_after_the_cadence_refreshes_once() { /* wall clock +8 h while the monotonic clock moves 60 s */ assert_eq!(udisks.calls("GetManagedObjects"), 2); }
#[tokio::test(start_paused = true)] async fn udisks_appearing_later_is_read_in_full() { /* the fake claims org.freedesktop.UDisks2 30 s after the feed starts */ assert!(before.drives.is_empty()); assert_eq!(after.drives.len(), 2); assert_eq!(drive(&after, "2O412LA45EG7").wear_pct, Some(3)); }
#[tokio::test(start_paused = true)] async fn nothing_runs_while_unwanted() { /* demand never contains disks.health; 20 min pass */ assert_eq!(udisks.total_calls(), 0); }
```

```js
test('serial of a drive id', () => assert.equal(serialOf('XPG GAMMIX S70 BLADE#2O412LA45EG7'), '2O412LA45EG7'));
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test health && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check and the smoke**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && node --test 'shell/tests/*.test.mjs' && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t16.sock make smoke` (headless).
Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Live check and measure** (system bus, read-only)

Run a dev daemon plus `topic-tap.py` on `disks.health`. Expected:
- the XPG, the Kingston and the WD report wear and hours (the WD's from UDisks2's older cache, P31);
- `sda` and `sdb` report hours and stay `running`: their power state is unchanged before and after (`cat /sys/block/sd?/device/state`);
- the storage panel shows the WD as DEAD (sysfs `state = dead`) with no wear or hours.

Snapshot Storage at V1 width, headless: every line fits. Report the time per refresh.

- [ ] **Step 5: Commit**

```bash
git add crates widgets/storage shell/kit/GhostKit shell/tests
git commit -m "feat(ghostd): add drive wear and power-on hours from UDisks2"
```

---

### Task 17: `batteries` feed and the sensors widget (P19)

**Files:**
- Create: `crates/ghostd/src/feeds/batteries.rs`, `crates/ghostd/tests/batteries.rs`
- Modify:
  - `crates/ghost-proto/src/{snapshot.rs,lib.rs,config.rs}`, `crates/ghostd/src/config/default.toml`;
  - `crates/ghostd/src/{main.rs,feeds/mod.rs,fake/feeds.rs}`;
  - `widgets/sensors/{manifest.toml,Widget.qml}`.

**Interfaces:**
- Produces:
  - `topics::BATTERIES = "batteries"`, added to `BUILTIN`.
  - `Batteries { devices: Vec<Battery> }`.
  - `Battery { id: String, label: String, kind: BatteryKind, pct: Option<f32>, charging: bool }`.
  - `enum BatteryKind { Controller, Phone, Mouse, Keyboard, Headset, Other }`.
  - **Config:** `[batteries] labels = {}`, the `Labels` type, keyed by `id`.
  - `batteries::feed() -> Feed` and `batteries::feed_on(system: zbus::Connection, session: zbus::Connection, clock: WallClock) -> Feed`.
  - **UPower** (system bus), event-driven:
    - `EnumerateDevices`, `DeviceAdded`/`DeviceRemoved`, and each device's `PropertiesChanged`;
    - devices with `PowerSupply == false` only, never the `DisplayDevice`;
    - `id` is the `NativePath`, and `label` the `Model` uppercased;
    - `Type`: 12 (gaming input) → Controller, 5 → Mouse, 6 → Keyboard, 17 → Headset, anything else → Other;
    - `charging` is `State == 1`.
  - **KDE Connect** (session bus), polled every 60 s on the wall clock (R25):
    - `/modules/kdeconnect` `devices(onlyReachable: true, onlyPaired: true)`, interface `org.kde.kdeconnect.daemon`, signature `bb → as` (fx/kdeconnect.json);
    - `org.kde.kdeconnect.device` `name` (the label, uppercased) and `type` (`phone` and `tablet` → Phone);
    - `org.kde.kdeconnect.device.battery` properties `charge` (a D-Bus `i`, read as `i32`), `isCharging` and `hasBattery`: `pct` is `None` unless `hasBattery` is true and 0 ≤ `charge` ≤ 100, so a phone that never reported reads neither 0 % nor −1 % (P32);
    - `id` is `kdeconnect:<device id>`;
    - it never requests a refresh from the device.
  - Both services are re-read in full when their name gains an owner. While unwanted, nothing runs.
  - **Sensors widget:** `requires = ["sensors", "batteries"]`. Battery labels are `Text`s with `textFormat: Text.PlainText` (device models and names are foreign text, C6(b)). Batteries follow the fans after a separator, in the same two-column grid: `DUALSENSE 80%`, `PHONE 15%`, coloured by `Segments.levelLow` with `Theme.batteryWarn`/`batteryCrit` (≤ 20 warn, ≤ 10 crit), and a `+` after the value while charging. This closes P19: rows that don't fit in 184 px elide the last group, and the meta count still counts every probe.

- [ ] **Step 1: Confirm the KDE Connect interface**

fx/kdeconnect.json already answers this: `devices(bb)` on `org.kde.kdeconnect.daemon`; `name` and `type` on `org.kde.kdeconnect.device`; `charge` (`i`), `isCharging` and `hasBattery` on `org.kde.kdeconnect.device.battery`. Re-check only if the installed KDE Connect version differs from the one in the fixture, with the read-only `busctl --user --xml-interface introspect org.kde.kdeconnect <path>` (plain `introspect` fails on the overloads); the user approved read-only introspection on 2026-10-21. If the names differ, stop with NEEDS_CONTEXT.

- [ ] **Step 2: Write the failing tests**

These use fake UPower and KDE Connect servers on two private buses.

```rust
#[tokio::test(start_paused = true)] async fn a_dualsense_reads_as_a_controller() { assert_eq!(b.kind, BatteryKind::Controller); assert_eq!(b.label, "DUALSENSE WIRELESS CONTROLLER"); assert_eq!(b.pct, Some(80.0)); }
#[tokio::test(start_paused = true)] async fn the_display_device_and_power_supplies_are_skipped() { /* the fake enumerates DisplayDevice, a PowerSupply `battery_BAT0` and the DualSense */ assert_eq!(snap.devices.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["ps-controller-battery-test"]); }
#[tokio::test(start_paused = true)] async fn a_paired_reachable_phone_reports_its_charge() { assert_eq!(phone.pct, Some(15.0)); assert_eq!(phone.id, "kdeconnect:abc123"); }
#[tokio::test(start_paused = true)] async fn no_refresh_is_ever_requested_from_the_phone() { assert_eq!(kdeconnect.method_calls_except(&["devices"]), 0); }
#[tokio::test(start_paused = true)] async fn upower_appearing_later_is_read_in_full() { /* the fake UPower claims its name 5 s after the feed starts */ assert!(before.devices.is_empty()); assert_eq!(after.devices.len(), 1); assert_eq!(after.devices[0].pct, Some(80.0)); }
#[tokio::test(start_paused = true)] async fn a_phone_that_never_reported_has_no_charge() { /* hasBattery false, charge -1; then hasBattery true, charge 101 */ assert_eq!(phone_pct(NO_BATTERY), None); assert_eq!(phone_pct(OUT_OF_RANGE), None); assert_eq!(phone_pct(CHARGE_ZERO), Some(0.0)); }
#[tokio::test(start_paused = true)] async fn a_config_label_replaces_the_model() { assert_eq!(b.label, "PAD"); }
#[tokio::test(start_paused = true)] async fn nothing_runs_while_unwanted() { /* demand never contains batteries; 3 min pass */ assert_eq!((upower.calls(), kdeconnect.calls()), (0, 0)); }
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p ghostd --test batteries`
Expected: FAIL.

- [ ] **Step 4: Implement, then run every check, the smoke and a Sensors snapshot**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && node --test 'shell/tests/*.test.mjs' && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t17.sock make smoke` (headless).
Take the Sensors snapshot at V1 and MONITOR sizes with the fake's two batteries, headless. Everything fits or elides cleanly. Expected: PASS and `smoke: ok`.

- [ ] **Step 5: Live check** (read-only)

Run a dev daemon plus `topic-tap.py` on `batteries`. The DualSense shows with its real UPower model (or the label from the temp config's `[batteries] labels`) and its real charge, and the phone shows with its own charge when KDE Connect reaches it (the fixture's values differ from the tests' fake ones, P32). Report the cost per KDE Connect poll.

- [ ] **Step 6: Commit**

```bash
git add crates widgets/sensors
git commit -m "feat(ghostd): add device batteries from UPower and KDE Connect"
```

---

### Task 17b: Spectrum retry after a PipeWire restart

M7/C27 of Plan 2a's final review. Today `capture_while_wanted` (`crates/ghostd/src/feeds/spectrum/mod.rs`) answers a capture that ends on its own (`Stop::Ended`: PipeWire went away, or was never there) by waiting for the topic to be released, so the bars stay dark until the next pause, view change or cover. The backoff below is the user's (2026-10-21).

**Files:**
- Modify:
  - `crates/ghostd/src/feeds/spectrum/mod.rs` (the retry loop, its constants, `run`, the module and function docs, and the tests);
  - `crates/ghostd/src/feeds/spectrum/capture.rs` (`Capture::connected`, and a test-only `Capture::detached`: the feed has no injectable connector today, and this is the smallest one).

**Interfaces:**
- Produces:
  - **Constants in `spectrum/mod.rs`** (user-confirmed 2026-10-21): `RETRY_FIRST = Duration::from_secs(1)` and `RETRY_CAP = Duration::from_secs(30)`. The delay doubles after every attempt that does not reset it, and never exceeds `RETRY_CAP`, so the waits are 1, 2, 4, 8, 16, 30, 30, … s.
  - **`Capture::connected(&self) -> bool`:** true once PipeWire has taken the stream, that is once its state reached `Paused` or `Streaming`. The capture thread sets a shared `Arc<AtomicBool>` in the stream's `state_changed` callback (`Connecting` stays false). `connected` is the only evidence that a reconnect succeeded: a capture on a suspended sink delivers no frames, so a frame cannot be that evidence.
  - **`Capture` split for the seam:** `Capture { frames, connected, running: Option<Running> }`, where `Running { quit, thread }` holds what only the real thread has; `Capture::stop` returns at once when `running` is `None`. `#[cfg(test)] Capture::detached(frames: broadcast::Receiver<Frame>, connected: bool) -> Capture` builds one without a thread.
  - **`run(context: FeedContext, start: impl FnMut() -> io::Result<Capture>)`:** the loop `feed()` had inline (`until_wanted`, then `capture_while_wanted`, forever). `feed()` becomes `Feed::new(topics::SPECTRUM, |context| run(context, Capture::start))`.
  - **`capture_while_wanted(context, start)`**, with `delay = RETRY_FIRST` as a local, so every fresh want starts over:
    1. call `start`; an `Err` is a failed attempt, handled like step 3 with `connected = false` (it replaces the "stays dark until released" branch);
    2. `publish_frames` until `Stop::Released` or `Stop::Ended`; read `capture.connected()`; `capture.stop().await`;
    3. on `Stop::Released`, return. On `Stop::Ended`: if the capture was connected, set `delay = RETRY_FIRST`; then wait `delay`, or return as soon as the topic is released (`until_unwanted`), whichever comes first; then `delay = min(delay × 2, RETRY_CAP)` and go to 1.
  - Nothing is attempted while the topic is unwanted: no timer outlives a release, and `run` only starts a capture after `until_wanted`.
  - Each failed attempt keeps the capture thread's own warning (`audio output capture failed; audio.spectrum stays dark`), so a PipeWire that stays down logs at most two lines a minute. The module doc line "Without PipeWire the feed logs once each time the topic becomes wanted" becomes "…and retries with a backoff while the topic stays wanted".

- [ ] **Step 1: Write the failing tests**

In the existing `tests` module, lift the `FeedContext` that `forwarding()` builds into `fn context(wanted: bool) -> (FeedContext, watch::Sender<BTreeSet<String>>, mpsc::UnboundedReceiver<Value>)`, and use it in both. Then:

```rust
const RETRY_DELAYS: [Duration; 7] = [Duration::from_secs(1), Duration::from_secs(2), Duration::from_secs(4), Duration::from_secs(8), Duration::from_secs(16), Duration::from_secs(30), Duration::from_secs(30)];
const SLACK: Duration = Duration::from_millis(1);

#[derive(Clone, Copy)]
enum Outcome { StartFails, EndsUnconnected, EndsConnected }

/// `run` on a connector the test controls: attempt `n` gets `outcome(n)`, and every start is recorded.
struct Retrying { starts: Arc<Mutex<Vec<Instant>>>, demand: watch::Sender<BTreeSet<String>>, _published: mpsc::UnboundedReceiver<Value>, running: JoinHandle<()> }

fn retrying(wanted: bool, outcome: impl Fn(usize) -> Outcome + Send + 'static) -> Retrying {
    let starts = Arc::new(Mutex::new(Vec::new()));
    let record = Arc::clone(&starts);
    let (context, demand, _published) = context(wanted);
    let start = move || {
        let attempt = { let mut starts = record.lock().unwrap(); starts.push(Instant::now()); starts.len() - 1 };
        match outcome(attempt) {
            Outcome::StartFails => Err(io::Error::other("no thread")),
            ended => { let (_, frames) = broadcast::channel(1); Ok(Capture::detached(frames, matches!(ended, Outcome::EndsConnected))) } // the sender is dropped: the capture has ended
        }
    };
    Retrying { starts, demand, _published, running: tokio::spawn(run(context, start)) }
}
impl Retrying {
    fn count(&self) -> usize { self.starts.lock().unwrap().len() }
    fn gaps(&self) -> Vec<Duration> { self.starts.lock().unwrap().windows(2).map(|pair| pair[1] - pair[0]).collect() }
}

#[tokio::test(start_paused = true)]
async fn retries_start_after_one_second_and_double_to_the_cap() {
    for outcome in [Outcome::EndsUnconnected, Outcome::StartFails] {
        let run = retrying(true, move |_| outcome);
        sleep(RETRY_DELAYS.iter().sum::<Duration>() + SLACK).await;
        assert_eq!(run.gaps(), RETRY_DELAYS); // 1, 2, 4, 8, 16, 30, 30 s
        run.running.abort();
    }
}

#[tokio::test(start_paused = true)]
async fn a_successful_connection_resets_the_delay() {
    let run = retrying(true, |attempt| if attempt == 3 { Outcome::EndsConnected } else { Outcome::EndsUnconnected });
    sleep(Duration::from_secs(1 + 2 + 4 + 1 + 2) + SLACK).await;
    assert_eq!(run.gaps(), [1, 2, 4, 1, 2].map(Duration::from_secs)); // attempt 3 connected before it ended
}

#[tokio::test(start_paused = true)]
async fn nothing_is_attempted_while_unwanted() {
    let run = retrying(false, |_| Outcome::EndsUnconnected);
    sleep(Duration::from_secs(120)).await;
    assert_eq!(run.count(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_release_cancels_the_pending_retry_and_a_later_want_starts_over() {
    let run = retrying(true, |_| Outcome::EndsUnconnected);
    sleep(Duration::from_secs(1 + 2) + SLACK).await; // attempts at 0, 1 and 3 s; the next is due at 7 s
    assert_eq!(run.count(), 3);

    run.demand.send_replace(BTreeSet::new());
    sleep(Duration::from_secs(120)).await;
    assert_eq!(run.count(), 3);

    run.demand.send_replace(BTreeSet::from([topics::SPECTRUM.to_owned()]));
    sleep(RETRY_FIRST + SLACK).await;
    assert_eq!(run.count(), 5); // one at once, one RETRY_FIRST later
    assert_eq!(run.gaps().last(), Some(&RETRY_FIRST));
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd spectrum`
Expected: FAIL to compile (no `run`, `Capture::detached`, `RETRY_FIRST`); the existing spectrum tests are unchanged.

- [ ] **Step 3: Implement**

`Capture`'s split and `connected`, `run`, the retry loop, the constants, and the doc updates above. No other module changes: the fake spectrum and the protocol are untouched.

- [ ] **Step 4: Run every check**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. The four tests above and the four existing `publish_frames` tests all run.

- [ ] **Step 5: Live check** (the controller asks the user first: it interrupts their audio for a moment)

Run a dev daemon on its own socket with `topic-tap.py` on `audio.spectrum` while something plays. The user restarts PipeWire (`systemctl --user restart pipewire pipewire-pulse wireplumber`). Expected: the bands go dark when PipeWire stops, and frames resume within 30 s of PipeWire being back, with no release of the topic in between. Report the time to the first frame.

- [ ] **Step 6: Commit**

```bash
git add crates/ghostd/src/feeds/spectrum/mod.rs crates/ghostd/src/feeds/spectrum/capture.rs
git commit -m "fix(ghostd): retry the spectrum capture after PipeWire restarts"
```

---

### Task 18: Default views without machine placeholders

**Files:**
- Modify: `crates/ghost-proto/src/config.rs`, `crates/ghostd/src/config/default.toml`, `crates/ghostd/tests/config_load.rs`, `shell/tests/layout.test.mjs`

**Interfaces:**
- Consumes: every widget above, and the scope height `S` from Task 8's report.
- Produces:
  - **V1:** the right column becomes:
    - `column` 300: tabs `[uplink "UPLINK", silicon "SILICON", process-tree "TREE"]` (default 0, flex) and `network {style="scope"}` `S`;
    - then `storage`;
    - then `sensors` 184.
  - **V2:**
    - centre tabs: `uplink "UPLINK"`, `silicon "SILICON"`, `process-tree "PROCESS TREE"`;
    - right column: `placeholder{Network}` 164 → `network` 164.
  - **MONITOR:**
    - `placeholder{Telemetry}` → `column {gutter = 6}` of six `telemetry-lane` slots, `metric` = `cpu`, `gpu`, `mem`, `net`, `disk` and `therm`;
    - `placeholder{Process tree}` → `process-tree`.
  - `placeholder{Notifications}` stays (Plan 3).
  - `default.toml` spells out all three, in the same comment style.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn default_views_have_no_machine_placeholders() { let c = load_str_with_user("", "u").unwrap(); let labels = placeholder_labels(&c.layout); assert_eq!(labels, ["Notifications", "Notifications"]); }
// placeholder_labels is a test helper this task writes (it walks the resolved layout for `placeholder` slots); DEFAULT_*_FOR_TEST exist in shell/tests/layout.test.mjs.
// default_file_equals_builtin_defaults (Plan 1) must keep passing.
```

```js
test('v1 scope column', () => { const r = computeRects(DEFAULT_V1_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12); assert.equal(pick(r, 'network').h, S); assert.equal(r.find((x) => x.tabs).h, 300 - 12 - S); });
test('monitor lanes are six, 6 px apart', () => { const lanes = computeRects(DEFAULT_MONITOR_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12).filter((x) => x.widget === 'telemetry-lane'); assert.equal(lanes.length, 6); assert.equal(lanes[1].y - (lanes[0].y + lanes[0].h), 6); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test config_load && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Write the trees**

- [ ] **Step 4: Run every check and both smokes**

Run, inside the headless KWin (P10): `cargo test --workspace && node --test 'shell/tests/*.test.mjs' && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t18.sock scripts/qml-smoke.sh && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t18.sock GHOST_SMOKE_BUS=private GHOST_SMOKE_ACTIONS="monitor swap" scripts/qml-smoke.sh && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2b-t18.sock make smoke-holds`
Expected: PASS, `smoke: ok` twice with zero slot errors, and `holds: ok` over the real default views.

- [ ] **Step 5: Snapshot each view**

Snapshot every default view, headless, at the mockup's `u` and at `QT_SCALE_FACTOR=1.15` with `--fake`, and compare them with `L-v1`, `L-v2` and `L-mon`. Nothing overflows. Record the deviations: the two-panel scope (call 7) and the omitted silicon facts (call 5).

- [ ] **Step 6: Commit**

```bash
git add crates/ghost-proto/src/config.rs crates/ghostd/src/config/default.toml crates/ghostd/tests/config_load.rs shell/tests/layout.test.mjs
git commit -m "feat(config): fill the default views with the machine widgets"
```

---

### Task 19: Install, measure, accept Part 2b

**Files:**
- Create: `docs/superpowers/notes/2026-10-21-plan2b-acceptance.md`
- Modify: `README.md` (Performance: every new collector, feed, widget and state; credits for DB-IP, Natural Earth and GeoLite2)

**Interfaces:**
- Consumes: everything above; Plan 2a R17's install order and its config regeneration (`alert_chips = false` included).

- [ ] **Step 1: Install**

Follow R17's order (`make release`, which builds the shaders, `.vert` included). When regenerating the user's config:
- ask the user whether to add a `[uplink.labels]` table, with organisations of their choosing;
- write the `[batteries] labels` entry that maps the DualSense's id, its UPower `NativePath` (`ps-controller-battery-<MAC>`, read from the live `batteries` topic and never committed), to `"DUALSENSE EDGE"` (user, 2026-10-21).

Ask the user to install the RAPL rule (moved from Task 14, P14): `sudo install -m 0644 packaging/udev/70-ghost-shell-rapl.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=powercap --action=add`. Then check that `stat -c %a /sys/class/powercap/intel-rapl:0/energy_uj` prints `444` and that `ghostd probe cpu` shows a plausible `package_w`, and report the read cost per sample.

- [ ] **Step 2: Measure each state for 60 s** (shell + daemon combined; RSS is `smaps_rollup` Rss, R50, which includes the mapped GeoIP pages)

Every state is measured at the default appearance, headless as 2a Task 18 did, with frame counts taken after C21's settle. The user's own appearance (spectrum 30 fps, stripe step 0.01 s, attention 600 s) is recorded beside each figure, not judged (P9). If a reading breaks its pass mark, stop and report the numbers: the controller asks the user.

| State | How | Expected |
|---|---|---|
| Animating | V1 (UPLINK tab: globe, packets, the dead-drive alert) | ≤ 2.00 %, RSS ≤ 307200 KiB |
| Animating | V2 with SILICON | ≤ 2.00 % |
| Still | MONITOR, no active alert, nothing playing | ≤ 1.00 % |
| Covered | `scripts/measure-covered.sh 60` | 0 frames, ≤ 0.50 %, and `net.links`, `uplink`, `disks.health` and `batteries` make no calls (daemon log at debug) |

- [ ] **Step 3: Behaviour checks with the user**

- The globe:
  - São Paulo sits on the south-east coast;
  - the arcs reach real endpoints;
  - an organisation set to `"process"` shows the app's name;
  - a direct Tailscale peer appears as `TAILNET:<HOST>`.
- Collapsing and expanding a tree node works.
- The network panel shows LAN, OpenVPN and Tailscale with their real states. Disconnecting the VPN turns its row `DOWN` (user step).
- PKG and SYS ≈ show. After the next reboot `energy_uj` is still `0444`; the user confirms later.
- The DualSense (as `DUALSENSE EDGE`) and the phone show their batteries. The storage rows of live drives show wear and hours, and the dead WD stays the DEAD alert line without them (P31). The dead-drive alert check stands: it keys on sysfs `state = dead`.

- [ ] **Step 4: Write the note and update README**

Record:
- every measurement;
- the deviations: calls 4, 5 and 7, and the omitted mockup facts;
- the user's decisions, 1 to 5, with 5 as amended by R49 (shaders with packed uniforms), and the answers of 2026-10-21 (budget gate, confirmed values, the spectrum retry, the DualSense label).

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/notes/2026-10-21-plan2b-acceptance.md README.md
git commit -m "docs: record plan 2b acceptance"
```

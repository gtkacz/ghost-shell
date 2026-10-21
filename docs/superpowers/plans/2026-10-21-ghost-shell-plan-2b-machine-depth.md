# ghost-shell Plan 2 — Part 2b: Machine Depth Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** By the end of Part 2b:
- every machine panel the spec lists has a real widget: network, the six MONITOR telemetry lanes, the process tree, the uplink globe, the silicon schematic;
- storage shows drive wear and power-on hours, sensors shows device batteries, and the processor shows CPU package power;
- full-width graphs draw as one native scene-graph node each, so no view pays one `Rectangle` per cell;
- the default V1, V2 and MONITOR views lose their last machine placeholders;
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
  - a new C++ QML module `GhostKit.Native` draws cell graphs (bars, mirror, heat) and the dot globe as single geometry nodes;
  - `BlockGraph` keeps its public API on top of it, and the kit adds `MirrorGraph` and `HeatGraph`;
  - tab strips move into the panel header row (P18);
  - layout splits may set their own `gutter`.
- **Widgets:**
  - new: `telemetry-lane`, `network`, `process-tree`, `uplink`, `silicon`;
  - changed: `memory` (reads `procs`), `processor` (PKG), `storage` (wear, hours), `sensors` (batteries).

**Tech Stack:**
- Rust 1.98. New crates:
  - `maxminddb` with its `mmap` feature, so a 100 MB+ database costs page cache, not RSS;
  - nothing else. NetworkManager, UPower, UDisks2 and KDE Connect go through the existing zbus 6. Tailscale's local API is plain HTTP/1.0 over `tokio::net::UnixStream`. Ping uses `nix` sockets.
- C++17, Qt 6.11 (`Qt6::Quick`, `qt_add_qml_module`), CMake 4.3 and Ninja, built into `target/native/` (already git-ignored).
- Quickshell 0.2.1, QtQuick.Shapes, ShaderEffect (`qsb-qt6`).
- Node 24 for `.mjs` tests.
- `uv run --no-project --with pyshp --with pillow python -I` for the land-mask generator.
- ImageMagick `compare` for pixel checks.

**Spec:** `docs/superpowers/specs/2026-10-20-ghost-shell-design.md`: §3 the machine, §4 stack, §5 visual language, §6.2–6.3 layouts and panels, §12 data sources, §13 extensibility, §15 performance. The mockup `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html` is the visual truth, and its drawing functions (`blocks`, `mirror`, `heat`, `drawGlobe`, `drawDie`, `buildTree`/`drawTree`, the `C.lane`/`C.net`/`C.scope` templates) are the reference for every renderer below. Read it together with:
- Plan 2a, `docs/superpowers/plans/2026-10-21-ghost-shell-plan-2-dashboard.md`, which this plan's outline closes;
- the Plan 2a ledger `.superpowers/sdd/2026-10-21-ghost-shell-plan-2-dashboard/progress.md`, rulings R1–R37 and carry-forwards C1–C20.

**Prerequisite:** Plan 2a is complete. Tasks 15–18 must be merged (media widget, audio rows, default V2 and MONITOR, install), because:
- this plan edits the trees Task 17 writes;
- it uses Task 15's public `ClickArea` (R13);
- it uses the `fake/` split that R4(g) foresaw.

This plan was written against `main` at `fbc6a86`, with Task 15 in progress.

## Decisions (user, 2026-10-21)

The four open questions from the Plan 2a outline, plus one on the renderer:

1. **Endpoint labels:**
   - the default is the endpoint's **ASN organisation** from the offline DB-IP ASN Lite file;
   - `[uplink.labels]` in config.toml maps an organisation name (case-insensitive) or `AS<number>` either to fixed text or to the special value `"process"`, which shows the local process that owns the socket;
   - the `/proc/*/fd` walk runs only for endpoints whose label resolves to `"process"`.
2. **Land mask:** a repo script rasterises Natural Earth 1:50m land into a PNG. **Both the script and the PNG are committed.** Install needs no network.
3. **RAPL:** **world-readable** `energy_uj` (0444), set by a udev rule the user installs with sudo. The rule file states the PLATYPUS (CVE-2020-8694) trade-off in its comment.
4. **Tailscale peers:** a peer with a **direct** connection is located by GeoIP of its current public endpoint (`CurAddr` in Tailscale's status) and labelled `TAILNET:<HOST>`. A relayed (DERP) peer is left off the globe and still counted in the VPN line.
5. **Renderer (P17):** a **native C++ QML module** (`GhostKit.Native`), not packed shader uniforms. It needs `qt6-qtbase-devel` and `qt6-qtdeclarative-devel`, installed once by the user (Task 1).

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

- **Process rules** (Plan 2a ledger `process-rules.md`, binding for every agent):
  - never `rm -rf` (use `rm -r --`), `pkill`/`killall`, or `git reset`/`rebase`/`push`/`branch -D`;
  - stage by explicit path;
  - Python only as `uv run --no-project python -I …`;
  - private buses only as `dbus-run-session --config-file=<repo>/scripts/private-bus.conf -- …`;
  - dev daemons use their own socket, `--config <temp>` and `--widgets-dir "$repo/widgets"`;
  - never touch the running `ghostd`/`ghost-shell` services or `~/.config/ghost-shell/config.toml`, except in Task 19.
- **Native module:**
  - C++ lives in `shell/native/` and builds with `make native` into `target/native/qml/GhostKit/Native/`;
  - the smoke script and the service add `target/native/qml` (dev) or the installed kit (install) to `QML_IMPORT_PATH`;
  - widgets never import `GhostKit.Native`; only GhostKit's own QML files do;
  - `.gitignore` is not edited.
- **Native drawing:**
  - one `QSGGeometryNode` per item, with vertex colours (`QSGVertexColorMaterial`);
  - geometry is rebuilt only when an input property changes;
  - every rect snaps to whole device pixels (`window()->effectiveDevicePixelRatio()`);
  - colour rules are ported verbatim from `shell/kit/GhostKit/lib/segments.mjs` (and the mockup's `mirror`/`heat`), and C++ tests pin the same numbers as the node tests.
- **History keys:**
  - `load`, `ccd0`, `ccd1`, `core<N>` (one per physical core) belong to `cpu`;
  - `gpu_load`, `temp_gpu` belong to `gpu`;
  - `mem_used` (GiB) belongs to `mem`;
  - `temp_cpu` belongs to `sensors`;
  - `disk_read`, `disk_write` (B/s summed over live drives) and `temp_nvme` (hottest live NVMe, °C) belong to `disks`;
  - `net_down`, `net_up` (B/s on the default-route interface) belong to `net`.

  A key is pushed only on ticks its own topic is published, so the shell's appended series (Plan 2a R5) and the daemon's stay identical.
- **Network access added by this plan:**
  - ICMP echo to `network.ping` (default `1.1.1.1`, spec §12) every 5 s, only while `net.links` is wanted;
  - Tailscale's local socket (`/run/tailscale/tailscaled.sock`);
  - DB-IP downloads only through `scripts/fetch-geoip.sh` with the user's consent;
  - the Natural Earth download only while generating the land mask (Task 11, with consent).

  No reverse DNS. Tests never touch the network or the system bus.
- **Do not wake hardware (the R34 pattern):**
  - UDisks2 is read from cached properties and `SmartGetAttributes` on NVMe controllers only, never `SmartUpdate`, and never a method call on an ATA drive;
  - KDE Connect battery values are read, and no refresh is ever requested from the phone.
- **Services that come and go (the R30 pattern):** every D-Bus feed reads the full state when its service's name gains an owner (`NameOwnerChanged`), not only at start. An absent service publishes an empty value and waits. A feed never ends (Plan 2a R4(d)).
- **Long cadences use the wall clock (R25):** `disks.health` (10 min) and the KDE Connect poll (60 s) decide due-ness on wall-clock seconds, and every sleep is capped at 60 s while the topic is wanted.
- **Measure every new collector and feed:**
  - its cadence;
  - wall and on-CPU time per run;
  - its RSS delta;
  - for `uplink`, the process-walk cost with an override in force.

  The controller records the figures in README's Performance section (user rule).
- **Ask the user before:**
  - `sudo dnf install qt6-qtbase-devel qt6-qtdeclarative-devel` (Task 1);
  - the Natural Earth download (Task 11);
  - running `scripts/fetch-geoip.sh` (Task 10);
  - installing the RAPL udev rule with sudo (Task 14);
  - anything in Task 19 that touches the live desktop beyond Plan 2a R17's procedure.
- **Files ≤ 800 lines.** `crates/ghostd/src/fake.rs` (740 today) becomes a `fake/` module in Task 2, before any 2b fake is added (Plan 2a R4(g)).

## Review Focus

1. **Counters that jump:**
   - `/proc/net/dev` counters reset when an interface goes down and up;
   - a VPN interface appears or disappears between samples;
   - RAPL `energy_uj` wraps at `max_energy_range_uj`.

   A rate must never read negative or as a burst of exabytes. *Tests: Tasks 5 and 14.*
2. **Endpoints with nothing to show:**
   - loopback, RFC 1918, link-local, CGNAT `100.64/10`, multicast, ULA, and IPv4-mapped IPv6 are never shown;
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
   - the dead WD NVMe reports `None` without breaking the rows of the other drives. *Tests: Task 16.*

---

### Task 1: Native cell graphs (`GhostKit.Native`) and `BlockGraph` on them

**Files:**
- Create:
  - `shell/native/CMakeLists.txt`, `shell/native/cellgraph.h`, `shell/native/cellgraph.cpp`;
  - `shell/native/cellcolors.h` (pure colour and lit-cell rules) and `shell/native/tests/cellcolors_test.cpp`;
  - `shell/kit/GhostKit/MirrorGraph.qml`, `shell/kit/GhostKit/HeatGraph.qml`.
- Modify:
  - `shell/kit/GhostKit/BlockGraph.qml` (body only) and `shell/kit/GhostKit/qmldir`;
  - `Makefile` (`native` target; `release`, `smoke`, `smoke-layout` and `test` depend on it), `scripts/qml-smoke.sh` (`QML_IMPORT_PATH` gains `$repo/target/native/qml`), `scripts/install.sh` (copy the built module into `$shell_dir/kit/GhostKit/Native`, and refuse when it is missing, as it does for `.qsb`);
  - `shell/host/Gallery.qml` (`GHOST_GALLERY_LANES=1` shows six full-width graphs fed by a 1 Hz synthetic series);
  - `README.md` Build (the two devel packages).

**Interfaces:**
- Produces:
  - **QML module `GhostKit.Native 1.0`**, internal to the kit. `CellGraph` is a `QQuickItem` (`QML_ELEMENT`) with these properties:
    - `mode: CellGraph.Bars | Mirror | Heat`;
    - `series: var`: one array for Bars, `[up, down]` for Mirror, an array of rows (top row first) for Heat. `null`/`NaN` samples are missing;
    - `lo`, `hi`, `warnAt`, `critAt` (fractions of lo..hi), `minLit`;
    - `columns` (0 fits `cellWidth` columns into the width; leftover width on the left, newest sample right);
    - `cellWidth`, `cellHeight`, `gap` (logical px, already scaled by `Theme.d`);
    - `sqrtScale` (Mirror);
    - `live` (false draws every cell unlit);
    - `signalColor`, `hiColor`, `warnColor`, `critColor`.
  - **`cellcolors.h`**, ported from `segments.mjs`:
    - `enum class Level { Ok, Warn, Crit }`;
    - `Level level(double fraction, double warnAt, double critAt)`;
    - `int litCells(double value, double lo, double hi, int rows, int minLit)`;
    - `QColor barCell(Level, int row, int lit, int rows, const Palette&)`, the same as `cellColor`;
    - `QColor mirrorCell(bool upper, int row, int lit, int rows, const Palette&)`: upper lit `hi` at (45 + 40·row/rows) %, lower lit `signal` at (45 + 50·row/rows) %, unlit `signal` 5 %;
    - `QColor heatCell(double fraction, Level, const Palette&)`: ok is `signal` at (7 + 80·fraction) %, warn and crit solid. A missing sample is `signal` 6 %.
  - **`BlockGraph`** keeps its whole public API (`values`, `lo`, `hi`, `cols`, `cellW`, `cellH`, `gap`, `warnAt`, `critAt`, `minLit`, `sourceLive`, `live`) and becomes one `CellGraph { mode: Bars }`.
  - **`MirrorGraph { up; down; floor; cellW: 4; cellH: 2; gap: 1; sourceLive }`**, public:
    - Mirror mode with `sqrtScale: true`;
    - `max = max(floor, every sample of both) × 1.1` (mockup `nmax`/`imax`);
    - `half = floor(height / 2)`, with the lower half starting at `half + 1` (mockup `mirror`).
  - **`HeatGraph { rows; lo: 0; hi: 100; warnAt: Theme.loadWarn; critAt: Theme.loadCrit; cellW: 4; gap: 1; sourceLive }`**, public: Heat mode, row height `(h − (rows − 1)·gap) / rows` (mockup `heat`).

- [ ] **Step 1: Ask the user to install the build dependencies**

Ask for `sudo dnf install qt6-qtbase-devel qt6-qtdeclarative-devel`. The packages must match the installed Qt 6.11.2. Stop the task if the user declines.

- [ ] **Step 2: Record the baseline before touching `BlockGraph`**

With the Plan 1 `BlockGraph`, take two snapshots:
- `GHOST_VIEW=gallery GHOST_SNAPSHOT=/tmp/gs-2b-t1/gallery-before.png`;
- the default V1 with `--fake` at `GHOST_FORCE_U=2` → `/tmp/gs-2b-t1/v1-before.png`.

Then run the six-lane gallery page (bars only, since the old graph has no mirror or heat) and measure:
- page creation time, from the log line the page prints on `Component.onCompleted`;
- RSS;
- `scripts/measure-cpu.sh <qs pid> 60`.

- [ ] **Step 3: Write the failing C++ tests**

```cpp
// shell/native/tests/cellcolors_test.cpp — plain asserts, run by CTest
assert(litCells(50, 0, 100, 10, 0) == 5); assert(litCells(0.1, 0, 100, 10, 1) == 1); assert(litCells(NAN, 0, 100, 10, 1) == 0);
assert(level(0.74, 0.75, 0.9) == Level::Ok); assert(level(0.75, 0.75, 0.9) == Level::Warn); assert(level(0.9, 0.75, 0.9) == Level::Crit);
assert(barCell(Level::Ok, 0, 0, 10, P) == alpha(P.signal, 0.06)); assert(barCell(Level::Ok, 4, 5, 10, P) == P.hi);
assert(barCell(Level::Ok, 2, 5, 10, P) == alpha(P.signal, 0.45)); assert(barCell(Level::Warn, 0, 5, 10, P) == P.warn);
assert(mirrorCell(true, 0, 3, 10, P) == alpha(P.hi, 0.45)); assert(mirrorCell(false, 5, 6, 10, P) == alpha(P.signal, 0.70));
assert(mirrorCell(true, 7, 3, 10, P) == alpha(P.signal, 0.05));
assert(heatCell(0.5, Level::Ok, P) == alpha(P.signal, 0.47)); assert(heatCell(0.95, Level::Crit, P) == P.crit);
```

The existing `segments.test.mjs` cases stay the JS half of the same contract.

- [ ] **Step 4: Run them and watch them fail**

Run: `make native && ctest --test-dir target/native --output-on-failure`
Expected: the build fails (no `cellcolors.h`).

- [ ] **Step 5: Implement `cellcolors.h`, `CellGraph`, the CMake module, the three QML files and the build plumbing**

`updatePaintNode` builds one `QSGGeometryNode`: 6 vertices per cell, unlit cells included. `CellGraph` marks its geometry dirty only from its property setters and `geometryChange`.

- [ ] **Step 6: Run every check**

Run: `make native && ctest --test-dir target/native --output-on-failure && node --test 'shell/tests/*.test.mjs' && make smoke && make smoke-layout`
Expected: PASS, then `smoke: ok` twice. The shell log has no `module "GhostKit.Native" is not installed`.

- [ ] **Step 7: Check the pixels and measure**

Take the same two snapshots as Step 2, as `…-after.png`. Then:
- `compare -metric AE -fuzz 1% before.png after.png null:` reports `0` for both pairs;
- the six-lane page, now with two bars, two mirror and two heat graphs, meets these targets **[call]**:
  - page creation ≤ 100 ms;
  - RSS ≤ 30 MiB above the gallery without the page;
  - shell CPU ≤ 0.30 % above the gallery idle over 60 s.

Report the before/after figures for README.

- [ ] **Step 8: Commit**

```bash
git add shell/native Makefile scripts/qml-smoke.sh scripts/install.sh README.md
git commit -m "build(native): add the GhostKit native module and its build"
git add shell/kit/GhostKit/BlockGraph.qml shell/kit/GhostKit/MirrorGraph.qml shell/kit/GhostKit/HeatGraph.qml shell/kit/GhostKit/qmldir shell/host/Gallery.qml
git commit -m "perf(kit): draw cell graphs as one scene-graph node"
```

---

### Task 2: `procs` and `procs.tree` topics, split out of `mem`

**Files:**
- Create:
  - `crates/ghostd/src/collect/procs.rs`, holding the process scan moved out of `mem.rs`, its constants and its tests;
  - `crates/ghostd/src/fake/{mod.rs,sampled.rs,feeds.rs}`, from `fake.rs` (no behaviour change, its own commit first);
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
    - it builds `tree` on scans 0, 5, 10, … (`TREE_EVERY_SCANS = 5`, so every 10 s; spec: 5–20 s);
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
#[test] fn procs_and_tree_publish_only_when_wanted() { /* sampler: wanted [procs] → procs only; wanted [procs.tree] → tree on its scans only; neither → source not sampled (counted) */ }
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

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && make smoke`
Expected: PASS, then `smoke: ok`.

- [ ] **Step 6: Measure**

- Release daemon on its own socket, with `topic-tap.py` subscribed to `procs.tree` for 60 s.
- Report:
  - scan wall and CPU time, before (Plan 2a figure) and after;
  - tree serialisation size in bytes;
  - the daemon's CPU % with `[procs]` and with `[procs, procs.tree]`.
- Crop the memory panel from a V1 snapshot. `compare -metric AE` against the Plan 2a crop gives 0.

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
  - `crates/ghostd/src/fake/sampled.rs`, `crates/ghostd/tests/{cpu.rs,server_handshake.rs}`;
  - `shell/kit/GhostKit/lib/history.mjs`, `shell/tests/history.test.mjs`.

**Interfaces:**
- Produces:
  - `CpuSnapshot.cores: Vec<CoreLoad>`, with `CoreLoad { cpus: Vec<u16>, load: f32 }`: one entry per physical core, ordered by its lowest cpu id; `load` is the mean of its threads (spec §6.2: "SMT pairs averaged").
  - **`sampler::history_topic(key: &str) -> Option<&'static str>`** replaces `HISTORY_KEYS`. It maps every key in Global Constraints to its topic. `core<N>` matches any decimal `N`, and `net_*` returns `net` (its recorder arrives in Task 5).
  - `State::subscribe` sends every series whose `history_topic` is subscribed.
  - Recorders:
    - `temp_cpu` is the first reading whose id is in `CPU_PACKAGE_SENSORS = ["k10temp/Tctl", "coretemp/Package id 0"]`;
    - `temp_gpu` is `gpu.temp_c`, `None` while the GPU is absent;
    - `temp_nvme` is the maximum `temp_c` over live drives whose `kernel_name` starts with `nvme`, or `None`;
    - `mem_used` is `used_kib / 1048576`;
    - `disk_read`/`disk_write` sum `read_bps`/`write_bps` over live drives.
  - **`history.mjs`:**
    - `topicOfKey(key) -> string | null` mirrors `history_topic`;
    - `withData` records the same samples for `cpu`, `gpu`, `mem`, `sensors`, `disks` (and `net` from Task 5);
    - `fromHistory` marks a topic as replaying when any of its keys has samples.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn cores_average_their_smt_siblings() { /* fake_9950x3d: cpu0 load 10, cpu16 load 30 */ assert_eq!(snapshot.cores[0], CoreLoad { cpus: vec![0, 16], load: 20.0 }); assert_eq!(snapshot.cores.len(), 16); }
#[test] fn every_key_has_one_topic() { for (k, t) in [("load","cpu"),("core15","cpu"),("gpu_load","gpu"),("temp_gpu","gpu"),("mem_used","mem"),("temp_cpu","sensors"),("disk_read","disks"),("temp_nvme","disks"),("net_up","net")] { assert_eq!(history_topic(k), Some(t)); } assert_eq!(history_topic("corex"), None); }
#[test] fn temp_cpu_is_recorded_only_when_sensors_is_published() { /* wanted [disks] → sensors sampled for drive temps, but no temp_cpu sample */ }
#[test] fn temp_nvme_is_the_hottest_live_nvme() { /* nvme 38, nvme dead with None, sda 30 → 38 */ }
#[test] fn a_subscriber_to_mem_gets_mem_used_history() { /* server_handshake: subscribe [mem] → History has exactly mem_used */ }
```

```js
test('every key maps to one topic', () => { assert.equal(topicOfKey('core3'), 'cpu'); assert.equal(topicOfKey('mem_used'), 'mem'); assert.equal(topicOfKey('nope'), null); });
test('cpu data appends one sample per core', () => { const t = withData(fromHistory({}), { topic: 'cpu', seq: 1, data: { load: 10, ccds: [], cores: [{ cpus: [0, 16], load: 20 }] } }, 300); assert.deepEqual(t.series.core0, [20]); });
test('disks data sums live drives and keeps the hottest nvme', () => { /* two live nvme 1e6 + 2e6 B/s, 38° and 41°, a dead one → disk_read [3e6], temp_nvme [41] */ });
test('a gap before mem data is filled with nulls', () => { /* seq 1 then 4 → mem_used [a, null, null, b] */ });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement**

The server's History message now carries about 30 series × 300 samples.

- [ ] **Step 4: Run every check, then measure the History message**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && node --test 'shell/tests/*.test.mjs' && make smoke`
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
  - A negative or non-finite value is a config error: `<path>: gutter must be a finite number ≥ 0`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn a_split_reads_its_gutter() { let n: LayoutNode = toml::from_str("direction='column'\ngutter=6\nchildren=[{widget='a'},{widget='b'}]").unwrap(); assert!(matches!(n, LayoutNode::Split { gutter: Some(g), .. } if g == 6.0)); }
#[test] fn a_negative_gutter_names_its_node() { let e = load_str_with_user("[layout.views.v9]\ndirection='row'\ngutter=-1\nchildren=[]", "u").unwrap_err(); assert!(e.to_string().contains("layout.views.v9: gutter must be a finite number ≥ 0")); }
```

```js
test('a split gutter spaces only its own children', () => { const tree = { direction: 'column', gutter: 6, children: [{ widget: 'a' }, { direction: 'row', children: [{ widget: 'b' }, { widget: 'c' }] }] }; const r = computeRects(tree, { x: 0, y: 0, w: 100, h: 106 }, 12); assert.equal(pick(r, 'a').h, 50); assert.equal(pick(r, 'b').y, 56); assert.equal(pick(r, 'c').x, 56); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghost-proto --test layout_node && cargo test -p ghostd --test config_load && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check**

Run the same commands plus `make smoke-layout`.
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

**Interfaces:**
- Produces:
  - `topics::NET = "net"`, added to `BUILTIN`.
  - `NetSnapshot { default_iface: Option<String>, rx_bps: u64, tx_bps: u64, interfaces: Vec<NetIface> }` and `NetIface { name: String, oper_up: bool, rx_bps: u64, tx_bps: u64 }`.
    - The headline `rx_bps`/`tx_bps` belong to the default-route interface. That is the `/proc/net/route` entry with destination and mask `00000000`, the `RTF_UP` flag and the lowest metric.
    - `lo` is skipped.
    - `oper_up` is `sys/class/net/<if>/operstate == "up"`.
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
  - `crates/ghostd/tests/links.rs`, `crates/ghostd/tests/fixtures/tailscale-status.json` (trimmed from a real `/localapi/v0/status`, with addresses replaced by documentation ranges).
- Modify:
  - `crates/ghost-proto/src/{snapshot.rs,lib.rs,config.rs}`, `crates/ghostd/src/config/default.toml`, `crates/ghostd/src/config/load.rs`;
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
    - `async fn status(&self) -> Option<TailscaleStatus>`: HTTP/1.0 `GET /localapi/v0/status` with `Host: local-tailscaled.sock`, a 2 s timeout, and `None` when the socket is missing or refused;
    - `TailscaleStatus { backend_state: String, self_ips: Vec<IpAddr>, peers: Vec<TailscalePeer> }`;
    - `TailscalePeer { host_name: String, online: bool, cur_addr: Option<SocketAddr> }`.
  - **`feeds::links::ping`:** `trait Pinger: Send + Sync { fn ping(&self, host: Ipv4Addr) -> BoxFuture<Option<Duration>> }`. `IcmpPinger` opens an unprivileged `SOCK_DGRAM`/`IPPROTO_ICMP` socket via `nix` in `tokio::io::unix::AsyncFd`, waits 2 s, and returns `None` on any error.
  - **`feeds::links`:**
    - `feed() -> Feed` (system bus, `/run/tailscale/tailscaled.sock`, `IcmpPinger`);
    - `feed_on(bus: zbus::Connection, tailscale: TailscaleClient, pinger: Arc<dyn Pinger>) -> Feed` for tests.
  - **Behaviour, all of it only while `net.links` is wanted (`until_wanted`/`until_unwanted`; nothing runs otherwise):**
    - **LAN:** NM `Devices` of type ethernet (1) or wifi (2) that NM manages. State 100 → `Up`, 40–90 → `Connecting`, anything else → `Down`. The address is `Ip4Config.AddressData[0]`. The speed is `Device.Wired.Speed`.
    - **VPNs:** every saved connection of type `vpn` or `wireguard` (`Settings.ListConnections` + `GetSettings`; `kind` is the last segment of `vpn.service-type`, or `wireguard`), merged with the active connection's state: 2 → `Up`, 1 → `Connecting`, otherwise `Down`. `iface` is the active connection's `Devices[0]` `IpInterface`. Type `tun` (Tailscale and other external tunnels) is excluded.
    - NM is re-read on its `PropertiesChanged` (ActiveConnections, Devices), on an active connection's `StateChanged`, and in full when `org.freedesktop.NetworkManager` gains an owner.
    - **Tailscale** is read every 10 s **[call]**. `BackendState "Running"` → `Up`, `"Starting"` → `Connecting`, anything else → `Down`. `address` is the first `self_ips` entry.
    - **Ping** runs every 5 s.
    - It publishes only when the snapshot changes.
  - The fake publishes the mockup's links: `ENP14S0 192.168.15.5/24 1 GbE UP`, `OpenVPN Flxpoint tun0` up, Tailscale `100.122.159.93` with 0 of 1 peers online, and ping 12.

- [ ] **Step 1: Write the failing tests**

These run on the private dbus-daemon harness (`tests/common/bus.rs`) with a fake NM object server (zbus `#[interface]`), a fake Tailscale `UnixListener` serving the fixture, and a counting fake `Pinger`.

```rust
#[tokio::test] async fn lan_and_vpn_links_come_from_network_manager() { assert_eq!(snap.lan, [LanLink { iface: "enp14s0".into(), address: Some("192.168.15.5/24".into()), speed_mbps: Some(1000), state: LinkState::Up }]); assert_eq!(snap.vpns[0].kind, "openvpn"); assert_eq!(snap.vpns[0].iface.as_deref(), Some("tun0")); }
#[tokio::test] async fn a_saved_vpn_that_is_not_active_reads_down() { assert_eq!(vpn(&snap, "Work").state, LinkState::Down); }
#[tokio::test] async fn tun_connections_are_not_vpns() { /* NM lists tailscale0 as type tun */ assert!(snap.vpns.iter().all(|v| v.iface.as_deref() != Some("tailscale0"))); }
#[tokio::test] async fn network_manager_appearing_later_is_read_in_full() { /* fake NM registers after the feed starts */ }
#[tokio::test] async fn network_manager_absent_publishes_empty_lists_and_keeps_running() { }
#[tokio::test] async fn tailscale_status_counts_online_peers() { assert_eq!((t.peers_online, t.peers_total, t.state), (1, 2, LinkState::Up)); }
#[tokio::test] async fn a_missing_tailscale_socket_reads_none() { assert!(snap.tailscale.is_none()); }
#[tokio::test(start_paused = true)] async fn nothing_runs_while_unwanted() { /* demand never contains net.links: 0 NM calls, 0 status requests, 0 pings after 60 s */ }
#[tokio::test(start_paused = true)] async fn an_empty_ping_host_never_pings() { assert_eq!(pinger.count(), 0); assert!(snap.ping_ms.is_none()); }
#[test] fn network_ping_must_be_an_ipv4_address_or_empty() { assert!(load_str_with_user("[network]\nping='x'", "u").unwrap_err().to_string().contains("network.ping: \"x\" is not an IPv4 address")); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test links --test config_load`
Expected: FAIL.

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

```bash
git add crates
git commit -m "feat(ghostd): add network links, Tailscale state and ping"
```

---

### Task 7: `telemetry-lane` widget

**Files:**
- Create: `widgets/telemetry-lane/{manifest.toml,Widget.qml,lanes.mjs}`, `shell/tests/lanes.test.mjs`
- Modify:
  - `shell/kit/GhostKit/lib/format.mjs` and `Fmt.qml` (`rateShort`);
  - `shell/kit/GhostKit/Glyph.qml` (`up` and `down` chevrons from the mockup's `IC.up` `M2.6 7.6 6 4.6l3.4 3` and `IC.down` `M2.6 4.4 6 7.4l3.4-3`);
  - `shell/tests/format.test.mjs`.

**Interfaces:**
- Consumes: `HeatGraph`, `MirrorGraph`, `BlockGraph` (Task 1); the history keys (Tasks 3 and 5); `net.links` (Task 6).
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
2. Run `node --test 'shell/tests/*.test.mjs' && GHOST_CONFIG=/tmp/gs-2b-t7.toml scripts/qml-smoke.sh`.
   Expected: PASS and `smoke: ok`.
3. Snapshot MONITOR with `--fake` at native `u` and compare it with the mockup's `L-mon` lanes. Nothing overflows.
4. Measure that MONITOR over 60 s (CPU, RSS, view creation time) for README. This lifts C14 for lanes.

- [ ] **Step 5: Commit**

```bash
git add widgets/telemetry-lane shell/kit/GhostKit shell/tests
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

Run `GHOST_CONFIG=<temp config>`. The temp config places `network` 164 in V2's right column, and `network {style="scope"}` under a placeholder in V1's right column. Expected: PASS and `smoke: ok`.

Snapshot both and compare them with the mockup's `p-net` and `C.scope`.

- [ ] **Step 4: Commit**

```bash
git add widgets/network shell/tests/network.test.mjs
git commit -m "feat(widgets): add the network widget in panel and scope styles"
```

---

### Task 9: `process-tree` widget

**Files:**
- Create: `widgets/process-tree/{manifest.toml,Widget.qml,tree.mjs}`, `shell/tests/tree.test.mjs`

**Interfaces:**
- Consumes: `procs.tree` (Task 2), `ClickArea` (Plan 2a R13), `Theme`, `Panel`.
- Produces:
  - **Manifest:** `id = "process-tree"`, `requires = ["procs.tree"]`, `animates = false`. The mockup tree has no pulses.
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
    - labels are rotated `Text`s at 8 px weight 600, flipped when `cos(angle) < 0`;
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
test('a thousand processes lay out once in under 20 ms', () => { const t0 = performance.now(); layoutRadial(buildTree(SYNTHETIC_1000, new Set()), 600, 500); assert.ok(performance.now() - t0 < 20); });
test('hit test picks the nearest node within the slop', () => { assert.equal(hitTest(NODES, 203, 201, 6), '1:systemd'); assert.equal(hitTest(NODES, 50, 50, 6), null); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement**

- [ ] **Step 4: Run the tests, a smoke and the click check**

1. Run `node --test` and `scripts/qml-smoke.sh` with a temp config that puts `process-tree` in MONITOR's right rail.
2. Snapshot it with `--fake`, which has the 40-process synthetic tree, and compare it with the mockup's `p-tree`.
3. Run the click check headless, in the offscreen harness only (Plan 2a R52): a click on a node collapses it and a second click expands it.
4. Report the rebuild time per `procs.tree` message.

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
  - `crates/ghostd/tests/uplink.rs`, `crates/ghostd/tests/fixtures/proc-net-{tcp,tcp6,udp,udp6}.txt`;
  - `scripts/fetch-geoip.sh`.
- Modify:
  - `Cargo.toml` and `crates/ghostd/Cargo.toml` (`maxminddb`, features `["mmap"]`);
  - `crates/ghost-proto/src/{snapshot.rs,lib.rs,config.rs}`, `crates/ghostd/src/config/{default.toml,load.rs}`;
  - `crates/ghostd/src/{main.rs,feeds/mod.rs,fake/feeds.rs}`, `crates/ghostd/tests/config_load.rs`.

**Interfaces:**
- Consumes: `feeds::tailscale::TailscaleClient` (Task 6).
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
    - `fn is_public(ip: &IpAddr) -> bool` excludes loopback, unspecified, RFC 1918, link-local, CGNAT `100.64.0.0/10`, multicast, broadcast, documentation, ULA `fc00::/7` and `fe80::/10`.
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
    8. keep `MAX_ENDPOINTS = 8` **[outline call]**;
    9. publish on change.

    Nothing is read while the topic is unwanted.
  - **`scripts/fetch-geoip.sh`:**
    - downloads `https://download.db-ip.com/free/dbip-{city,asn}-lite-YYYY-MM.mmdb.gz` for the current month, falling back to the previous month on a 404;
    - un-gzips them into `$XDG_DATA_HOME/ghost-shell/geoip/` through a temp file and `mv`;
    - prints the sizes;
    - never runs from `install.sh`.
  - The fake publishes the mockup's six `PEERS`, labels included.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn established_tcp_and_connected_udp_are_kept() { let s = parse_proc_net(TCP_FIXTURE, false, false); assert_eq!(s.len(), 3); assert_eq!(s[0], RemoteSocket { remote: "140.82.112.4".parse().unwrap(), uid: 1000, inode: 5_123_456 }); }
#[test] fn a_v4_mapped_v6_remote_reads_as_v4() { assert_eq!(parse_proc_net(TCP6_FIXTURE, true, false)[0].remote, "151.101.1.69".parse::<IpAddr>().unwrap()); }
#[test] fn private_and_special_ranges_are_never_shown() { for ip in ["127.0.0.1","10.1.2.3","172.16.0.1","192.168.15.5","169.254.1.1","100.122.159.93","224.0.0.1","fd7a:115c::1","fe80::1","::1"] { assert!(!is_public(&ip.parse().unwrap()), "{ip}"); } assert!(is_public(&"1.1.1.1".parse().unwrap())); }
#[test] fn an_override_by_asn_beats_one_by_name() { assert_eq!(label_for(Some(&(16509, "Amazon.com, Inc.".into())), &ovr(&[("AS16509", "AWS"), ("amazon.com, inc.", "process")]), None, ip()), "AWS"); }
#[test] fn process_overrides_fall_back_to_the_org() { assert_eq!(label_for(Some(&(20940, "Akamai International B.V.".into())), &ovr(&[("AKAMAI INTERNATIONAL B.V.", "process")]), None, ip()), "AKAMAI INTERNATIONAL B.V."); assert_eq!(label_for(Some(&(20940, "Akamai International B.V.".into())), &ovr(&[("akamai international b.v.", "process")]), Some("spotify"), ip()), "SPOTIFY"); }
#[test] fn the_label_chain_ends_at_the_ip() { assert_eq!(label_for(None, &ovr(&[]), None, "9.9.9.9".parse().unwrap()), "9.9.9.9"); }
#[test] fn owners_are_found_through_fd_links_and_only_for_the_socket_uid() { /* FakeRoot: proc/42/fd/3 -> socket:[5123456] owned by 1000, proc/43 owned by 0 with the same link */ assert_eq!(owners.names(&[(5_123_456, 1000)])[&5_123_456], "spotify"); }
#[tokio::test(start_paused = true)] async fn endpoints_group_rank_and_cut_at_eight() { /* 12 orgs with 1..=12 sockets → 8 endpoints, first has 12, total 12 */ }
#[tokio::test(start_paused = true)] async fn a_direct_tailnet_peer_is_located_and_a_relayed_one_is_not() { assert!(snap.endpoints.iter().any(|e| e.tailnet && e.label == "TAILNET:PHONE")); assert_eq!(snap.endpoints.iter().filter(|e| e.tailnet).count(), 1); }
#[tokio::test(start_paused = true)] async fn no_database_still_publishes() { assert_eq!((snap.endpoints.len(), snap.attribution.as_str()), (0, "")); }
#[tokio::test(start_paused = true)] async fn nothing_is_read_while_unwanted() { assert_eq!(geo.lookups(), 0); }
#[test] fn uplink_labels_reject_empty_values() { assert!(load_str_with_user("[uplink.labels]\n\"AS1\"=''", "u").unwrap_err().to_string().contains("uplink.labels")); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test uplink --test config_load`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 4: Ask the user to run `scripts/fetch-geoip.sh`, then check live and measure**

The script downloads two files from download.db-ip.com, which is network access beyond the 2a list.

Then run a dev daemon with a temp config that sets `uplink.labels` to one real organisation → `"process"`, plus `topic-tap.py` on `uplink` for 30 s. Report:
- the endpoints seen;
- per-cycle wall and CPU time, without and with the override;
- the fd-walk time and how many fds it read;
- the daemon's RSS with the databases mapped.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates scripts/fetch-geoip.sh
git commit -m "feat(ghostd): publish live connection endpoints with offline geolocation"
```

---

### Task 11: Land mask and the native globe

**Files:**
- Create:
  - `scripts/land-mask.py`;
  - `widgets/uplink/land-mask.png`, committed: 2048 × 1024, 1-bit, equirectangular, 180° W at the left, 90° N at the top;
  - `shell/native/globe.{h,cpp}`, `shell/native/globemath.h`, `shell/native/tests/globe_test.cpp`.
- Modify: `shell/native/CMakeLists.txt`

**Interfaces:**
- Produces:
  - **`scripts/land-mask.py`:**
    - downloads `https://naciscdn.org/naturalearth/50m/physical/ne_50m_land.zip` and checks a SHA-256 that the script records;
    - rasterises the polygons with pyshp and Pillow `ImageDraw.polygon`;
    - writes the PNG;
    - runs as `uv run --no-project --with pyshp --with pillow python -I scripts/land-mask.py <out.png>`.
  - **`globemath.h`:**
    - `fibonacciPoint(int i, int n) -> LatLon`;
    - `unitVector(LatLon) -> Vec3`;
    - `view(Vec3, double rot, double tilt) -> Vec3`;
    - `slerp(Vec3, Vec3, double t) -> (Vec3, double omega)`;
    - `arcAltitude(double omega, int i, int segments) = 1 + (0.08 + 0.22·omega/π)·sin(π·i/segments)`.

    All are mockup `drawGlobe` maths.
  - **`Globe`** (`GhostKit.Native`, `QQuickItem`):
    - **inputs:**
      - `landMask: url`, read once into the land flags of `points` Fibonacci points (default 3200, mockup `N`);
      - `elapsedMs`;
      - `unit` (`Theme.u`);
      - `hasHome`, `homeLat`, `homeLon`;
      - `endpoints: var` (`[{ lat, lon }]`);
      - `signalColor`, `hiColor`.
    - **drawing, one geometry node per frame, mockup constants named in `globe.cpp`:**
      - `small = min(w, h) < 240·unit`, and `R = min(w, h) / 2 − (small ? 10 : 30)·unit`;
      - `rot = 0.6 + elapsedMs · 0.00009`, and `tilt = 0.38`;
      - the outline circle at `R + 1`, `signal` 20 %, 0.8 width;
      - land dots `max(1.3·unit, R/70)` square, `signal` at `.2 + .75·z`;
      - sea dots at half size, `signal` at `.07·z`;
      - arcs from home in 40 segments at `arcAltitude`, `hi` 50 %, width `(small ? .7 : 1)·unit`, drawn as triangle strips;
      - a packet per arc, a 3-unit `hi` square at segment `floor(((elapsedMs·0.00025 + k·0.37) % 1)·40)`;
      - 4-unit `hi` end squares;
      - the home marker: a 5-unit `signal` square with an expanding outline pulse over 1800 ms (`6 + 16·p` units, alpha `1 − p`).
    - **output:** `readonly labelAnchors: var`, `[{ x, y, visible }]`, one per endpoint, then one for home (`x + 9`, `y − 6` offsets are applied by the widget).

- [ ] **Step 1: Ask the user to allow the Natural Earth download** (public domain, about 1 MB).

- [ ] **Step 2: Write the failing C++ tests**

```cpp
assert(landAt(mask, -23.55, -46.63)); assert(!landAt(mask, -25.0, -44.5)); assert(landAt(mask, 51.5, -0.12)); assert(!landAt(mask, 0.0, -30.0)); assert(landAt(mask, -25.0, 135.0)); assert(!landAt(mask, 35.0, -40.0));
auto p0 = fibonacciPoint(0, 3200); assert(std::abs(p0.lat - 90.0) < 1e-9);
auto [mid, w] = slerp(unitVector({-23.55, -46.63}), unitVector({51.5, -0.12}), 0.5); assert(std::abs(norm(mid) - 1.0) < 1e-9); assert(w > 1.4 && w < 1.6); // São Paulo–London ≈ 9 480 km ≈ 1.49 rad
assert(std::abs(arcAltitude(M_PI, 20, 40) - 1.30) < 1e-9); assert(arcAltitude(1.0, 0, 40) == 1.0);
```

- [ ] **Step 3: Run them and watch them fail**

Run: `make native && ctest --test-dir target/native --output-on-failure`
Expected: FAIL.

- [ ] **Step 4: Generate the mask, implement `globemath.h` and `Globe`, then run the tests**

Expected: PASS. Spec §6.3: São Paulo is land on Brazil's south-east coast, and the sea just south-east of it is not.

- [ ] **Step 5: Commit**

```bash
git add scripts/land-mask.py widgets/uplink/land-mask.png shell/native
git commit -m "feat(native): add the dot globe and its Natural Earth land mask"
```

---

### Task 12: `uplink` widget

**Files:**
- Create: `widgets/uplink/{manifest.toml,Widget.qml,uplink.mjs}`, `shell/tests/uplink.test.mjs`
- Modify: `shell/kit/GhostKit/Globe.qml` (public wrapper, the kit's only importer of `GhostKit.Native.Globe`), `shell/kit/GhostKit/qmldir`

**Interfaces:**
- Consumes: `uplink` (Task 10), `host`, `Data.config.location`, `Globe` (Task 11), `FrameClock`.
- Produces:
  - **Manifest:** `id = "uplink"`, `requires = ["uplink", "host"]`, `animates = true`, setting `dots`: int, default 3200.
  - **`uplink.mjs`:**
    - `footerText(snapshot)`:
      - `6 ACTIVE ENDPOINTS · IP GEOLOCATION BY DB-IP` normally;
      - `NO GEO-IP DATABASE · SEE README` when `attribution` is `""`;
      - `total` is the count;
    - `homeLabel(host)` → the hostname uppercased (`TKACZ-WORKSTATION`);
    - `hasHome(config)`: both `location.lat` and `location.lon` are set.
  - **Widget:**
    - a header `Uplink` with tag `scope`, the `Globe` filling the body, and labels as `Text`s (600 weight, 9 px, 8 px when small, `hi` at 75 %) at `labelAnchors` + (6, 3);
    - the home label at + (9, −6) in `signal`;
    - the footer in `dim` 9 px at (10, h − 10), hidden when small;
    - without a location there is no home, no arcs and no packets; endpoints show as end squares only;
    - while visible it holds `FrameClock.acquire()` and binds `elapsedMs`; it releases when hidden.

- [ ] **Step 1: Write the failing tests**

```js
test('footer', () => { assert.equal(footerText({ total: 6, attribution: 'IP GEOLOCATION BY DB-IP' }), '6 ACTIVE ENDPOINTS · IP GEOLOCATION BY DB-IP'); assert.equal(footerText({ total: 0, attribution: '' }), 'NO GEO-IP DATABASE · SEE README'); });
test('home', () => { assert.equal(homeLabel({ hostname: 'tkacz-workstation' }), 'TKACZ-WORKSTATION'); assert.equal(hasHome({ location: { lat: -23.55, lon: null } }), false); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run the tests, a smoke and snapshots**

Use a temp config with the São Paulo location. Snapshot the globe at V2 centre size and at V1 scope size with `--fake`. Check that:
- the São Paulo marker sits on Brazil's south-east coast;
- the continents are unmistakable at three `GHOST_FORCE_U` frames;
- the small size shows endpoint labels (spec §6.2).

Measure the V2 centre with UPLINK over 60 s (frames, CPU, RSS).

- [ ] **Step 4: Commit**

```bash
git add widgets/uplink shell/kit/GhostKit/Globe.qml shell/kit/GhostKit/qmldir shell/tests/uplink.test.mjs
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

- [ ] **Step 3: Run the smokes and the headless tab-click test** (Plan 1 R52 harness). Expected: `smoke: ok`, and the tab test passes.

Snapshot V1's scope and V2's centre and compare them with the mockup's `C.scope`/`C.centre` header rows:
- the label and tag sit on the left, the tabs on the right, on one line;
- nothing overlaps at 1280 or at native `u`.

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
    - finds the powercap zone whose `name` is `package-0` under `sys/class/powercap/*` at build time;
    - reads `energy_uj` each sample;
    - computes W = ΔµJ / Δµs;
    - wraps with `max_energy_range_uj` when the counter decreased;
    - returns `None` when the file is unreadable (logged once at info) or on the first sample.
  - `CpuSnapshot.igpu_busy_pct: Option<f32>`, read from the first `sys/class/drm/card*/device` with `vendor` `0x1002` that has `gpu_busy_percent`, else `None`.
  - **The udev rule:** `ACTION=="add|change", SUBSYSTEM=="powercap", ATTR{name}=="package-0", RUN+="/usr/bin/chmod 0444 /sys%p/energy_uj"`. Its comment says why world-readable was chosen (user decision 2026-10-21) and names PLATYPUS / CVE-2020-8694.
  - **Processor:** a fifth `Kv` `PKG` (`Fmt.watts`), between `FREQ` and `CPU FAN`, only while `package_w` is non-null. The row re-divides. The mockup has four; spec §6.3 adds package power.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn package_power_from_the_energy_delta() { /* energy 1_000_000 → 46_000_000 µJ over sample_after(1 s) */ assert_eq!(s.package_w, Some(45.0)); }
#[test] fn a_wrapped_counter_still_reads_positive() { /* max 262_143_328_850; before max − 1_000_000, after 44_000_000, 1 s */ assert_eq!(s.package_w, Some(45.0)); }
#[test] fn an_unreadable_counter_hides_package_power() { /* no energy_uj */ assert_eq!(s.package_w, None); }
#[test] fn the_first_sample_has_no_package_power() { assert_eq!(first.package_w, None); }
#[test] fn no_amd_drm_device_means_no_igpu() { assert_eq!(s.igpu_busy_pct, None); }
#[test] fn an_amd_igpu_reports_its_busy_percent() { /* card0 vendor 0x1002 gpu_busy_percent 7 */ assert_eq!(s.igpu_busy_pct, Some(7.0)); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test cpu`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check and `make smoke`**

Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Ask the user to install the rule**

Ask the user to run:

```
sudo install -m 0644 packaging/udev/70-ghost-shell-rapl.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=powercap --action=add
```

Then check that `stat -c %a /sys/class/powercap/intel-rapl:0/energy_uj` prints `444`, and that `ghostd probe cpu` shows a plausible `package_w`. Report the read cost per sample. The check after a reboot is a Task 19 row.

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

Snapshot V2's centre (SILICON tab) and V1's scope (narrow) with `--fake`, and compare them with the mockup's `drawDie` at both widths. Measure V2 with SILICON over 60 s (animating, ≤ 2 % with the daemon; call 4).

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
    - **NVMe** (`org.freedesktop.UDisks2.NVMe.Controller`): `power_on_hours` comes from `SmartPowerOnHours`, and `wear_pct` from `SmartGetAttributes({})["percent_used"]`, both only when `SmartUpdated > 0`;
    - **ATA** (`Drive.Ata`): `power_on_hours = SmartPowerOnSeconds / 3600` when `SmartUpdated > 0`, and `wear_pct` is `None`. No method is called;
    - **never `SmartUpdate`**;
    - a `NotAuthorized` or any other error leaves that field `None`, logged once per drive.
  - **`labels.mjs`:** `serialOf(driveId)` returns the text after the last `#`.
  - **Storage widget:** `requires = ["disks", "disks.health"]`. A drive's second line ends with a `dim` `W 3% · 12345 H` when either value is known. It is never coloured: the spec sets no wear threshold. The mount text elides first.

- [ ] **Step 1: Write the failing tests**

These run on the private bus with a fake UDisks2 object server that counts method calls per interface.

```rust
#[tokio::test(start_paused = true)] async fn wear_and_hours_come_from_the_cache() { assert_eq!(drive(&snap, "2O412LA45EG7"), &DriveHealth { serial: "2O412LA45EG7".into(), wear_pct: Some(3), power_on_hours: Some(12_345) }); }
#[tokio::test(start_paused = true)] async fn smart_update_is_never_called() { assert_eq!(udisks.calls("SmartUpdate"), 0); }
#[tokio::test(start_paused = true)] async fn ata_drives_are_read_from_properties_only() { assert_eq!(udisks.method_calls_on("org.freedesktop.UDisks2.Drive.Ata"), 0); assert_eq!(drive(&snap, "WD-BLUE").power_on_hours, Some(20_000)); }
#[tokio::test(start_paused = true)] async fn a_drive_never_read_reports_none() { /* SmartUpdated 0: the dead WD */ assert_eq!(drive(&snap, "213823802844").wear_pct, None); }
#[tokio::test(start_paused = true)] async fn not_authorized_leaves_wear_unknown_and_keeps_hours() { }
#[tokio::test(start_paused = true)] async fn a_resume_after_the_cadence_refreshes_once() { /* wall clock +8 h while the monotonic clock moves 60 s */ assert_eq!(udisks.calls("GetManagedObjects"), 2); }
#[tokio::test(start_paused = true)] async fn udisks_appearing_later_is_read_in_full() { }
#[tokio::test(start_paused = true)] async fn nothing_runs_while_unwanted() { assert_eq!(udisks.total_calls(), 0); }
```

```js
test('serial of a drive id', () => assert.equal(serialOf('XPG GAMMIX S70 BLADE#2O412LA45EG7'), '2O412LA45EG7'));
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test health && node --test 'shell/tests/*.test.mjs'`
Expected: FAIL.

- [ ] **Step 3: Implement, then run every check and `make smoke`**

Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Live check and measure** (system bus, read-only)

Run a dev daemon plus `topic-tap.py` on `disks.health`. Expected:
- the XPG and the Kingston report wear and hours;
- `sda` and `sdb` report hours or `None`, and their power state is unchanged before and after (`cat /sys/block/sd?/device/state`);
- the dead WD reports `None`.

Snapshot Storage at V1 width: every line fits. Report the time per refresh.

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
    - `/modules/kdeconnect` `devices(onlyReachable: true, onlyPaired: true)`;
    - `org.kde.kdeconnect.device` `name` (the label, uppercased) and `type` (`phone` and `tablet` → Phone);
    - `org.kde.kdeconnect.device.battery` properties `charge` and `isCharging`;
    - `id` is `kdeconnect:<device id>`;
    - it never requests a refresh from the device.
  - Both services are re-read in full when their name gains an owner. While unwanted, nothing runs.
  - **Sensors widget:** `requires = ["sensors", "batteries"]`. Batteries follow the fans after a separator, in the same two-column grid: `DUALSENSE 80%`, `PHONE 15%`, coloured by `Segments.levelLow` with `Theme.batteryWarn`/`batteryCrit` (≤ 20 warn, ≤ 10 crit), and a `+` after the value while charging. This closes P19: rows that don't fit in 184 px elide the last group, and the meta count still counts every probe.

- [ ] **Step 1: Read-only introspection**

Run `busctl --user introspect org.kde.kdeconnect /modules/kdeconnect/devices/<id>/battery` and the same for `org.kde.kdeconnect.device`. These are read-only calls on the session bus, allowed for this step only. Confirm the property names. If they differ, stop with NEEDS_CONTEXT.

- [ ] **Step 2: Write the failing tests**

These use fake UPower and KDE Connect servers on two private buses.

```rust
#[tokio::test(start_paused = true)] async fn a_dualsense_reads_as_a_controller() { assert_eq!(b.kind, BatteryKind::Controller); assert_eq!(b.label, "DUALSENSE WIRELESS CONTROLLER"); assert_eq!(b.pct, Some(80.0)); }
#[tokio::test(start_paused = true)] async fn the_display_device_and_power_supplies_are_skipped() { }
#[tokio::test(start_paused = true)] async fn a_paired_reachable_phone_reports_its_charge() { assert_eq!(phone.pct, Some(15.0)); assert_eq!(phone.id, "kdeconnect:abc123"); }
#[tokio::test(start_paused = true)] async fn no_refresh_is_ever_requested_from_the_phone() { assert_eq!(kdeconnect.method_calls_except(&["devices"]), 0); }
#[tokio::test(start_paused = true)] async fn upower_appearing_later_is_read_in_full() { }
#[tokio::test(start_paused = true)] async fn a_config_label_replaces_the_model() { assert_eq!(b.label, "PAD"); }
#[tokio::test(start_paused = true)] async fn nothing_runs_while_unwanted() { }
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p ghostd --test batteries`
Expected: FAIL.

- [ ] **Step 4: Implement, then run every check, `make smoke` and a Sensors snapshot**

Take the Sensors snapshot at V1 and MONITOR sizes with the fake's two batteries. Everything fits or elides cleanly. Expected: PASS and `smoke: ok`.

- [ ] **Step 5: Live check** (read-only)

Run a dev daemon plus `topic-tap.py` on `batteries`. The DualSense shows when it is connected, and the phone shows when KDE Connect reaches it. Report the cost per KDE Connect poll.

- [ ] **Step 6: Commit**

```bash
git add crates widgets/sensors
git commit -m "feat(ghostd): add device batteries from UPower and KDE Connect"
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

Run: `cargo test --workspace && node --test 'shell/tests/*.test.mjs' && scripts/qml-smoke.sh && GHOST_SMOKE_BUS=private GHOST_SMOKE_ACTIONS="monitor swap" scripts/qml-smoke.sh`
Expected: PASS, then `smoke: ok` twice with zero slot errors.

- [ ] **Step 5: Snapshot each view**

Snapshot every default view at native `u` with `--fake`, and compare them with `L-v1`, `L-v2` and `L-mon`. Nothing overflows. Record the deviations: the two-panel scope (call 7) and the omitted silicon facts (call 5).

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

Follow R17's order, with `make native` added to the build step. When regenerating the user's config, ask the user whether to add a `[uplink.labels]` table, with organisations of their choosing.

- [ ] **Step 2: Measure each state for 60 s** (shell + daemon combined; RSS includes the mapped GeoIP pages)

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
- The DualSense and the phone show their batteries. The storage rows show wear and hours.

- [ ] **Step 4: Write the note and update README**

Record:
- every measurement;
- the deviations: calls 4, 5 and 7, and the omitted mockup facts;
- the user's decisions, 1 to 5.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/notes/2026-10-21-plan2b-acceptance.md README.md
git commit -m "docs: record plan 2b acceptance"
```

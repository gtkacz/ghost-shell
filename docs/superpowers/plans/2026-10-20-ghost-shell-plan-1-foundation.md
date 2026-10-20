# ghost-shell Plan 1 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A running `ghostd` daemon streams real telemetry and the live-reloaded config over a Unix socket. A Quickshell desktop surface draws the dot grid, the scanlines and the five V1 machine panels (Processor, Graphics, Memory, Storage, Sensors) in the final style, from that live data, within the spec's idle budget.

**Architecture:**
- A Rust workspace with two crates:
  - `ghost-proto`: wire types and config types, sent as JSON lines.
  - `ghostd`: collectors that read from a configurable `/proc` + `/sys` root, history ring buffers, config load/validate/watch, a tokio Unix-socket server, and a fake mode.
- A Quickshell config in `shell/`:
  - singletons for the daemon connection, the theme and the frame clock;
  - pure-logic ES modules, tested with Node;
  - QML components and panels;
  - one wlr-layer-shell desktop surface, sized in design pixels × `u = logicalWidth / 1280`.

**Tech Stack:**
- Rust 1.98: tokio (current-thread), serde, serde_json, toml, notify, nvml-wrapper, nix, zbus, thiserror, tracing, tracing-subscriber; dev dependency tempfile.
- Quickshell 0.2.1 on Qt 6.11: QtQuick, QtQuick.Shapes, ShaderEffect with shaders compiled by `qsb-qt6`.
- Node 24 (`node --test`).
- GNU make.

**Spec:** `docs/superpowers/specs/2026-10-20-ghost-shell-design.md`. Read it alongside this plan. The reference mockup `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html` is the visual truth.

**Roadmap.** This is plan 1 of 4. Each later plan is written after the previous one ships, because it depends on this plan's spike results and measurements.
1. **This plan:** foundation.
2. The rest of the dashboard:
   - panels: network, uplink globe (Natural Earth geography), silicon, collapsible process tree, media and PipeWire spectrum, weather, INMET alerts and countdown, clock;
   - views: V2 and MONITOR, print transitions, desktop-only shortcuts;
   - text scramble and the Japanese chips;
   - the greeting over windows.
3. Taskbar and switching:
   - bar layouts A/B/C, top or bottom position, mono/colour icons;
   - tasks, badges, jump lists, Meta+1…0;
   - tray host with KDE's standard popups;
   - launcher;
   - the KWin TabBox switcher;
   - the notification server, centre, popups and Do Not Disturb.
4. Taking over from plasmashell:
   - keeping every Plasma function (§4);
   - migrating off gridlock, conky, `dash` and `storm_alert` (with consent);
   - the Chakra Petch Slashed generator;
   - packaging;
   - a performance and spacing audit at native resolution.

## Global Constraints

- **Design units:** every size here is mockup px at 1280×720. On screen, multiply by `u = surfaceLogicalWidth / 1280` (≈ 1.74 on the 2560×1440 M27Q at scale 1.15). Never hard-code the scale.
- **Palette defaults:**
  - `ground #0a0f11`, `panel #0a0f11` at **55 %** fill, `hair #2a3a3d`, `hair2 #1a2527`;
  - `text #b7d1cd`, `hi #e4f1ef`, `dim #587170`;
  - `signal #a3d4cc`, `warn #e9a84c`, `crit #ff3b45`;
  - `dot #a3d4cc` at **16 %** on a **24 px** grid;
  - scanlines: a **1 px** dark line every **3 px** at **15 %** black.
- **Colour only for attention:**
  - loads (CPU, GPU, memory) ≥ **75 %** warn and ≥ **90 %** crit;
  - temperatures ≥ **80 °C** warn and ≥ **90 °C** crit;
  - a dead drive is crit.
- **Panels:**
  - a 45° chamfer of **12 px** on the top-right and bottom-left corners;
  - **1.5 px** signal-colour brackets with **14 px** arms on the top-left and bottom-right corners;
  - a 1 px `hair` edge;
  - a header with a **5 px** signal square, an UPPERCASE label (10 px, weight 600, tracking .16 em), the Japanese tag, and right-aligned `dim` meta.
- **Segmented bars:**
  - meter cells are **4 px** wide with **2 px** gaps and **7 px** tall;
  - graph cells are columns of small cells;
  - unlit cells are signal at **6 %**.
- **Font:** config `appearance.font`, default **"Kode Mono"**; CJK fallback default **"Noto Sans CJK JP"**. Numbers never shift.
- **Japanese tags:** mode `kanji | katakana | mixed | off`, default `mixed`, where mixed means kanji on panel tags.

  | Key | Kanji | Katakana |
  |---|---|---|
  | cpu | 演算 | コア |
  | gpu | 描画 | グラフィック |
  | mem | 記憶 | メモリ |
  | disk | 記録 | ストレージ |
  | sens | 温度 | センサー |
  | crit chip | 危険 | クリティカル |

- **Never:** glow or bloom, glitch effects, smooth fades on alert text, Unicode glyphs used as icons.
- **Boxed alert:**
  - a 3 px left edge;
  - a 16 px diagonal stripe tile scrolling one tile per 1.6 s;
  - text does a held-frame arrival flicker over 1.6 s, then blinks like the boot cursor: a 1 s loop of four held frames with opacity `1, .5, 0, .5`.
- **Motion:** decorative motion runs at ≤ **15 fps** in held steps, never at the display's refresh rate (170 Hz). Data panels repaint only on new data (1 Hz).
- **Budgets:** shell + daemon ≤ **2 %** of one core (averaged over 60 s) while anything animates, ≤ **1 %** with nothing animating, and ≤ **300 MB** RSS combined.
- **Daemon behaviour:** no forks, shell scripts or busy polling in steady state. A missing or broken source degrades one row, never a panel or the daemon.
- **Drives** are identified by model + serial, never by their `nvmeX` name.
- **Formats:** dates `DD.MM.YY`, 24-hour time, °C.
- **Config:**
  - lives at `$XDG_CONFIG_HOME/ghost-shell/config.toml`, in TOML;
  - reloads live when saved;
  - if the new file is invalid, the last good config is kept and a warning notification names the problem.

## Review Focus

1. **A sensor read fails mid-run.** The dead WD NVMe returns EIO, or the DualSense is unplugged. That reading shows as unavailable (`--` in `dim`) and every other reading still updates. *Tests: Task 7 and Task 16.*
2. **An editor replaces the config file by atomic rename, or the file is half-written when read.** The watcher keeps working after renames, and a parse error keeps the last good config. *Test: Task 4.*
3. **The shell starts before the daemon, or the daemon restarts.** The shell reconnects with backoff and shows values in `dim`, never fake zeros or errors. *Tests: Task 11 and Task 13.*
4. **Two drives share a model, or the kernel renumbers NVMe drives at boot.** Identity comes from the serial, and the order stays the same across boots. *Test: Task 9.*
5. **The display scale or connector changes** (scale 1.15 → 1.0, DP-4 → DP-1). The surface stays on the M27Q, matched by model, `u` recomputes, and nothing overflows. *Tests: Task 12 and Task 16.*

---

### Task 1: Rendering spike (layer, frame pacing, cost)

This task decides how every later animation is driven, and it must finish before Task 13. It's a spike: the deliverable is a measurement script plus a decision note, not product code.

**Files:**
- Create: `scripts/measure-cpu.sh`, `spikes/pacing/shell.qml`, `spikes/pacing/dots.frag`, `docs/superpowers/notes/2026-10-20-rendering-spike.md`
- Modify: `.gitignore` (add `target/` and `*.qsb`)

**Interfaces:**
- Produces:
  - `scripts/measure-cpu.sh <pid> <seconds>` prints exactly one line, `cpu_pct=<float with 2 decimals> rss_kib=<int>`. The CPU figure is the average % of one core, from the `utime+stime` delta in `/proc/<pid>/stat` with `getconf CLK_TCK`; RSS is `VmRSS` at the end. With an unknown pid it exits 2 and prints `no such process <pid>` to stderr.
  - The note's **Decision** section, which Task 13 follows. It fixes:
    - the layer: `Bottom` or `Background`;
    - the FrameClock approach and its rate;
    - whether occluded surfaces stop rendering on their own.

- [ ] **Step 1: Check the script against a known-idle process**

Run: `sleep 30 & scripts/measure-cpu.sh $! 3; scripts/measure-cpu.sh 999999 1; echo "exit=$?"`
Expected: the first line matches `cpu_pct=0.0[0-9] rss_kib=[0-9]+`, then `no such process 999999` and `exit=2`.

- [ ] **Step 2: Write the spike shell**

`spikes/pacing/shell.qml` puts a full-screen `PanelWindow` on the screen whose `model` contains `M27Q`, with `WlrLayershell.layer: WlrLayer.Bottom` and `exclusionMode: ExclusionMode.Ignore`. It draws a dot-grid ShaderEffect (`dots.frag`: 1 px dots, 24-unit spacing, 16 % alpha). The env var `SPIKE_MODE` picks what else runs:
- `static`: nothing else;
- `data`: 600 `Rectangle`s in a 30×20 grid recoloured by a 1000 ms `Timer`;
- `anim`: `data`, plus a 12 px square moved 1 px by a 66 ms `Timer`.

Compile the shader: `qsb-qt6 --qt6 -o spikes/pacing/dots.frag.qsb spikes/pacing/dots.frag`

- [ ] **Step 3: Measure each mode**

The gridlock HUD currently runs on the desktop layer. **Ask the user before stopping it** with `pkill -f "qs -c gridlock"`, and restart it afterwards with `qs -c gridlock -d`.

For each mode, run:
```bash
SPIKE_MODE=<mode> QSG_RENDER_TIMING=1 qs -p spikes/pacing 2> /tmp/spike-<mode>.log &
sleep 5; scripts/measure-cpu.sh "$(pgrep -f 'qs -p spikes/pacing')" 60; pkill -f 'qs -p spikes/pacing'
grep -c "Frame rendered" /tmp/spike-<mode>.log
```
Repeat `anim` with a maximised window covering the whole screen.

Record CPU %, RSS and frames per second for each case. Also record whether the surface sits above Plasma's desktop, sits below windows, and stays visible under Show Desktop (Meta+D).

- [ ] **Step 4: Write the decision note**

Write the results table and a **Decision** section.
- **Acceptance:** `static` renders about 0 frames/s, and `anim` renders ≤ 15 frames/s at ≤ 1.5 % CPU.
- **If `anim` is over budget,** record the mitigation Task 13 must use: either a lower FrameClock rate, or animated regions on their own small surfaces.
- **If covered surfaces keep rendering,** record that Plan 2 needs explicit occlusion detection.

- [ ] **Step 5: Commit**

```bash
git add scripts/measure-cpu.sh spikes/pacing docs/superpowers/notes/2026-10-20-rendering-spike.md .gitignore
git commit -m "chore: add rendering spike and cpu measurement script"
```

---

### Task 2: Rust workspace and the wire protocol (`ghost-proto`)

**Files:**
- Create: `Cargo.toml` (workspace with members `crates/*`, `resolver = "3"`, edition 2024), `crates/ghost-proto/Cargo.toml`, `crates/ghost-proto/src/{lib.rs,msg.rs,snapshot.rs}`
- Test: `crates/ghost-proto/tests/wire.rs`

**Interfaces:**
- Produces (all `Serialize + Deserialize + Debug + Clone + PartialEq`):
  - `pub const PROTO_VERSION: u32 = 1;`
  - `pub enum Topic { Cpu, Mem, Sensors, Gpu, Disks, Perf }`, serialised `snake_case`.
  - `pub enum ClientMsg { Hello { proto: u32, topics: Vec<Topic> } }`, tagged `#[serde(tag = "t", rename_all = "snake_case")]`.
  - `pub enum ServerMsg` with the same tagging. Variants:
    - `Hello { proto: u32, host: String }`
    - `Error { message: String }`
    - `History { step_ms: u32, series: BTreeMap<String, Vec<f32>> }`
    - `Cpu(CpuSnapshot)`, `Mem(MemSnapshot)`, `Sensors(SensorSnapshot)`, `Gpu(GpuSnapshot)`, `Disks(DiskSnapshot)`, `Perf(PerfSnapshot)`. Newtype variants flatten their fields next to `"t"`.
  - Snapshot types:
    - `CpuSnapshot { seq: u64, model: String, threads: Vec<f32>, ccds: Vec<CcdLoad>, load: f32, freq_mhz: u32 }`
    - `CcdLoad { id: u8, cpus: Vec<u16>, l3_kib: u32, vcache: bool, load: f32 }`
    - `MemSnapshot { seq: u64, total_kib: u64, used_kib: u64, swap_total_kib: u64, swap_used_kib: u64, top: Vec<ProcUsage> }`
    - `ProcUsage { pid: u32, name: String, cpu: f32, mem: f32 }`, where both values are % of the whole machine.
    - `SensorSnapshot { seq: u64, temps: Vec<Reading>, fans: Vec<Reading> }`
    - `Reading { id: String, label: String, value: Option<f32> }`, where `None` means unavailable.
    - `GpuSnapshot { seq: u64, present: bool, name: String, load: f32, vram_used_mib: u32, vram_total_mib: u32, temp_c: f32, power_w: f32, power_limit_w: f32, fan_pct: f32 }`
    - `DiskSnapshot { seq: u64, drives: Vec<Drive> }`
    - `Drive { id: String, label: String, model: String, kernel_name: String, state: DriveState, mounts: Vec<String>, used_pct: Option<f32>, temp_c: Option<f32>, read_bps: u64, write_bps: u64 }`
    - `DriveState { Live, Dead, Unmounted }`, serialised `snake_case`.
    - `PerfSnapshot { seq: u64, daemon_cpu: f32, daemon_rss_kib: u64, shell_cpu: Option<f32>, shell_rss_kib: Option<u64> }`
  - `pub fn encode_line(msg: &ServerMsg) -> String` returns compact JSON plus `\n`.
  - `pub fn decode_client_line(line: &str) -> Result<ClientMsg, WireError>`, where `WireError` (thiserror) wraps the serde_json error.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn hello_has_exact_wire_form() {
    let s = encode_line(&ServerMsg::Hello { proto: 1, host: "tkacz-workstation".into() });
    assert_eq!(s, "{\"t\":\"hello\",\"proto\":1,\"host\":\"tkacz-workstation\"}\n");
}
#[test] fn snapshot_fields_flatten_next_to_tag() {
    let v: serde_json::Value = serde_json::from_str(&encode_line(&ServerMsg::Gpu(GpuSnapshot { seq: 3, present: true, ..Default::default() }))).unwrap();
    assert_eq!(v["t"], "gpu"); assert_eq!(v["seq"], 3); assert_eq!(v["present"], true);
}
#[test] fn unavailable_reading_is_null() {
    let r = Reading { id: "nvme/213823802844".into(), label: "WD GREEN".into(), value: None };
    assert_eq!(serde_json::to_value(&r).unwrap()["value"], serde_json::Value::Null);
}
#[test] fn decodes_client_hello() {
    let m = decode_client_line(r#"{"t":"hello","proto":1,"topics":["cpu","gpu"]}"#).unwrap();
    assert_eq!(m, ClientMsg::Hello { proto: 1, topics: vec![Topic::Cpu, Topic::Gpu] });
}
#[test] fn rejects_garbage() { assert!(decode_client_line("{nope").is_err()); }
#[test] fn every_server_msg_round_trips() { /* build one of each variant, encode, decode as ServerMsg, assert_eq */ }
#[test] fn drive_state_wire_names() { assert_eq!(serde_json::to_string(&DriveState::Dead).unwrap(), "\"dead\""); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghost-proto`
Expected: compile errors for the missing types.

- [ ] **Step 3: Implement the types and the two functions**

Derive `Default` on the snapshot structs so tests can use `..Default::default()`.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghost-proto`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/ghost-proto
git commit -m "feat(proto): add wire protocol types and json-lines codec"
```

---

### Task 3: Config schema, defaults and validation

**Files:**
- Create: `crates/ghost-proto/src/config.rs`, `crates/ghostd/Cargo.toml`, `crates/ghostd/src/main.rs` (a stub with a `default-config` subcommand), `crates/ghostd/src/lib.rs`, `crates/ghostd/src/config/{mod.rs,load.rs,default.toml}`
- Modify: `crates/ghost-proto/src/msg.rs`, adding `ServerMsg::Config { data: ResolvedConfig }` and `ServerMsg::ConfigError { message: String }`
- Test: `crates/ghostd/tests/config_load.rs`

**Interfaces:**
- Produces, in `ghost-proto`: `ResolvedConfig` and its sections. Every field has a default; serde uses `deny_unknown_fields`; `Default` gives exactly these values.
  - `appearance`:
    - `font = "Kode Mono"`, `font_cjk = "Noto Sans CJK JP"`;
    - `japanese = Mixed` (enum `Kanji | Katakana | Mixed | Off`);
    - `panel_fill = 55` (u8, 0..=100);
    - `colors { ground = "#0a0f11", panel = "#0a0f11", hair = "#2a3a3d", hair2 = "#1a2527", text = "#b7d1cd", hi = "#e4f1ef", dim = "#587170", signal = "#a3d4cc", warn = "#e9a84c", crit = "#ff3b45", dot = "#a3d4cc" }`;
    - `dots { spacing = 24, opacity = 16 }`;
    - `scanlines { opacity = 15 }`.
  - `thresholds { load_warn = 75, load_crit = 90, temp_warn = 80, temp_crit = 90, sensors: BTreeMap<String, SensorThreshold { warn: f32, crit: f32 }> }`
  - `display { model = "M27Q" }`
  - `sensors { labels: BTreeMap<String,String> }` and `disks { labels: BTreeMap<String,String> }`, the latter keyed by serial.
  - `dashboard { default_view = V1 (enum V1 | V2), history_minutes = 5 (1..=60) }`
  - `bar`:
    - `layout = A` (enum `A | B | C`), `position = Top` (enum `Top | Bottom`), `icons = Mono` (enum `Mono | Colour`);
    - `pinned = ["org.kde.dolphin", "steam", "tixati", "com.discordapp.Discord", "org.mozilla.firefox", "thorium-browser", "com.spotify.Client", "thorium-hnpfjngllnobngcgfapefoaidbinmjnm-Profile_1", "com.visualstudio.code", "com.slack.Slack", "org.kde.konsole"]`.
  - `greeting { name = "" }`. An empty name is replaced with the login name by `load_*`.
  - `location { city = "São Paulo", lat = -23.55, lon = -46.63, inmet_geocode = "3550308" }`
  - `shortcuts { monitor = "Meta+M", swap = "Meta+Shift+M" }`
  - `notifications { history_limit = 200, popup_timeout_s = 6 }`
- Produces, in `ghostd::config`:
  - `pub const DEFAULT_TOML: &str`: the commented default file, holding every key at its default (`include_str!("default.toml")`).
  - `pub fn load_str_with_user(s: &str, user: &str) -> Result<ResolvedConfig, ConfigError>`. An empty `greeting.name` becomes `user`.
  - `pub fn load_str(s: &str) -> Result<ResolvedConfig, ConfigError>`, which is `load_str_with_user` with `$USER`.
  - `pub fn load_path(p: &Path) -> Result<ResolvedConfig, ConfigError>`. A missing file gives the defaults.
  - `pub fn default_path() -> PathBuf` (`$XDG_CONFIG_HOME` or `~/.config`, then `ghost-shell/config.toml`).
  - `pub struct ConfigError { pub message: String }` with `Display = message`.
  - Messages use the exact formats in the tests.
- CLI: `ghostd default-config` prints `DEFAULT_TOML`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn default_file_equals_builtin_defaults() { let mut d = ResolvedConfig::default(); d.greeting.name = "gtkacz".into(); assert_eq!(load_str_with_user(DEFAULT_TOML, "gtkacz").unwrap(), d); }
#[test] fn empty_file_is_defaults() { assert_eq!(load_str("").unwrap().appearance.panel_fill, 55); }
#[test] fn partial_override_keeps_other_defaults() {
    let c = load_str("[appearance]\npanel_fill = 40\n").unwrap();
    assert_eq!(c.appearance.panel_fill, 40); assert_eq!(c.appearance.colors.warn, "#e9a84c"); assert_eq!(c.bar.pinned.len(), 11);
}
#[test] fn bad_colour_names_the_key() {
    let e = load_str("[appearance.colors]\nwarn = \"orange\"\n").unwrap_err();
    assert_eq!(e.message, "appearance.colors.warn: expected #rrggbb, got \"orange\"");
}
#[test] fn warn_must_be_below_crit() {
    let e = load_str("[thresholds]\nload_warn = 95\n").unwrap_err();
    assert_eq!(e.message, "thresholds: load_warn (95) must be below load_crit (90)");
}
#[test] fn fill_out_of_range() { assert_eq!(load_str("[appearance]\npanel_fill = 120\n").unwrap_err().message, "appearance.panel_fill: must be 0..=100, got 120"); }
#[test] fn unknown_key_is_rejected() { assert!(load_str("[appearance]\nglow = true\n").unwrap_err().message.contains("unknown field `glow`")); }
#[test] fn empty_greeting_name_becomes_login_name() { assert_eq!(load_str_with_user("", "gtkacz").unwrap().greeting.name, "gtkacz"); }
#[test] fn japanese_off_parses() { assert_eq!(load_str("[appearance]\njapanese = \"off\"\n").unwrap().appearance.japanese, Japanese::Off); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test config_load`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the types, the defaults and `load_*`**

Parse with `toml::from_str` into the `deny_unknown_fields` types. Then run one `validate(&ResolvedConfig) -> Result<(), ConfigError>` pass: hex colours `^#[0-9a-fA-F]{6}$`, ranges, and `warn < crit` for loads, temps and every per-sensor pair. Map toml errors to `"{dotted.key}: {message}"` where toml gives the key.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test config_load && cargo run -q -p ghostd -- default-config | head -3`
Expected: PASS, then the first lines of the commented default file.

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src crates/ghostd
git commit -m "feat(config): add config schema with defaults and validation"
```

---

### Task 4: Config watcher with keep-last-good reload

**Files:**
- Create: `crates/ghostd/src/config/watch.rs`
- Test: `crates/ghostd/tests/config_watch.rs`

**Interfaces:**
- Consumes: `load_path` (Task 3).
- Produces: `pub fn spawn_watcher(path: PathBuf, initial: ResolvedConfig) -> notify::Result<(WatchHandle, tokio::sync::watch::Receiver<ResolvedConfig>, tokio::sync::mpsc::Receiver<ConfigError>)>`.
  - It watches the **parent directory**, so it survives atomic renames and delete-then-recreate.
  - It debounces events by 150 ms.
  - When the reparse succeeds and the result differs, it publishes the new config.
  - When the reparse fails, it sends the error and leaves the watch value alone.
  - When the file is removed, it sends `ConfigError { message: "config file removed; keeping last configuration" }`.
  - Dropping `WatchHandle` stops the watcher.

- [ ] **Step 1: Write the failing tests** (tokio test; each waits at most 2 s)

```rust
#[tokio::test] async fn plain_save_publishes() { /* write panel_fill = 40 → rx.changed() → borrow().appearance.panel_fill == 40 */ }
#[tokio::test] async fn atomic_rename_publishes() { /* write config.toml.tmp, rename over config.toml → value 30 */ }
#[tokio::test] async fn half_written_file_keeps_last_good() {
    /* write "[appearance\npanel_fi" → err_rx yields a message; watch value still 55; then write valid 35 → 35 */
}
#[tokio::test] async fn removed_file_keeps_last_good() {
    /* remove → err message == "config file removed; keeping last configuration"; value unchanged */
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test config_watch`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `spawn_watcher`**

Use `notify::RecommendedWatcher` on `path.parent()`, filter events by the file name, and debounce with a tokio timer.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test config_watch`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/config/watch.rs crates/ghostd/tests/config_watch.rs
git commit -m "feat(config): watch config directory and keep last good config on errors"
```

---

### Task 5: SysRoot, the `Source` trait and the CPU collector

**Files:**
- Create: `crates/ghostd/src/sysroot.rs`, `crates/ghostd/src/collect/{mod.rs,cpu.rs}`, `crates/ghostd/tests/common/mod.rs` (`FakeRoot`)
- Test: `crates/ghostd/tests/cpu.rs`

**Interfaces:**
- Produces:
  - `pub struct SysRoot(PathBuf)`.
    - `SysRoot::new(p)` and `SysRoot::host()` (which is `/`).
    - `fn read(&self, rel: &str) -> io::Result<String>`, where `rel` looks like `"proc/stat"`.
    - `fn list(&self, rel: &str) -> io::Result<Vec<String>>`, the sorted entry names.
    - `fn path(&self, rel: &str) -> PathBuf`.
  - `pub trait Source<T> { fn sample(&mut self) -> T; }`. Every collector in Tasks 5–9 implements it.
  - `pub fn parse_cpu_list(s: &str) -> Vec<u16>`, so `"0-7,16-23"` gives `[0..=7, 16..=23]`.
  - `pub struct CpuCollector`.
    - `CpuCollector::new(root: &SysRoot) -> io::Result<Self>` reads the topology once. It groups logical CPUs by `sys/devices/system/cpu/cpuN/cache/index3/shared_cpu_list`, orders the CCDs by their lowest CPU, sets `vcache = l3_kib > min(l3_kib)`, and takes `model` from the `model name` line in `proc/cpuinfo`.
    - `impl Source<CpuSnapshot>`. The first sample has zero loads. Per-thread load is `100·Δbusy/Δtotal`, where `busy = total − idle − iowait`. CCD and total loads are means of their threads. `freq_mhz` is the mean `cpufreq/scaling_cur_freq` (kHz) / 1000, or 0 when that's missing.
  - `tests/common::FakeRoot` creates a tempdir with `fn write(&self, rel, contents)`, `fn remove(&self, rel)` and `fn root(&self) -> SysRoot`.
  - `tests/common::fake_9950x3d() -> FakeRoot` writes 32 `cpuN` topology dirs, a two-line `proc/stat` and a `proc/cpuinfo` with `model name : AMD Ryzen 9 9950X3D 16-Core Processor`. Later tasks reuse it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn parses_ranges() { assert_eq!(parse_cpu_list("0-2,16-17"), vec![0, 1, 2, 16, 17]); }
#[test] fn groups_ccds_and_detects_vcache() {
    let r = fake_9950x3d();   // cpu0..31, index3 lists "0-7,16-23"/98304K and "8-15,24-31"/32768K
    let mut c = CpuCollector::new(&r.root()).unwrap(); let s = c.sample();
    assert_eq!(s.ccds[0].cpus, [0,1,2,3,4,5,6,7,16,17,18,19,20,21,22,23]);
    assert!(s.ccds[0].vcache); assert!(!s.ccds[1].vcache); assert_eq!(s.ccds[0].l3_kib, 98304);
    assert_eq!(s.model, "AMD Ryzen 9 9950X3D 16-Core Processor");
}
#[test] fn computes_thread_and_ccd_load() {
    /* sample 1; rewrite proc/stat so cpu0 gained 50 busy + 50 idle jiffies, cpu1 0 busy + 100 idle, all others 100 idle */
    /* sample 2 → threads[0] == 50.0, threads[1] == 0.0, ccds[0].load == 50.0/16.0 */
}
#[test] fn missing_cpufreq_is_zero_not_panic() { /* no cpufreq dirs → freq_mhz == 0 */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test cpu`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `SysRoot`, `Source` and `CpuCollector`**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test cpu`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/sysroot.rs crates/ghostd/src/collect crates/ghostd/tests
git commit -m "feat(ghostd): add cpu collector with ccd topology from l3 sharing"
```

---

### Task 6: Memory and top processes

**Files:**
- Create: `crates/ghostd/src/collect/mem.rs`
- Test: `crates/ghostd/tests/mem.rs`

**Interfaces:**
- Consumes: `SysRoot` and `Source` (Task 5).
- Produces: `MemCollector::new(root: &SysRoot, top_n: usize) -> Self`, implementing `Source<MemSnapshot>`.
  - `used_kib = MemTotal − MemAvailable` and `swap_used_kib = SwapTotal − SwapFree`, both from `proc/meminfo`.
  - Each process's `cpu = 100·Δ(utime+stime) / Δ(sum of all jiffies in the "cpu " line of proc/stat)`.
  - Each process's `mem = 100·rss_kib / total_kib`, using `VmRSS` from `proc/<pid>/status`, and `name` comes from `proc/<pid>/comm`.
  - Processes with an RSS of 0 (kernel threads) are excluded. A process that vanishes between reads is skipped.
  - `top` is sorted by `cpu` descending, then by pid, and has `top_n` entries.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn used_is_total_minus_available() { /* MemTotal 96388184, MemAvailable 77000000 → used_kib == 19388184 */ }
#[test] fn process_cpu_is_share_of_whole_machine() { /* pid 42: Δ(utime+stime)=64 over Δtotal=3200 → cpu == 2.0 */ }
#[test] fn vanished_process_is_skipped() { /* remove proc/42 between samples → no panic, 42 absent */ }
#[test] fn kernel_threads_excluded() { /* pid 7 without VmRSS → not in top */ }
#[test] fn top_is_sorted_and_truncated() { /* 6 procs, top_n 5 → 5 entries, cpu descending */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test mem`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `MemCollector`**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test mem`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/mem.rs crates/ghostd/tests/mem.rs
git commit -m "feat(ghostd): add memory and top process collector"
```

---

### Task 7: Temperatures and fans (hwmon)

**Files:**
- Create: `crates/ghostd/src/collect/hwmon.rs`, `crates/ghostd/src/collect/labels.rs`
- Test: `crates/ghostd/tests/hwmon.rs`

**Interfaces:**
- Consumes: `SysRoot` and `Source`.
- Produces: `SensorCollector::new(root: &SysRoot, overrides: &BTreeMap<String,String>) -> Self`, implementing `Source<SensorSnapshot>`. It rescans `sys/class/hwmon` every 30 samples to pick up hotplugged devices.
  - **Ids** are `"{chip}/{label}"`, where `chip` is the hwmon `name` and `label` is the content of `tempN_label` / `fanN_label`, or `tempN` / `fanN` when there is none.
    - NVMe chips use `"nvme/{serial}"`, with the serial from `hwmonX/device/serial`, trimmed.
    - `spd5118` chips use `"spd5118/{n}"`, with n = 1, 2… in hwmon order.
  - **Built-in labels**, matched by chip prefix and sensor label:

    | Sensor | Label |
    |---|---|
    | `k10temp` `Tctl` | `CPU TCTL` |
    | `k10temp` `Tccd1` | `CCD0` |
    | `k10temp` `Tccd2` | `CCD1` |
    | `nct6799` `fan2` | `CPU FAN` |
    | `nct6799` `fan3` | `CHASSIS FAN` |
    | `nct6799` `SYSTIN` | `MOBO` |
    | `r8169*` `temp1` | `NIC` |
    | `spd5118/n` | `DIMM n` |
    | `nvme/*` | the drive model, uppercased and trimmed |

    Any other sensor is labelled `"{CHIP} {LABEL}"`, uppercased. `overrides[id]` replaces any label.
  - **Values:** temperatures are millidegrees / 1000. Fans are listed only if they have a built-in label or read > 0 at the first scan. **A failed read gives `value: None`, and the other readings are unaffected.**
  - `ps_controller_battery_*` and any other hwmon without `temp*`/`fan*` inputs contributes nothing.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn labels_and_values_for_this_machine() {
    /* k10temp temp1 "Tctl"=57875, temp3 "Tccd1"=57625, temp4 "Tccd2"=40875; nct6799 fan2=1670, fan3=450, fan1=0 */
    /* → temps contain (id "k10temp/Tctl", label "CPU TCTL", 57.875); fans == ["CPU FAN" 1670, "CHASSIS FAN" 450]; no fan1 */
}
#[test] fn nvme_uses_serial_and_model() { /* hwmon name nvme, device/serial "2O412LA45EG7 ", device/model "XPG GAMMIX S70 BLADE" → id "nvme/2O412LA45EG7", label "XPG GAMMIX S70 BLADE" */ }
#[test] fn failed_read_is_unavailable_not_fatal() {
    /* make temp1_input of the WD nvme a directory (read fails like the dead drive's EIO) → that Reading.value == None; k10temp still Some */
}
#[test] fn override_label_wins() { /* overrides {"k10temp/Tctl": "CPU"} → label "CPU" */ }
#[test] fn hotplug_is_picked_up_on_rescan() { /* add a new hwmon dir, sample 30 times → its reading appears */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test hwmon`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `SensorCollector` and the label table**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test hwmon`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/hwmon.rs crates/ghostd/src/collect/labels.rs crates/ghostd/tests/hwmon.rs
git commit -m "feat(ghostd): add hwmon temperature and fan collector"
```

---

### Task 8: GPU through NVML

**Files:**
- Create: `crates/ghostd/src/collect/gpu.rs` (unit tests inside it)

**Interfaces:**
- Produces:
  - `pub struct GpuReading { name: String, load: f32, vram_used_mib: u32, vram_total_mib: u32, temp_c: f32, power_w: f32, power_limit_w: f32, fan_pct: f32 }`
  - `pub trait GpuSource { fn read(&mut self) -> Option<GpuReading>; }`
  - `pub struct NvmlSource`.
    - `NvmlSource::open() -> Option<Self>` is `None` when NVML can't be loaded.
    - It reads device 0: `utilization_rates().gpu`, `memory_info()` (bytes / 1 MiB), `temperature(Gpu)`, `power_usage()` and `enforced_power_limit()` (mW / 1000), `fan_speed(0)`, and `name()`. Any NVML error gives `None`.
  - `GpuCollector::new(src: Option<Box<dyn GpuSource>>)`, implementing `Source<GpuSnapshot>`. It sets `present = false` and zeroes the fields when the source is missing or returns `None`.
- CLI: `ghostd probe gpu` prints one `GpuSnapshot` as JSON. Task 11 adds the other topics to `probe`.

- [ ] **Step 1: Write the failing tests** (in-module, with a `FakeGpu(Vec<Option<GpuReading>>)`)

```rust
#[test] fn maps_reading() { /* Some(load 31.0, vram 3174/16376, 41.0, 54.4, 320.0, 31.0) → snapshot fields equal, present true */ }
#[test] fn missing_nvml_is_absent() { let mut c = GpuCollector::new(None); assert!(!c.sample().present); }
#[test] fn error_mid_run_degrades() { /* Some then None → second sample present false, no panic */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --lib -- gpu`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `NvmlSource`, `GpuCollector` and `probe gpu`**

- [ ] **Step 4: Run them, then try the real GPU**

Run: `cargo test -p ghostd --lib -- gpu && cargo run -q -p ghostd -- probe gpu`
Expected: PASS, then JSON containing `"name":"NVIDIA GeForce RTX 4080 SUPER"` and `"present":true`.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/gpu.rs crates/ghostd/src/main.rs
git commit -m "feat(ghostd): add nvml gpu collector"
```

---

### Task 9: Drives (identity, state, usage, I/O)

**Files:**
- Create: `crates/ghostd/src/collect/disk.rs`
- Test: `crates/ghostd/tests/disk.rs`

**Interfaces:**
- Consumes: `SysRoot`, `Source`, and the `nvme/{serial}` readings from `SensorSnapshot` (Task 7).
- Produces:
  - `pub trait FsStat { fn used_pct(&self, mount: &str) -> Option<f32>; }`. `StatvfsFs` implements it with `nix::sys::statvfs`.
  - `DiskCollector::new(root: &SysRoot, fs: Box<dyn FsStat>, labels: &BTreeMap<String,String>)`, implementing `Source<DiskSnapshot>`.
  - **Which devices:** every `sys/block/*` except `loop*`, `zram*`, `ram*` and `dm-*`.
  - **Identity:**
    - `model` is `device/model`, trimmed.
    - `serial` is `device/serial` (NVMe), else `device/wwid`, else `"{kernel_name}"`.
    - `id = "{model}#{serial}"`.
    - `label` is `labels[serial]`, else `model`.
  - **State:**
    - `Dead` when the controller's `device/state` (NVMe) is `dead` or the size is 0.
    - Otherwise `Live` when a partition or the device appears in `proc/self/mountinfo` (matched by `major:minor` from `sys/block/X/**/dev`).
    - Otherwise `Unmounted`.
  - **Usage and rates:**
    - `mounts` are the mount points, with `/` first.
    - `used_pct` is `fs.used_pct(first mount)`, refreshed every 60 samples and cached in between.
    - `read_bps` and `write_bps` come from the `proc/diskstats` whole-device sector deltas × 512 per second.
  - **Order:**
    1. the drive mounted at `/`;
    2. other `Live` drives;
    3. `Dead` drives;
    4. `Unmounted` drives.

    Ties break by `id`.
  - `pub fn attach_temps(d: &mut DiskSnapshot, s: &SensorSnapshot)` sets `temp_c` from `nvme/{serial}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn dead_nvme_is_dead() { /* nvme1: device/state "dead", size 0 → state Dead, used_pct None */ }
#[test] fn identity_survives_renumbering() {
    /* build twice with XPG/WD names swapped between nvme0n1 and nvme1n1 → identical (id, state) lists in identical order */
}
#[test] fn same_model_different_serial_are_distinct() { /* two "WDC WD10EZEX" with serials A and B → 2 ids */ }
#[test] fn root_mounted_first_then_dead_then_unmounted() { /* XPG on "/", Kingston on /run/media/x, WD dead, sda unmounted → that order */ }
#[test] fn io_rates_from_diskstats() { /* Δ sectors read 2000 over 1 s → read_bps == 1_024_000 */ }
#[test] fn temps_attach_by_serial() { /* SensorSnapshot nvme/2O412LA45EG7 = 37.85 → XPG temp_c Some(37.85) */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test disk`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `DiskCollector`, `StatvfsFs` and `attach_temps`**

The sampling interval is injected (`Instant` delta) so tests control the elapsed time.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test disk`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/disk.rs crates/ghostd/tests/disk.rs
git commit -m "feat(ghostd): add drive collector keyed by model and serial"
```

---

### Task 10: History buffers and the sampler

**Files:**
- Create: `crates/ghostd/src/history.rs`, `crates/ghostd/src/sampler.rs` (unit tests inside both)

**Interfaces:**
- Consumes: the `Source<…>` collectors (Tasks 5–9) and `attach_temps`.
- Produces:
  - `History`:
    - `History::new(len: usize)`
    - `push(&mut self, key: &str, v: f32)`
    - `series(&self) -> BTreeMap<String, Vec<f32>>`, oldest first, each series at most `len` long.
    - `resize(&mut self, len)`, which keeps the newest samples.
  - `Sampler`:
    - `Sampler::new(cpu, mem, sensors, gpu, disks)`, each a `Box<dyn Source<…>>`.
    - `fn tick(&mut self, hist: &mut History) -> Vec<ServerMsg>` returns `[Cpu, Mem, Sensors, Gpu, Disks]` in that order, with a shared, increasing `seq`.
    - Each tick pushes the history keys `"load"`, `"ccd0"`, `"ccd1"` and `"gpu_load"`.
    - It calls `attach_temps` before emitting `Disks`.
  - History length is `history_minutes × 60` (one sample a second).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn tick_order_and_seq() { /* fakes → kinds == [cpu, mem, sensors, gpu, disks]; second tick seq == first + 1 */ }
#[test] fn history_grows_then_caps() { /* len 3, 5 ticks → series["load"].len() == 3, holding ticks 3..=5 */ }
#[test] fn resize_keeps_newest() { /* 5 samples, resize(2) → last two */ }
#[test] fn ccd_history_keys_present() { /* series has "ccd0" and "ccd1" */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --lib -- sampler history`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `History` and `Sampler`**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --lib -- sampler history`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/history.rs crates/ghostd/src/sampler.rs
git commit -m "feat(ghostd): add sampler and history ring buffers"
```

---

### Task 11: The socket server, `main`, fake mode, perf and config-error notifications

**Files:**
- Create: `crates/ghostd/src/{server.rs,fake.rs,selfmon.rs,notify.rs}`
- Modify: `crates/ghostd/src/main.rs`
- Test: `crates/ghostd/tests/server.rs`

**Interfaces:**
- Consumes: everything from Tasks 2–10.
- Produces:
  - **CLI:**
    - `ghostd run [--socket PATH] [--config PATH] [--fake] [--tick-ms N]`. Defaults: the socket is `$XDG_RUNTIME_DIR/ghostd.sock`, the config is `default_path()`, and the tick is 1000 ms.
    - `ghostd default-config`.
    - `ghostd probe <cpu|mem|sensors|gpu|disks>`.
  - `pub async fn serve(listener: UnixListener, hub: Hub)`.
    - **Handshake:** a client must send `ClientMsg::Hello` with `proto == 1` within 2 s. Otherwise the server sends `ServerMsg::Error { message: "expected hello with proto 1" }` and closes the connection.
    - **After hello,** the server sends `Hello`, `Config`, then `History { step_ms: 1000, series }`, then streams only the subscribed topics from each tick.
    - **Config changes** are broadcast to every client as `Config`. Watcher errors are broadcast as `ConfigError`.
    - **Slow clients:** a client whose unsent output exceeds 256 KiB is disconnected.
    - **Disconnects:** one client leaving never affects the others.
    - **Startup:** a stale socket file is removed before binding, and the socket is created with mode 0600.
  - `Hub { ticks: broadcast::Sender<Arc<Vec<ServerMsg>>>, config: watch::Receiver<ResolvedConfig>, errors: broadcast::Sender<String>, history: Arc<Mutex<History>> }`
  - `selfmon`:
    - It emits `Perf` every tick.
    - Its own figures come from `/proc/self`.
    - The shell's pid is the peer pid of the client that subscribed to `perf` (`UnixStream::peer_cred()`). No process scanning is needed.
    - The shell's CPU and RSS are measured like `measure-cpu.sh` does.
  - `notify::send_warning(summary: &str, body: &str)`.
    - It sends a zbus `org.freedesktop.Notifications.Notify` with app_name `"ghost-shell"` and urgency `normal`.
    - It's called with `("Config error", message)` on watcher errors.
    - A failure is only logged.
  - **Fake mode:**
    - deterministic sine-based sources for every topic, with 2 CCDs of 16 threads;
    - one `Dead` drive "WD GREEN 960G" and one sensor whose `value` is `None`;
    - GPU present with the name "NVIDIA GeForce RTX 4080 SUPER".

- [ ] **Step 1: Write the failing tests** (spawn `serve` with fake sources at `--tick-ms 50` on a temp socket)

```rust
#[tokio::test] async fn handshake_then_subscribed_topics_only() {
    /* send hello topics [cpu] → lines: hello, config, history, then ≥2 "cpu" lines within 500 ms and zero "gpu" lines */
}
#[tokio::test] async fn wrong_proto_gets_error_and_close() { /* hello proto 9 → {"t":"error","message":"expected hello with proto 1"} then EOF */ }
#[tokio::test] async fn silent_client_times_out() { /* send nothing → error line within 2.5 s, then EOF */ }
#[tokio::test] async fn clients_are_independent_and_reconnect_works() { /* A and B connected; drop A; B keeps getting ticks; new A' handshake succeeds */ }
#[tokio::test] async fn config_change_reaches_clients() { /* send new ResolvedConfig into watch → client receives "config" with panel_fill 40 */ }
#[tokio::test] async fn stale_socket_file_is_replaced() { /* create a plain file at the socket path → bind succeeds */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test server`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `serve`, `Hub`, the wiring in `main`, fake mode, `selfmon` and `notify`**

`main` uses a current-thread tokio runtime. The sampler runs on a `tokio::time::interval` with `MissedTickBehavior::Skip`. The config watcher from Task 4 feeds `Hub.config` and `Hub.errors`.

- [ ] **Step 4: Run them, then do a live check**

```bash
cargo test -p ghostd --test server
cargo run -q -p ghostd -- run --socket /tmp/gs.sock & sleep 2
uv run python -I -c "import socket,time; s=socket.socket(socket.AF_UNIX); s.connect('/tmp/gs.sock'); s.send(b'{\"t\":\"hello\",\"proto\":1,\"topics\":[\"cpu\",\"disks\"]}\n'); time.sleep(2.5); print(s.recv(1<<20).decode()[:3000])"; kill %1
```
Expected: PASS. The live check prints `hello`, `config` and `history` lines, then real `cpu` and `disks` lines. The disks include `"state":"dead"` for the WD Green.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd
git commit -m "feat(ghostd): serve telemetry and config over a unix socket"
```

---

### Task 12: Shell logic modules (Node-tested)

**Files:**
- Create: `shell/lib/{format.mjs,segments.mjs,theme.mjs,scale.mjs,labels.mjs}`
- Test: `shell/tests/{format,segments,theme,scale,labels}.test.mjs`

**Interfaces:**
- Produces (ES modules; QML imports them with `import "../lib/x.mjs" as X`):
  - **`format.mjs`:**
    - `pct(v)` → `"23%"`, rounded.
    - `tempC(v)` → `"64°C"`. `null` gives `"--"`.
    - `tempShort(v)` → `"64°"`.
    - `gib(kib, dp = 1)` → `"19.7"`.
    - `rate(bps)`: below 1e3 gives `"830 B/s"`, below 1e6 gives `"830.0 KB/s"`, otherwise `"12.4 MB/s"`.
    - `ghz(mhz)` → `"5.21 GHz"`.
    - `rpm(v)` → `"1690 RPM"`.
    - `ddmmyy(date)` → `"20.10.26"`.
  - **`segments.mjs`:**
    - `cellCount(width, cell = 4, gap = 2)` = `floor((width + gap) / (cell + gap))`.
    - `litCells(value, lo, hi, cells, min = 0)`.
    - `level(fraction, warnAt, critAt)` → `"normal" | "warn" | "crit"`, using ≥ comparisons.
    - `cellColor(level, row, lit, rows, t)`:
      - an unlit cell gives `t.signal` at 6 %;
      - a crit cell gives `t.crit`; a warn cell gives `t.warn`;
      - the top lit cell gives `t.hi`;
      - any other lit cell gives `t.signal` at `0.35 + 0.5·row/rows`.
  - **`theme.mjs`:**
    - `TAGS` holds the Global Constraints table (kanji and katakana sets).
    - `tag(key, mode)`: `mixed` → kanji, `off` → `""`.
    - `withAlpha(hex, pct)` → `"#AARRGGBB"`.
    - `isMonospace(w0, w1)` = `Math.abs(w0 − w1) < 0.5`.
  - **`scale.mjs`:**
    - `designUnit(logicalWidth)` = `logicalWidth / 1280`.
    - `pickScreen(screens, model)`: the first screen whose `.model` includes `model`, else `screens[0]`, else `null`.
  - **`labels.mjs`:**
    - `cpuModelShort(s)`: `"AMD Ryzen 9 9950X3D 16-Core Processor"` → `"RYZEN 9 9950X3D"`.
    - `gpuNameShort(s)`: `"NVIDIA GeForce RTX 4080 SUPER"` → `"RTX 4080 SUPER"`.
    - `ccdTitle(ccd)` → `"CCD0 · V-CACHE"` or `"CCD1 · FREQ"`.
    - `reading(snapshot, id)` → the value or `null`.

- [ ] **Step 1: Write the failing tests**

```js
import { test } from 'node:test'; import assert from 'node:assert/strict';
test('pct rounds', () => assert.equal(pct(22.6), '23%'));
test('rate tiers', () => { assert.equal(rate(830), '830 B/s'); assert.equal(rate(830000), '830.0 KB/s'); assert.equal(rate(12400000), '12.4 MB/s'); });
test('unavailable temp', () => assert.equal(tempC(null), '--'));
test('cells', () => { assert.equal(cellCount(266), 44); assert.equal(litCells(50, 0, 100, 10), 5); assert.equal(litCells(0.4, 0, 100, 10, 1), 1); });
test('levels at the spec thresholds', () => { assert.equal(level(0.749, .75, .9), 'normal'); assert.equal(level(.75, .75, .9), 'warn'); assert.equal(level(.9, .75, .9), 'crit'); });
test('tags', () => { assert.equal(tag('cpu', 'mixed'), '演算'); assert.equal(tag('cpu', 'katakana'), 'コア'); assert.equal(tag('cpu', 'off'), ''); });
test('alpha', () => assert.equal(withAlpha('#0a0f11', 55), '#8C0A0F11'));
test('design unit at 1.15 scale', () => assert.ok(Math.abs(designUnit(2560 / 1.15) - 1.739) < 0.001));
test('screen by model survives connector drift', () => {
  assert.equal(pickScreen([{ model: 'DELL U2720Q' }, { model: 'Gigabyte M27Q' }], 'M27Q').model, 'Gigabyte M27Q');
  assert.equal(pickScreen([], 'M27Q'), null);
});
test('short names', () => { assert.equal(cpuModelShort('AMD Ryzen 9 9950X3D 16-Core Processor'), 'RYZEN 9 9950X3D'); assert.equal(gpuNameShort('NVIDIA GeForce RTX 4080 SUPER'), 'RTX 4080 SUPER'); });
test('ccd titles', () => { assert.equal(ccdTitle({ id: 0, vcache: true }), 'CCD0 · V-CACHE'); assert.equal(ccdTitle({ id: 1, vcache: false }), 'CCD1 · FREQ'); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/`
Expected: FAIL (the modules are missing).

- [ ] **Step 3: Implement the five modules**

They are plain functions with no dependencies, because QML's JS engine loads them as-is.

- [ ] **Step 4: Run them and watch them pass**

Run: `node --test shell/tests/`
Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add shell/lib shell/tests
git commit -m "feat(shell): add tested formatting, segment, theme and scale logic"
```

---

### Task 13: Shell skeleton (services, desktop surface, dot grid, scanlines, frame clock)

**Files:**
- Create:
  - `shell/shell.qml`
  - `shell/services/{Daemon.qml,Theme.qml,FrameClock.qml,qmldir}`
  - `shell/components/{DotGrid.qml,Scanlines.qml}`
  - `shell/shaders/{dotgrid.frag,scanlines.frag}`
  - `shell/views/Desktop.qml`
  - `scripts/qml-smoke.sh`
  - `Makefile` (targets `shaders`, `test`, `smoke`)

**Interfaces:**
- Consumes: the Task 1 decision note (layer, frame rate), the Task 11 protocol, and the Task 12 modules.
- Produces:
  - **`Daemon` singleton:**
    - properties `connected`, `stale` (no message for 3 s), `config`, `cpu`, `mem`, `sensors`, `gpu`, `disks`, `perf`, `history` (`{ key: number[] }`), and the signal `configError(string message)`;
    - the socket path is `Quickshell.env("GHOST_SOCKET")`, else `$XDG_RUNTIME_DIR/ghostd.sock`;
    - when it connects, it writes `{"t":"hello","proto":1,"topics":["cpu","mem","sensors","gpu","disks","perf"]}\n` and logs `console.info("ghostd connected")`;
    - after each `cpu` and `gpu` message, it appends to `history.load`, `ccd0`, `ccd1` and `gpu_load`, trimmed to `config.dashboard.history_minutes × 60`;
    - it reconnects after 1 s, doubling to at most 10 s.
  - **`Theme` singleton:**
    - `u` (set by Desktop) and `function d(px) { return px * u }`;
    - colours `ground, panel, hair, hair2, text, hi, dim, signal, warn, crit, dot` as `color`;
    - `panelFill` (0..1), `font`, `fontCjk`, `japanese`, `loadWarn` and `loadCrit` (fractions), `tempWarn` and `tempCrit` (°C), and `tag(key)`;
    - its defaults equal the Global Constraints until a config message arrives.
  - **`FrameClock` singleton:**
    - `frame`: an int at the Task 1 rate (default 15 fps), driven by a `Timer`;
    - `blinkOpacity`: `[1, .5, 0, .5]` on a 250 ms `Timer`;
    - `acquire()` and `release()` keep a refcount, so both timers stop when nothing uses them.
  - **`Desktop`:**
    - a `PanelWindow` on `pickScreen(Quickshell.screens, Theme config display.model)`, on the layer from Task 1, with namespace `"ghost-shell-desktop"`, `ExclusionMode.Ignore`, all four anchors and colour `Theme.ground`;
    - it sets `Theme.u = width / 1280` (`GHOST_FORCE_U` overrides this for tests);
    - it stacks `DotGrid`, a `Loader` for the view (Task 16 sets V1), `Scanlines` on top, and a perf overlay when `GHOST_DEBUG=1` (daemon and shell CPU % and RSS);
    - other screens get a window with only `DotGrid`.
  - **Shaders:**
    - `dotgrid.frag` uniforms: `vec4 dotColor; float spacing; float radius;`, sizes in design px × `u`.
    - `scanlines.frag` uniforms: `float strength; float period; float lineWidth;`, with defaults 0.15, 3 and 1 in design px × `u`.
  - **Stale rule:** while `!Daemon.connected || Daemon.stale`, panels draw values in `dim` and meters with nothing lit.

- [ ] **Step 1: Write the smoke script and watch it fail**

`scripts/qml-smoke.sh`:
1. Builds with `cargo build -q -p ghostd`, then starts `target/debug/ghostd run --fake --socket /tmp/ghost-smoke.sock` and `GHOST_SOCKET=/tmp/ghost-smoke.sock qs -p shell`, logging to `/tmp/ghost-smoke.log`.
2. Waits 6 s.
3. Kills ghostd, waits 3 s, restarts it, and waits 5 s.
4. Stops both.
5. **Fails** if the log matches `ReferenceError|TypeError|is not defined|Cannot assign|Error:|failed to load|Unable to`.
6. **Fails** unless `ghostd connected` appears at least twice, which covers the reconnect.

Run: `make shaders && scripts/qml-smoke.sh`
Expected: FAIL (there's no `shell.qml` yet).

- [ ] **Step 2: Implement the services, the components, the shaders, Desktop and `shell.qml`**

- [ ] **Step 3: Run the smoke test and the logic tests**

Run: `make shaders && scripts/qml-smoke.sh && node --test shell/tests/`
Expected: `smoke: ok (2 connects, 0 errors)` and PASS.

- [ ] **Step 4: Check it visually**

Run: `GHOST_SOCKET=/tmp/ghost-smoke.sock qs -p shell & sleep 3; spectacle -b -n -f -o /tmp/gs-task13.png`
Then look at the screenshot. Expected:
- a `#0a0f11` screen with faint teal dots on a 24-design-px grid;
- the faint scanlines are visible on close inspection;
- no panels yet;
- windows draw above it.

- [ ] **Step 5: Commit**

```bash
git add shell Makefile scripts/qml-smoke.sh
git commit -m "feat(shell): add desktop surface, daemon client, theme and frame clock"
```

---

### Task 14: Components (Panel, SegMeter, BlockGraph, AlertBox, Digits, Kv)

**Files:**
- Create: `shell/components/{Panel.qml,SegMeter.qml,BlockGraph.qml,AlertBox.qml,Digits.qml,Kv.qml}`, `shell/shaders/stripes.frag`, `shell/views/Gallery.qml`
- Modify: `shell/views/Desktop.qml` (when `GHOST_VIEW=gallery`, load `Gallery`)

**Interfaces:**
- Consumes: Theme, FrameClock, `segments.mjs`, `theme.mjs`.
- Produces:
  - **`Panel { label: string; tagKey: string; meta: string; default property alias content; padding: Theme.d(12) }`**
    - The background is a `Shape` polygon with the 12 px chamfers, filled with `Theme.panel` at `Theme.panelFill`.
    - The frame is a 1 px `Theme.hair` edge plus 1.5 px `Theme.signal` brackets with 14 px arms on the top-left and bottom-right.
    - The header is 26 high: a 5 px square, the label (UPPERCASE, 10 px, weight 600, tracking .16 em), `Theme.tag(tagKey)` in signal, and `meta` right-aligned in dim.
    - All text uses `elide: Text.ElideRight`.
  - **`SegMeter { value: real (0..1); warnAt: Theme.loadWarn; critAt: Theme.loadCrit }`**: 7 px tall, with cells from `cellCount(width / Theme.u)`.
  - **`BlockGraph { values: var; lo: 0; hi: 100; cols: 0; cellW: 3; cellH: 2; gap: 1; warnAt; critAt; minLit: 1 }`**
    - `cols: 0` fits as many columns as `cellW` allows. The newest value is on the right.
    - It's built from Repeaters of `Rectangle`, and repaints only when `values` changes.
  - **`AlertBox { text: string; trailing: string; level: "warn"|"crit"; chipKey: string }`**
    - It has a 3 px left edge in the level's colour.
    - The stripes come from a `stripes.frag` ShaderEffect whose phase follows `FrameClock.frame` (one 16 px tile per 1.6 s).
    - On `Component.onCompleted`, the text runs the stepped arrival keyframes (opacity `0%:.3 6%:1 10%:.46 14%:1 40%:.82 70%:.64`) over 1.6 s with held frames. After that it binds to `FrameClock.blinkOpacity`.
    - It calls `FrameClock.acquire()` while visible.
  - **`Digits { text: string; pixelSize: real; weight: int }`**: a single `Text` when the font is monospace (`isMonospace` from `TextMetrics` of "0" and "1"). Otherwise each digit sits in a cell as wide as "0".
  - **`Kv { key: string; value: string }`**: the key in dim at 8.5 px with tracking .14 em, above the value in `hi` at 10 px.

- [ ] **Step 1: Write the gallery**

The gallery shows each component with fixed values:
- meters at 0.2, 0.8 and 0.95;
- a `BlockGraph` ramp from 0 to 100;
- AlertBoxes with `"STORM WARNING"` (warn) and `"WD GREEN 960G · nvme1"` / `"DEAD"` (crit);
- a `Digits` sample of `"20:07"`;
- three Panels.

Run: `GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: FAIL (the components don't exist yet).

- [ ] **Step 2: Implement the components and the stripe shader**

- [ ] **Step 3: Run the smoke test again**

Run: `make shaders && GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: `smoke: ok`.

- [ ] **Step 4: Compare it with the mockup and measure the animation cost**

Take a screenshot (as in Task 13) and open the reference mockup next to it. Check:
- the chamfers are on the top-right and bottom-left, and the brackets on the top-left and bottom-right;
- the 0.2 meter is teal, 0.8 amber and 0.95 red;
- the alert stripes move to the right, and the text blinks in visible held steps with no smooth fading;
- there is no glow anywhere.

Then run `scripts/measure-cpu.sh "$(pgrep -f 'qs -p shell')" 60` with the gallery up.
Expected: `cpu_pct` ≤ 1.50.

- [ ] **Step 5: Commit**

```bash
git add shell/components shell/shaders/stripes.frag shell/views
git commit -m "feat(shell): add panel, meter, graph, alert and digit components"
```

---

### Task 15: Left-column panels (Processor, Graphics, Memory)

**Files:**
- Create: `shell/panels/{ProcessorPanel.qml,GraphicsPanel.qml,MemoryPanel.qml}`

**Interfaces:**
- Consumes:
  - `Daemon.cpu`, `gpu`, `mem`, `sensors` and `history`;
  - the components from Task 14;
  - `labels.mjs` and `format.mjs`;
  - the sensor ids `k10temp/Tctl`, `k10temp/Tccd1` (CCD0), `k10temp/Tccd2` (CCD1) and `nct6799/fan2`.
- Produces the three panels, each filling its parent:
  - **`ProcessorPanel`:**
    - `label: "Processor"`, `tagKey: "cpu"`, `meta: cpuModelShort(cpu.model)`;
    - two side-by-side groups, each a `ccdTitle` line with its temperature (`tempShort`) above a `BlockGraph` with `cols: 16`, `cellH: 3`, `gap: 2`, values = that CCD's thread loads in `ccd.cpus` order, and warn/crit at 0.75 and 0.9. **These groups stretch to fill spare height.**
    - a history `BlockGraph` 40 high from `history.load`;
    - a `Kv` row: LOAD (`pct`), TCTL (`tempC`), FREQ (`ghz`), CPU FAN (`rpm`).
  - **`GraphicsPanel`:**
    - `label: "Graphics"`, `tagKey: "gpu"`, `meta: gpuNameShort(gpu.name)`;
    - a big `Digits` load % (30 px) with the dim caption `LOAD`;
    - `SegMeter` rows:
      - VRAM: `used / total`, shown as `"3.1 / 16G"`;
      - TEMP: `temp / 100`, with temp thresholds;
      - POWER: `power / power_limit`;
      - FAN: `fan_pct / 100`;
    - a history `BlockGraph` from `history.gpu_load`. **It stretches to fill spare height.**
    - When `present` is false, every value shows `--` in dim.
  - **`MemoryPanel`:**
    - `label: "Memory"`, `tagKey: "mem"`, `meta: gib(total) + " GIB"`;
    - a big used GiB with the dim caption `/ 91.9 GiB`;
    - RAM and SWAP `SegMeter`s, using the load thresholds;
    - top processes (NAME, CPU, MEM, the last two as `x.x%`). **The list stretches and clips whole rows.**

- [ ] **Step 1: Add the panels to the gallery and watch the smoke test fail**

Add the three panels at their V1 sizes: 300×250, 300×204 and 300×192.

Run: `GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: FAIL (the panels don't exist yet).

- [ ] **Step 2: Implement the three panels**

- [ ] **Step 3: Run the smoke test against fake and real data**

Run: `make shaders && GHOST_VIEW=gallery scripts/qml-smoke.sh`, then repeat against the real `ghostd run`.
Expected: `smoke: ok` both times.

- [ ] **Step 4: Compare it with the mockup**

Take a screenshot and compare it with the reference mockup's left column. Check:
- the values update once a second;
- CCD0 shows "V-CACHE";
- a thread above 75 % shows amber cells, and one above 90 % red;
- the text never overflows (it's elided instead).

- [ ] **Step 5: Commit**

```bash
git add shell/panels shell/views/Gallery.qml
git commit -m "feat(shell): add processor, graphics and memory panels"
```

---

### Task 16: Storage and Sensors panels, and the V1 layout

**Files:**
- Create: `shell/panels/{StoragePanel.qml,SensorsPanel.qml,PlaceholderPanel.qml}`, `shell/views/V1.qml`, `shell/lib/layout.mjs`, `shell/tests/layout.test.mjs`
- Modify: `shell/views/Desktop.qml` (load `V1` by default)

**Interfaces:**
- Consumes: Daemon `disks` and `sensors`, the components, and the Task 15 panels.
- Produces:
  - **`StoragePanel`:**
    - `label: "Storage"`, `tagKey: "disk"`, `meta: drives.length + " DRIVES"`.
    - `Live` drives get two lines: `label · mounts` with the temperature on the right, then a `SegMeter` of used % (warn/crit 0.85/0.95) with the percentage.
    - `Dead` drives get `AlertBox { level: "crit"; chipKey: "crit"; text: label + " · " + kernel_name; trailing: "DEAD" }`.
    - `Unmounted` drives get one dim line ending in `UNMOUNTED`.
    - A footer shows R/W totals (`rate`).
  - **`SensorsPanel`:**
    - `label: "Sensors"`, `tagKey: "sens"`, `meta: count + " PROBES"`.
    - Temperatures (with level colours from the temperature thresholds), a 4 px gap, then fans.
    - An unavailable value is `--` in dim.
  - **`PlaceholderPanel { label: string; meta: "PLAN 2" }`**: an empty Panel standing in for later panels.
  - **`layout.mjs`:**
    - `V1` holds:
      - top inset 42 (a 30 px band is reserved for Plan 3's bar), side and bottom margins 12, gutters 12;
      - the left column: 300 wide, with rows `[250, 204, "flex"]`;
      - the centre: a `[262, "flex"]` stack whose second row is split `[290, "flex"]`;
      - the right column: 300 wide, with rows `[300, "flex", 184]`.
    - `fits(column, available)` is true when the fixed rows plus the gutters are under `available` (`720 − 42 − 12 = 666`).
  - **`V1.qml`:** lays out the left column (Processor, Graphics, Memory), the centre (Placeholder "Clock" on top; Placeholder "Media" and Placeholder "Notifications" below) and the right column (Placeholder "Uplink", Storage, Sensors), all in `Theme.d()` units.

- [ ] **Step 1: Write the failing layout test**

```js
test('v1 columns fit the 666 px body', () => { assert.ok(fits(V1.left, 666)); assert.ok(fits(V1.right, 666)); });
test('flex row leaves room for its content', () => assert.ok(666 - (300 + 184 + 2 * 12) >= 150));
```

Run: `node --test shell/tests/layout.test.mjs`
Expected: FAIL (the module is missing).

- [ ] **Step 2: Implement `layout.mjs`, the panels and `V1.qml`**

- [ ] **Step 3: Run the tests and the smoke test**

Run: `node --test shell/tests/ && make shaders && scripts/qml-smoke.sh`
Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Check it visually at two scales**

Take screenshots against the real daemon at the native `u`, and again with `GHOST_FORCE_U=1.0`. Check:
- it matches the mockup's V1 geometry, with placeholders where Plan 2 panels go;
- the WD Green shows as a red alert line that blinks in steps;
- the unmounted WD Blue and Seagate are dim;
- the dead NVMe's sensor row shows `--`;
- nothing overflows at either scale.

- [ ] **Step 5: Commit**

```bash
git add shell
git commit -m "feat(shell): add storage and sensors panels and the v1 layout"
```

---

### Task 17: Install, run as user services, and check the acceptance criteria

**Files:**
- Create: `packaging/systemd/{ghostd.service,ghost-shell.service}`, `scripts/install.sh`, `README.md`, `docs/superpowers/notes/2026-10-20-plan1-acceptance.md`
- Modify: `Makefile` (add `release` and `install`)

**Interfaces:**
- Produces:
  - **`scripts/install.sh`** (idempotent):
    1. `cargo build --release`.
    2. Install `ghostd` to `~/.local/bin/`.
    3. Copy `shell/` to `~/.local/share/ghost-shell/shell/`, including the compiled `.qsb` files.
    4. If Kode Mono is missing, download `KodeMono[ROND,wdth,wght].ttf` and `OFL.txt` from `https://github.com/google/fonts/raw/main/ofl/kodemono/` into `~/.local/share/fonts/ghost-shell/`, then run `fc-cache -f`.
    5. Install the units to `~/.config/systemd/user/` and run `systemctl --user daemon-reload`.

    It **doesn't** enable anything at login, and it doesn't touch Plasma, gridlock or conky (that's Plan 4).
  - **`ghostd.service`:** `ExecStart=%h/.local/bin/ghostd run`, `Restart=on-failure`, `PartOf=graphical-session.target`.
  - **`ghost-shell.service`:** `ExecStart=/usr/bin/qs -p %h/.local/share/ghost-shell/shell`, `Wants=ghostd.service`, `After=ghostd.service`, `Restart=on-failure`, `PartOf=graphical-session.target`.
  - **`README.md`:** build, test, install, run, and where the config lives (`ghostd default-config > ~/.config/ghost-shell/config.toml`).

- [ ] **Step 1: Install and start it**

**Ask the user before stopping gridlock for this session.**

Run: `scripts/install.sh && systemctl --user start ghostd ghost-shell && sleep 5 && systemctl --user is-active ghostd ghost-shell`
Expected: `active` twice. The V1 machine panels appear on the desktop.

- [ ] **Step 2: Measure the budgets**

Run `scripts/measure-cpu.sh` for 60 s on each pid.
Expected:
- the combined `cpu_pct` is ≤ 2.00 (the dead-drive alert animates);
- the combined RSS is ≤ 307200 KiB.

Record the numbers.

- [ ] **Step 3: Check the live config**

1. Run `ghostd default-config > ~/.config/ghost-shell/config.toml`. Set `panel_fill = 30` and save.
   Expected: the panels become more transparent within 1 s.
2. Set `warn = "orange"` and save.
   Expected: a notification reading "appearance.colors.warn: expected #rrggbb, got "orange"", while the colours stay unchanged.
3. Restore the valid value.

- [ ] **Step 4: Write the acceptance note**

Write `docs/superpowers/notes/2026-10-20-plan1-acceptance.md` with the measurements, the screenshot paths and any deviations. Stop the services, and **restart gridlock if the user wants it back**.

- [ ] **Step 5: Commit**

```bash
git add packaging scripts/install.sh README.md Makefile docs/superpowers/notes/2026-10-20-plan1-acceptance.md
git commit -m "chore: add install script, user services and plan 1 acceptance notes"
```

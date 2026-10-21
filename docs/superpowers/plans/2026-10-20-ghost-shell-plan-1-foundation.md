# ghost-shell Plan 1 — Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** By the end of this plan:
- `ghostd` streams real telemetry, the live-reloaded config, the resolved widget layout, and data relayed from external providers over a Unix socket.
- A Quickshell desktop surface renders V1 from that layout.
- The five machine panels (Processor, Graphics, Memory, Storage, Sensors) and a placeholder exist as **pluggable widgets**: each is a folder with a manifest and a QML file, built on a shared `GhostKit` module, in the final style and within the spec's idle budget.

**Architecture:**
- **A Rust workspace with two crates:**
  - `ghost-proto` holds the wire types, config types and layout types, sent as JSON lines. Topics are open strings.
  - `ghostd` holds:
    - the collectors, which read from a configurable `/proc` + `/sys` root;
    - the history buffers;
    - config load, validation and watching;
    - widget manifest discovery, and validation of the layout against the manifests;
    - a tokio Unix-socket server that relays external providers;
    - the fake mode.
- **The Quickshell config in `shell/`:**
  - the `GhostKit` QML module, holding the Theme, FrameClock and Data singletons and the Panel, SegMeter, BlockGraph, AlertBox, Digits, Kv, DotGrid and Scanlines components, plus Node-tested ES modules;
  - a host that turns the resolved layout tree into rectangles and loads each slot's widget, showing an error card when a widget fails.
- **Built-in widgets** live in `widgets/<id>/` and use exactly the contract a third-party widget would.

**Tech Stack:**
- Rust 1.98: tokio (current-thread), serde, serde_json, toml, notify, nvml-wrapper, nix, zbus, thiserror, tracing, tracing-subscriber; dev dependency tempfile.
- Quickshell 0.2.1 on Qt 6.11: QtQuick, QtQuick.Shapes, ShaderEffect with shaders compiled by `qsb-qt6`.
- Node 24 (`node --test`).
- GNU make.

**Spec:** `docs/superpowers/specs/2026-10-20-ghost-shell-design.md`. §13 covers extensibility and §14 configuration. The reference mockup `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html` is the visual truth.

**Roadmap.** This is plan 1 of 4. Each later plan is written after the previous one ships.
1. **This plan:** the foundation and the widget/provider contract.
2. The rest of the dashboard, each piece as a widget:
   - panels: network, uplink globe (Natural Earth), silicon, process tree, media with idle states and audio rows, weather with sun, moon, AQI and the INMET provider, clock;
   - views: V2 and MONITOR layouts, print transitions, desktop-only shortcuts;
   - text scramble and the Japanese chips;
   - the greeting.
3. The bar as a layout of bar widgets:
   - the brand block (configurable or off), tasks, tray, view chip, clock;
   - layouts A/B/C, top or bottom position, mono/colour icons;
   - the launcher;
   - the KWin TabBox switcher;
   - notifications.
4. Plasma takeover:
   - keeping every function in §4;
   - migration (with consent);
   - the Chakra Petch Slashed generator;
   - packaging;
   - the author's guide, an example widget and provider, and topic JSON schemas;
   - generating the user's machine-specific config on first run;
   - a native-resolution audit.

## Global Constraints

- **Pluggable by contract:**
  - Built-in widgets use only the public widget contract: the manifest, `Widget.qml`, the `GhostKit` imports and an injected `settings` object.
  - No panel's placement is hard-coded; placement comes from `layout.*` in the config file.
  - **Views are named layouts:** any number under `layout.views.<name>`. A layout tree has three node types: row/column splits, widget slots, and `tabs` containers. A `tabs` container loads only its active tab's widget.
  - Topic names match `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$` and are at most 64 characters.
- **Widget API version is `1`.**
- **Design units:** every size is mockup px at 1280×720. On screen, multiply by `u = surfaceLogicalWidth / 1280` (≈ 1.74 on the 2560×1440 M27Q at scale 1.15). Never hard-code the scale.
- **Palette defaults:**
  - `ground #0a0f11`, `panel #0a0f11` at **55 %** fill, `hair #2a3a3d`, `hair2 #1a2527`;
  - `text #b7d1cd`, `hi #e4f1ef`, `dim #587170`;
  - `signal #a3d4cc`, `warn #e9a84c`, `crit #ff3b45`;
  - `dot #a3d4cc` at **16 %** on a **24 px** grid;
  - scanlines: a **1 px** dark line every **3 px** at **15 %** black.
- **Colour only for attention:**
  - loads ≥ **75 %** warn and ≥ **90 %** crit;
  - temperatures ≥ **80 °C** warn and ≥ **90 °C** crit;
  - batteries ≤ **20 %** warn and ≤ **10 %** crit;
  - US AQI > **100** warn and > **150** crit;
  - a dead drive is crit.
- **Panels:**
  - a 45° chamfer of **12 px** on the top-right and bottom-left corners;
  - **1.5 px** signal-colour brackets with **14 px** arms on the top-left and bottom-right corners;
  - a 1 px `hair` edge;
  - a header with a **5 px** signal square, an UPPERCASE label (10 px, weight 600, tracking .16 em), the Japanese tag, and right-aligned `dim` meta.
- **Segmented bars:** meter cells are **4 px** wide with **2 px** gaps and **7 px** tall; graphs are columns of cells; unlit cells are signal at **6 %**.
- **Layout:** gutter **12**, margin **12**, and a **30 px** band reserved for the bar (Plan 3).
- **Font:** config `appearance.font`, default **"Kode Mono"**; CJK fallback **"Noto Sans CJK JP"**. Numbers never shift.
- **Japanese tags:** mode `kanji | katakana | mixed | off`, default `mixed`, where mixed means kanji tags.

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
  - a 16 px stripe tile scrolling one tile per 1.6 s;
  - text does a held-frame arrival flicker over 1.6 s, then the cursor blink: a 1 s loop of four held frames, `1, .5, 0, .5`.
- **Motion:** decorative motion runs at ≤ **15 fps** in held steps, never at the 170 Hz refresh rate. Data widgets repaint only on new data (1 Hz).
- **Budgets:** shell + daemon ≤ **2 %** of one core (averaged over 60 s) while animating, ≤ **1 %** when still, ≤ **300 MB** RSS.
- **Daemon behaviour:** no forks, scripts or busy polling in steady state. The only exception is starting each configured provider once, and restarting it when it exits. A broken source degrades one row, never a widget or the daemon.
- **Drives** are identified by model + serial.
- **Formats:** dates `DD.MM.YY`, 24-hour time, °C.
- **Generic defaults:**
  - `display.model = ""` (the first screen);
  - no location;
  - `bar.pinned = []`;
  - `bar.brand = { enabled = true, text = "GS//01", subtitle = "攻殻" }`;
  - the greeting name defaults to the login name.
- **Config:** `$XDG_CONFIG_HOME/ghost-shell/config.toml`, reloaded live. An invalid file keeps the last good config and raises a warning notification naming the problem.

## Review Focus

1. **A third-party widget is broken** (bad QML, a missing file, `api = 2`), or a layout names a widget that isn't installed. That slot shows an error card naming the widget and the reason; every other slot renders. *Tests: Task 5 and Task 16.*
2. **A sensor read fails mid-run** (the dead WD NVMe gives EIO; the DualSense is unplugged). That reading shows as unavailable (`--` in `dim`) and the rest still updates. *Tests: Task 8 and Task 17.*
3. **An editor replaces the config file by atomic rename, or the file is half-written.** The watcher survives, and a parse error keeps the last good config. *Test: Task 4.*
4. **The shell starts before the daemon, a provider crashes, or the daemon restarts.** The shell reconnects with backoff and shows `dim` values, not zeros. A dead provider's topics go stale without affecting the others. *Tests: Task 12 and Task 14.*
5. **Two drives share a model, or the kernel renumbers NVMe drives.** Identity comes from the serial and the order is stable. *Test: Task 10.*

---

### Task 1: Rendering spike (layer, frame pacing, cost)

This decides how every animation is driven, and must finish before Task 14. It's a spike: the deliverable is a measurement script and a decision note.

**Files:**
- Create: `scripts/measure-cpu.sh`, `spikes/pacing/shell.qml`, `spikes/pacing/dots.frag`, `docs/superpowers/notes/2026-10-20-rendering-spike.md`
- Modify: `.gitignore` (add `target/` and `*.qsb`)

**Interfaces:**
- Produces:
  - `scripts/measure-cpu.sh <pid> <seconds>` prints one line, `cpu_pct=<2 decimals> rss_kib=<int>`. CPU is the average % of one core, from the `/proc/<pid>/stat` `utime+stime` delta with `getconf CLK_TCK`; RSS is `VmRSS` at the end. With an unknown pid it prints `no such process <pid>` and exits 2.
  - The note's **Decision** section, which Task 14 follows. It fixes:
    - the layer: `Bottom` or `Background`;
    - the FrameClock rate;
    - whether occluded surfaces stop rendering on their own.

- [ ] **Step 1: Check the script against a known-idle process**

Run: `sleep 30 & scripts/measure-cpu.sh $! 3; scripts/measure-cpu.sh 999999 1; echo "exit=$?"`
Expected: `cpu_pct=0.0x rss_kib=N`, then `no such process 999999` and `exit=2`.

- [ ] **Step 2: Write the spike shell and compile its shader**

`spikes/pacing/shell.qml` puts a full-screen `PanelWindow` on the screen whose model contains `M27Q`, on `WlrLayer.Bottom` with `ExclusionMode.Ignore`. It draws a dot-grid ShaderEffect: 1 px dots, 24 spacing, 16 % alpha. `SPIKE_MODE` picks what else runs:
- `static`: nothing else;
- `data`: 600 `Rectangle`s recoloured every 1000 ms;
- `anim`: `data`, plus a square stepped 1 px every 66 ms by a `Timer`.

Compile the shader: `qsb-qt6 --qt6 -o spikes/pacing/dots.frag.qsb spikes/pacing/dots.frag`

- [ ] **Step 3: Measure each mode**

**Ask the user before stopping gridlock** (`pkill -f "qs -c gridlock"`). Restart it afterwards with `qs -c gridlock -d`.

For each mode, run:
```bash
SPIKE_MODE=<mode> QSG_RENDER_TIMING=1 qs -p spikes/pacing 2> /tmp/spike-<mode>.log &
sleep 5; scripts/measure-cpu.sh "$(pgrep -f 'qs -p spikes/pacing')" 60; pkill -f 'qs -p spikes/pacing'
grep -c "Frame rendered" /tmp/spike-<mode>.log
```
Repeat `anim` with a maximised window covering the whole screen.

Record CPU %, RSS and frames per second. Also record whether the surface sits above Plasma's desktop, sits below windows, and stays visible under Meta+D.

- [ ] **Step 4: Write the decision note**

- **Acceptance:** `static` renders about 0 frames/s, and `anim` renders ≤ 15 frames/s at ≤ 1.5 % CPU.
- **Otherwise,** record the mitigation: a lower rate, or separate small surfaces for animated regions.
- **If covered surfaces keep rendering,** record that Plan 2 needs occlusion detection.

- [ ] **Step 5: Commit**

```bash
git add scripts/measure-cpu.sh spikes/pacing docs/superpowers/notes/2026-10-20-rendering-spike.md .gitignore
git commit -m "chore: add rendering spike and cpu measurement script"
```

---

### Task 2: Workspace and the open wire protocol (`ghost-proto`)

**Files:**
- Create: `Cargo.toml` (workspace with members `crates/*`, `resolver = "3"`, edition 2024), `crates/ghost-proto/Cargo.toml`, `crates/ghost-proto/src/{lib.rs,msg.rs,snapshot.rs}`
- Test: `crates/ghost-proto/tests/wire.rs`

**Interfaces:**
- Produces (all `Serialize + Deserialize + Debug + Clone + PartialEq`, messages tagged `#[serde(tag = "t", rename_all = "snake_case")]`):
  - `pub const PROTO_VERSION: u32 = 1;`
  - `pub mod topics { pub const CPU: &str = "cpu"; MEM = "mem"; SENSORS = "sensors"; GPU = "gpu"; DISKS = "disks"; PERF = "perf"; pub const BUILTIN: [&str; 6]; }`
  - `pub fn is_valid_topic(name: &str) -> bool` implements the Global Constraints rule.
  - `pub enum ClientMsg`:
    - `Hello { proto: u32, topics: Vec<String> }`: a UI client.
    - `Subscribe { topics: Vec<String> }`: replaces the client's subscription set.
    - `Provide { proto: u32, name: String, topics: Vec<String> }`: an external provider.
    - `Publish { topic: String, data: serde_json::Value }`.
  - `pub enum ServerMsg`:
    - `Hello { proto: u32, host: String }`
    - `Error { message: String }`
    - `History { step_ms: u32, series: BTreeMap<String, Vec<f32>> }`
    - `Data { topic: String, seq: u64, data: serde_json::Value }`

    Task 3 adds `Config` and `ConfigError`; Task 5 adds `Layout`.
  - `pub fn data_msg<T: Serialize>(topic: &str, seq: u64, v: &T) -> ServerMsg`
  - Snapshot types for the built-in topics, all `Default`:
    - `CpuSnapshot { model: String, threads: Vec<f32>, ccds: Vec<CcdLoad>, load: f32, freq_mhz: u32 }`
    - `CcdLoad { id: u8, cpus: Vec<u16>, l3_kib: u32, vcache: bool, load: f32 }`
    - `MemSnapshot { total_kib: u64, used_kib: u64, swap_total_kib: u64, swap_used_kib: u64, top: Vec<ProcUsage> }`
    - `ProcUsage { pid: u32, name: String, cpu: f32, mem: f32 }`, both % of the whole machine.
    - `SensorSnapshot { temps: Vec<Reading>, fans: Vec<Reading> }`
    - `Reading { id: String, label: String, value: Option<f32> }`
    - `GpuSnapshot { present: bool, name: String, load: f32, vram_used_mib: u32, vram_total_mib: u32, temp_c: f32, power_w: f32, power_limit_w: f32, fan_pct: f32 }`
    - `DiskSnapshot { drives: Vec<Drive> }`
    - `Drive { id: String, label: String, model: String, kernel_name: String, state: DriveState, mounts: Vec<String>, used_pct: Option<f32>, temp_c: Option<f32>, read_bps: u64, write_bps: u64 }`
    - `DriveState { Live, Dead, Unmounted }`, `snake_case`.
    - `PerfSnapshot { daemon_cpu: f32, daemon_rss_kib: u64, shell_cpu: Option<f32>, shell_rss_kib: Option<u64> }`
  - `pub fn encode_line(m: &ServerMsg) -> String` returns compact JSON plus `\n`.
  - `pub fn decode_client_line(l: &str) -> Result<ClientMsg, WireError>`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn hello_exact_wire_form() { assert_eq!(encode_line(&ServerMsg::Hello { proto: 1, host: "tkacz-workstation".into() }), "{\"t\":\"hello\",\"proto\":1,\"host\":\"tkacz-workstation\"}\n"); }
#[test] fn data_envelope_wraps_snapshot() {
    let v: serde_json::Value = serde_json::from_str(&encode_line(&data_msg(topics::GPU, 3, &GpuSnapshot { present: true, ..Default::default() }))).unwrap();
    assert_eq!(v["t"], "data"); assert_eq!(v["topic"], "gpu"); assert_eq!(v["seq"], 3); assert_eq!(v["data"]["present"], true);
}
#[test] fn provider_messages_decode() {
    assert_eq!(decode_client_line(r#"{"t":"provide","proto":1,"name":"inmet","topics":["weather.alerts"]}"#).unwrap(),
               ClientMsg::Provide { proto: 1, name: "inmet".into(), topics: vec!["weather.alerts".into()] });
    assert!(matches!(decode_client_line(r#"{"t":"publish","topic":"weather.alerts","data":{"n":1}}"#).unwrap(), ClientMsg::Publish { .. }));
}
#[test] fn topic_names() { assert!(is_valid_topic("weather.alerts")); assert!(is_valid_topic("cpu")); assert!(!is_valid_topic("Weather")); assert!(!is_valid_topic("a..b")); assert!(!is_valid_topic(&"a".repeat(65))); }
#[test] fn unavailable_reading_is_null() { assert_eq!(serde_json::to_value(Reading { value: None, ..Default::default() }).unwrap()["value"], serde_json::Value::Null); }
#[test] fn drive_state_wire_names() { assert_eq!(serde_json::to_string(&DriveState::Dead).unwrap(), "\"dead\""); }
#[test] fn rejects_garbage() { assert!(decode_client_line("{nope").is_err()); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghost-proto`
Expected: compile errors.

- [ ] **Step 3: Implement the module**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghost-proto`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/ghost-proto
git commit -m "feat(proto): add open-topic wire protocol and built-in snapshot types"
```

---

### Task 3: Config schema, generic defaults and validation

**Files:**
- Create: `crates/ghost-proto/src/config.rs`, `crates/ghostd/{Cargo.toml,src/main.rs,src/lib.rs}`, `crates/ghostd/src/config/{mod.rs,load.rs,default.toml}`
- Modify: `crates/ghost-proto/src/msg.rs`, adding `ServerMsg::Config { data: ResolvedConfig }` and `ServerMsg::ConfigError { message: String }`
- Test: `crates/ghostd/tests/config_load.rs`

**Interfaces:**
- Produces, in `ghost-proto`: `ResolvedConfig` and its sections. Every field has a default; serde uses `deny_unknown_fields`; `Default` gives exactly these values.
  - `appearance`:
    - `font = "Kode Mono"`, `font_cjk = "Noto Sans CJK JP"`;
    - `japanese = Mixed` (`Kanji | Katakana | Mixed | Off`);
    - `panel_fill = 55` (0..=100);
    - `colors { ground "#0a0f11", panel "#0a0f11", hair "#2a3a3d", hair2 "#1a2527", text "#b7d1cd", hi "#e4f1ef", dim "#587170", signal "#a3d4cc", warn "#e9a84c", crit "#ff3b45", dot "#a3d4cc" }`;
    - `dots { spacing = 24, opacity = 16 }`, `scanlines { opacity = 15 }`.
  - `thresholds { load_warn 75, load_crit 90, temp_warn 80, temp_crit 90, battery_warn 20, battery_crit 10, aqi_warn 100, aqi_crit 150, sensors: BTreeMap<String, SensorThreshold { warn: f32, crit: f32 }> }`
  - `display { model = "" }`
  - `sensors { labels: BTreeMap<String,String> }`, `disks { labels: BTreeMap<String,String> }` (keyed by serial)
  - `dashboard { main_views: Vec<String> = ["v1"], monitor_view: String = "", default_view: String = "v1", history_minutes = 5 (1..=60) }`. Plan 2 adds `v2` and `monitor` to the defaults once those layouts exist.
  - `bar { layout = A (A|B|C), position = Top (Top|Bottom), icons = Mono (Mono|Colour), pinned: Vec<String> = [], brand { enabled = true, text = "GS//01", subtitle = "攻殻" } }`
  - `greeting { name = "" }`
  - `location { city = "", lat: Option<f64> = None, lon: Option<f64> = None, alerts { provider = "", area = "" } }`
  - `shortcuts { monitor = "Meta+M", swap = "Meta+Shift+M" }`
  - `notifications { history_limit = 200, popup_timeout_s = 6 }`
  - `layout { gutter = 12.0, margin = 12.0, views: BTreeMap<String, LayoutNode> }`, whose default is the V1 tree below.
  - `widgets { paths: Vec<String> = [] }`
  - `providers: Vec<ProviderSpec { name: String, exec: Vec<String>, restart: bool = true }>`
  - `LayoutNode`, `#[serde(untagged)]`:
    - `Slot { widget: String, size: Option<f32>, settings: serde_json::Map }`
    - `Split { direction: Direction (Row | Column), size: Option<f32>, children: Vec<LayoutNode> }`
    - `Tabs { tabs: Vec<TabSpec { widget: String, title: Option<String>, settings: serde_json::Map }>, size: Option<f32>, default: usize = 0 }`
- **The default `layout.views.v1`**, sizes in design px, no `size` meaning flex:
  - `Row`:
    - `Column` size 300:
      - `processor` 250
      - `graphics` 204
      - `memory`
    - `Column`:
      - `placeholder` 262 `{label="Clock"}`
      - `Row`:
        - `placeholder` 290 `{label="Media"}`
        - `placeholder` `{label="Notifications"}`
    - `Column` size 300:
      - `placeholder` 300 `{label="Uplink"}`
      - `storage`
      - `sensors` 184
- Produces, in `ghostd::config`:
  - `DEFAULT_TOML`: a commented file holding every default.
  - `load_str_with_user(s, user) -> Result<ResolvedConfig, ConfigError>`, which replaces an empty `greeting.name` with `user`.
  - `load_str(s)`, which uses `$USER`.
  - `load_path(&Path)`. A missing file gives the defaults.
  - `default_path()`.
  - `ConfigError { message: String }`.
- CLI: `ghostd default-config`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn default_file_equals_builtin_defaults() { let mut d = ResolvedConfig::default(); d.greeting.name = "gtkacz".into(); assert_eq!(load_str_with_user(DEFAULT_TOML, "gtkacz").unwrap(), d); }
#[test] fn defaults_are_machine_neutral() { let c = load_str_with_user("", "u").unwrap(); assert_eq!(c.display.model, ""); assert!(c.bar.pinned.is_empty()); assert_eq!(c.location.lat, None); assert_eq!(c.bar.brand.text, "GS//01"); assert!(c.bar.brand.enabled); }
#[test] fn partial_override_merges() { let c = load_str_with_user("[appearance]\npanel_fill = 40\n", "u").unwrap(); assert_eq!(c.appearance.panel_fill, 40); assert_eq!(c.appearance.colors.warn, "#e9a84c"); }
#[test] fn brand_can_be_turned_off_and_renamed() { let c = load_str_with_user("[bar.brand]\nenabled = false\ntext = \"TKZ//07\"\n", "u").unwrap(); assert!(!c.bar.brand.enabled); assert_eq!(c.bar.brand.text, "TKZ//07"); }
#[test] fn bad_colour_names_the_key() { assert_eq!(load_str_with_user("[appearance.colors]\nwarn = \"orange\"\n", "u").unwrap_err().message, "appearance.colors.warn: expected #rrggbb, got \"orange\""); }
#[test] fn warn_below_crit() { assert_eq!(load_str_with_user("[thresholds]\nload_warn = 95\n", "u").unwrap_err().message, "thresholds: load_warn (95) must be below load_crit (90)"); }
#[test] fn battery_order_is_reversed() { assert_eq!(load_str_with_user("[thresholds]\nbattery_crit = 30\n", "u").unwrap_err().message, "thresholds: battery_crit (30) must be below battery_warn (20)"); }
#[test] fn fill_out_of_range() { assert_eq!(load_str_with_user("[appearance]\npanel_fill = 120\n", "u").unwrap_err().message, "appearance.panel_fill: must be 0..=100, got 120"); }
#[test] fn unknown_key_rejected() { assert!(load_str_with_user("[appearance]\nglow = true\n", "u").unwrap_err().message.contains("unknown field `glow`")); }
#[test] fn default_v1_layout_shape() { let c = load_str_with_user("", "u").unwrap(); let LayoutNode::Split { children, .. } = &c.layout.views["v1"] else { panic!() }; assert_eq!(children.len(), 3); }
#[test] fn user_layout_replaces_default_view() { /* [layout.views.v1] with a single Slot{widget="memory"} → views["v1"] is that slot */ }
#[test] fn views_must_exist() { assert_eq!(load_str_with_user("[dashboard]\nmain_views = [\"v1\", \"v2\"]\n", "u").unwrap_err().message, "dashboard.main_views: view \"v2\" is not defined in layout.views"); }
#[test] fn extra_view_and_tabs_parse() { /* [layout.views.v3] = { tabs = [{widget="processor"},{widget="memory", title="RAM"}], default = 1 } + main_views ["v1","v3"] → Ok, views["v3"] is Tabs with 2 tabs */ }
#[test] fn provider_spec_parses() { let c = load_str_with_user("[[providers]]\nname = \"inmet\"\nexec = [\"inmet-provider\", \"--area\", \"3550308\"]\n", "u").unwrap(); assert_eq!(c.providers[0].exec[2], "3550308"); assert!(c.providers[0].restart); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test config_load`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the types, the defaults, `validate` and `load_*`**

`validate` checks:
- hex colours, ranges, and warn/crit ordering (reversed for batteries);
- provider names are unique and non-empty, and `exec` is non-empty;
- every name in `main_views` and `default_view`, and a non-empty `monitor_view`, exists in `layout.views`. The exact message is `dashboard.main_views: view "v2" is not defined in layout.views`;
- `default_view` is in `main_views`;
- a `tabs` node has at least one tab, and `default` < its tab count.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test config_load`
Expected: PASS (14 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src crates/ghostd
git commit -m "feat(config): add config schema with generic defaults, layout tree and providers"
```

---

### Task 4: Config watcher with keep-last-good reload

**Files:**
- Create: `crates/ghostd/src/config/watch.rs`
- Test: `crates/ghostd/tests/config_watch.rs`

**Interfaces:**
- Consumes: `load_path` (Task 3).
- Produces: `pub fn spawn_watcher(path: PathBuf, initial: ResolvedConfig) -> notify::Result<(WatchHandle, watch::Receiver<ResolvedConfig>, mpsc::Receiver<ConfigError>)>`.
  - It watches the **parent directory** and debounces events by 150 ms.
  - When the reparse succeeds and the result differs, it publishes the new config. When it fails, it sends the error and keeps the last good config.
  - When the file is removed, it sends `"config file removed; keeping last configuration"`.
  - Dropping the handle stops the watcher.

- [ ] **Step 1: Write the failing tests** (tokio; each waits ≤ 2 s)

```rust
#[tokio::test] async fn plain_save_publishes() { /* panel_fill = 40 → watch value 40 */ }
#[tokio::test] async fn atomic_rename_publishes() { /* write .tmp, rename over → 30 */ }
#[tokio::test] async fn half_written_keeps_last_good() { /* "[appearance\npanel_fi" → error received, value 55; then valid 35 → 35 */ }
#[tokio::test] async fn removed_file_keeps_last_good() { /* remove → "config file removed; keeping last configuration", value unchanged */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test config_watch`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it with `notify::RecommendedWatcher` and a tokio debounce**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test config_watch`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/config/watch.rs crates/ghostd/tests/config_watch.rs
git commit -m "feat(config): watch config directory and keep last good config on errors"
```

---

### Task 5: Widget manifests and layout resolution

**Files:**
- Create: `crates/ghost-proto/src/layout.rs`, `crates/ghostd/src/widgets/{mod.rs,manifest.rs,resolve.rs}`
- Modify: `crates/ghost-proto/src/msg.rs`, adding `ServerMsg::Layout { data: ResolvedLayout }`
- Test: `crates/ghostd/tests/widgets.rs`

**Interfaces:**
- Consumes: `ResolvedConfig.layout` and `ResolvedConfig.widgets` (Task 3).
- Produces:
  - **Manifest format** (`manifest.toml`; this is the public contract):
    - `id` (matching `^[a-z][a-z0-9-]*$`), `name`, `version` (semver), `api` (integer);
    - `kinds = ["panel" | "bar", …]`, `requires = [topic, …]`, `animates = bool` (default false);
    - `[size] min = [w, h]` and `preferred = [w, h]`;
    - `[settings.<key>] type = "bool"|"int"|"float"|"string"|"color"|"enum"|"list"`, `default = …`, `values = [...]` (enum only).
  - `pub const WIDGET_API: u32 = 1;`
  - `pub fn discover(dirs: &[PathBuf]) -> (BTreeMap<String, Manifest>, Vec<String>)`.
    - Dirs are searched in order; a later dir's widget with the same id replaces an earlier one.
    - The second value is the list of non-fatal problems, each naming the manifest path.
    - The default dirs are the built-in widget dir (`$GHOST_WIDGETS_DIR`, else `~/.local/share/ghost-shell/widgets`), then `~/.config/ghost-shell/widgets`, then `widgets.paths`.
  - `pub fn resolve(layout: &LayoutConfig, manifests: &BTreeMap<String, Manifest>) -> ResolvedLayout`. It **never fails**. Problems become per-slot `error` strings, and are also collected into `errors`.
  - `ResolvedLayout { gutter: f32, margin: f32, views: BTreeMap<String, ResolvedNode>, errors: Vec<String> }`
  - `ResolvedNode`:
    - `Split { direction, size, children }`
    - `Slot { widget: String, source: Option<String>, size: Option<f32>, settings: serde_json::Map, requires: Vec<String>, animates: bool, error: Option<String> }`, where `source` is the `file://` URL of `Widget.qml`.
    - `Tabs { size: Option<f32>, default: usize, tabs: Vec<ResolvedTab { title: String, slot: ResolvedNode::Slot }> }`. `title` is the override, else the manifest `name`, uppercased.
  - **Exact error strings:**
    - `no widget named "x" is installed`
    - `widget "x" targets API 2; this ghost-shell supports API 1`
    - `widget "x" has no setting "foo"`
    - `widget "x" setting "history": expected bool, got "yes"`
    - `widget "x" can't be placed in a panel slot`

    In `errors`, each one is prefixed with its path, e.g. `layout.v1[0][1]: `.
  - Slot settings are filled from the manifest defaults.

- [ ] **Step 1: Write the failing tests** (tempdir widget folders)

```rust
#[test] fn resolves_known_widget_with_defaults() { /* manifest graphics{settings.history bool default true}; slot {widget="graphics"} → source ends "graphics/Widget.qml", settings["history"] == true, error None */ }
#[test] fn unknown_widget_is_a_slot_error_not_a_failure() { /* slot {widget="nope"} → error Some("no widget named \"nope\" is installed"); other slots fine */ }
#[test] fn api_mismatch_is_reported() { /* api = 2 → "widget \"x\" targets API 2; this ghost-shell supports API 1" */ }
#[test] fn bad_setting_key_and_type() { /* {foo=1} → "has no setting \"foo\""; {history="yes"} → "setting \"history\": expected bool, got \"yes\"" */ }
#[test] fn errors_carry_layout_path() { /* second child of first column → errors[0] starts with "layout.v1[0][1]: " */ }
#[test] fn user_dir_overrides_builtin() { /* same id in two dirs → source from the later dir */ }
#[test] fn tabs_resolve_each_tab() { /* tabs [processor, nope] → tab 0 resolved with title "PROCESSOR", tab 1 error "no widget named \"nope\" is installed", path "layout.v3[1]" */ }
#[test] fn bar_only_widget_rejected_in_panel_slot() { /* kinds=["bar"] → "widget \"x\" can't be placed in a panel slot" */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test widgets`
Expected: FAIL to compile.

- [ ] **Step 3: Implement `discover`, `resolve` and the proto types**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test widgets`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src/layout.rs crates/ghost-proto/src/msg.rs crates/ghostd/src/widgets crates/ghostd/tests/widgets.rs
git commit -m "feat(ghostd): discover widget manifests and resolve layouts against them"
```

---

### Task 6: SysRoot, the `Source` trait and the CPU collector

**Files:**
- Create: `crates/ghostd/src/sysroot.rs`, `crates/ghostd/src/collect/{mod.rs,cpu.rs}`, `crates/ghostd/tests/common/mod.rs`
- Test: `crates/ghostd/tests/cpu.rs`

**Interfaces:**
- Produces:
  - `SysRoot`: `new(p)`, `host()` (`/`), `read(rel) -> io::Result<String>`, `list(rel) -> io::Result<Vec<String>>`, `path(rel)`.
  - `pub trait Source<T> { fn sample(&mut self) -> T; }`
  - `parse_cpu_list("0-7,16-23")`.
  - `CpuCollector::new(&SysRoot) -> io::Result<Self>`, implementing `Source<CpuSnapshot>`.
    - It groups CPUs by `cpuN/cache/index3/shared_cpu_list`, orders the CCDs by their lowest CPU, and sets `vcache = l3_kib > min`.
    - `model` comes from `proc/cpuinfo`.
    - Thread load is `100·Δbusy/Δtotal`, where `busy = total − idle − iowait`.
    - `freq_mhz` is the mean of `scaling_cur_freq` / 1000, or 0 when it's missing.
  - `tests/common`:
    - `FakeRoot` with `write`, `remove` and `root()`;
    - `fake_9950x3d()`, which writes 32 CPU topology dirs with the real lists and sizes, plus `proc/stat` and `proc/cpuinfo`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn parses_ranges() { assert_eq!(parse_cpu_list("0-2,16-17"), vec![0, 1, 2, 16, 17]); }
#[test] fn groups_ccds_and_detects_vcache() { /* ccds[0].cpus == [0..=7,16..=23], vcache true, l3 98304; ccds[1] vcache false; model "AMD Ryzen 9 9950X3D 16-Core Processor" */ }
#[test] fn computes_thread_and_ccd_load() { /* cpu0 +50 busy +50 idle → threads[0] == 50.0; ccds[0].load == 50.0/16.0 */ }
#[test] fn missing_cpufreq_is_zero() { /* freq_mhz == 0, no panic */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test cpu`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test cpu`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/sysroot.rs crates/ghostd/src/collect crates/ghostd/tests
git commit -m "feat(ghostd): add cpu collector with ccd topology from l3 sharing"
```

---

### Task 7: Memory and top processes

**Files:**
- Create: `crates/ghostd/src/collect/mem.rs`
- Test: `crates/ghostd/tests/mem.rs`

**Interfaces:**
- Produces: `MemCollector::new(&SysRoot, top_n: usize)`, implementing `Source<MemSnapshot>`.
  - `used = MemTotal − MemAvailable`; `swap_used = SwapTotal − SwapFree`.
  - Process `cpu = 100·Δ(utime+stime) / Δ(all-cpu jiffies)`; `mem = 100·VmRSS / total`; the name comes from `comm`.
  - Processes with an RSS of 0 are excluded, and vanished ones are skipped.
  - The list is sorted by cpu descending, then pid.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn used_is_total_minus_available() { /* 96388184 − 77000000 → 19388184 */ }
#[test] fn process_cpu_is_machine_share() { /* Δ 64 of Δtotal 3200 → 2.0 */ }
#[test] fn vanished_process_skipped() { }
#[test] fn kernel_threads_excluded() { }
#[test] fn top_sorted_and_truncated() { /* 6 procs, top_n 5 → 5, descending */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test mem`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test mem`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/mem.rs crates/ghostd/tests/mem.rs
git commit -m "feat(ghostd): add memory and top process collector"
```

---

### Task 8: Temperatures and fans (hwmon)

**Files:**
- Create: `crates/ghostd/src/collect/{hwmon.rs,labels.rs}`
- Test: `crates/ghostd/tests/hwmon.rs`

**Interfaces:**
- Produces: `SensorCollector::new(&SysRoot, overrides: &BTreeMap<String,String>)`, implementing `Source<SensorSnapshot>`. It rescans every 30 samples.
  - **Ids** are `"{chip}/{label}"`, falling back to `tempN`/`fanN` when there's no label. NVMe chips use `nvme/{device/serial trimmed}`, and `spd5118` chips use `spd5118/{n}`.
  - **Built-in labels.** These are generic rules for common chips, matched by chip prefix:

    | Sensor | Label |
    |---|---|
    | `k10temp` `Tctl` | `CPU TCTL` |
    | `k10temp` `Tccd1` | `CCD0` |
    | `k10temp` `Tccd2` | `CCD1` |
    | `nct*` `SYSTIN` | `MOBO` |
    | `nct*` `fan2` | `CPU FAN` |
    | `nct*` `fan3` | `CHASSIS FAN` |
    | `r8169*` | `NIC` |
    | `spd5118/n` | `DIMM n` |
    | `nvme/*` | the drive model, uppercased |
    | anything else | `"{CHIP} {LABEL}"` |

    The `overrides` map wins over all of them.
  - **Values:** temperatures are millidegrees / 1000. Fans are listed only if they have a known label or read > 0 at the first scan. **A failed read gives `None`, and the other readings are unaffected.** Hwmons without temp or fan inputs are ignored.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn labels_and_values() { /* Tctl 57875 → ("k10temp/Tctl","CPU TCTL",57.875); fans == [CPU FAN 1670, CHASSIS FAN 450]; fan1=0 absent */ }
#[test] fn nvme_uses_serial_and_model() { /* id "nvme/2O412LA45EG7", label "XPG GAMMIX S70 BLADE" */ }
#[test] fn failed_read_is_unavailable_not_fatal() { /* temp1_input is a directory → None; k10temp still Some */ }
#[test] fn override_label_wins() { }
#[test] fn hotplug_on_rescan() { /* new hwmon after 30 samples appears */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test hwmon`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test hwmon`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/hwmon.rs crates/ghostd/src/collect/labels.rs crates/ghostd/tests/hwmon.rs
git commit -m "feat(ghostd): add hwmon temperature and fan collector"
```

---

### Task 9: GPU through NVML

**Files:**
- Create: `crates/ghostd/src/collect/gpu.rs` (unit tests inside it)

**Interfaces:**
- Produces:
  - `GpuReading` (the `GpuSnapshot` fields minus `present`) and `trait GpuSource { fn read(&mut self) -> Option<GpuReading>; }`.
  - `NvmlSource::open() -> Option<Self>` reads device 0:
    - utilisation;
    - memory, in bytes / MiB;
    - temperature;
    - `power_usage` and `enforced_power_limit` (mW / 1000);
    - `fan_speed(0)`;
    - `name`.

    Any error gives `None`.
  - `GpuCollector::new(Option<Box<dyn GpuSource>>)`, implementing `Source<GpuSnapshot>`. `present = false` when the source is missing or returns `None`.
- CLI: `ghostd probe gpu`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn maps_reading() { }
#[test] fn missing_nvml_is_absent() { assert!(!GpuCollector::new(None).sample().present); }
#[test] fn error_mid_run_degrades() { }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --lib -- gpu`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it**

- [ ] **Step 4: Run them, then try the real GPU**

Run: `cargo test -p ghostd --lib -- gpu && cargo run -q -p ghostd -- probe gpu`
Expected: PASS, then JSON containing `"name":"NVIDIA GeForce RTX 4080 SUPER"`.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/gpu.rs crates/ghostd/src/main.rs
git commit -m "feat(ghostd): add nvml gpu collector"
```

---

### Task 10: Drives (identity, state, usage, I/O)

**Files:**
- Create: `crates/ghostd/src/collect/disk.rs`
- Test: `crates/ghostd/tests/disk.rs`

**Interfaces:**
- Produces:
  - `trait FsStat { fn used_pct(&self, mount: &str) -> Option<f32>; }`, with the `StatvfsFs` implementation.
  - `DiskCollector::new(&SysRoot, Box<dyn FsStat>, labels)`, implementing `Source<DiskSnapshot>`.
  - **Which devices:** every `sys/block/*` except `loop*`, `zram*`, `ram*` and `dm-*`.
  - **Identity:**
    - `model` is `device/model`, trimmed.
    - `serial` is `device/serial`, else `device/wwid`, else the kernel name.
    - `id = "{model}#{serial}"`.
    - `label` is `labels[serial]`, else `model`.
  - **State:**
    - `Dead` when the NVMe `device/state` is `dead` or the size is 0;
    - otherwise `Live` when it's mounted (via the `proc/self/mountinfo` `major:minor`);
    - otherwise `Unmounted`.
  - **Usage and rates:**
    - `mounts` come with `/` first;
    - `used_pct` is refreshed every 60 samples;
    - `read_bps` and `write_bps` come from the `proc/diskstats` sector deltas × 512 per second.
  - **Order:** the drive mounted at `/`, then other `Live` drives, then `Dead`, then `Unmounted`. Ties break by `id`.
  - `attach_temps(&mut DiskSnapshot, &SensorSnapshot)` matches drives to `nvme/{serial}`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn dead_nvme_is_dead() { }
#[test] fn identity_survives_renumbering() { /* swap nvme0/nvme1 names → identical ordered (id, state) */ }
#[test] fn same_model_different_serial_distinct() { }
#[test] fn ordering_root_live_dead_unmounted() { }
#[test] fn io_rates_from_diskstats() { /* Δ2000 sectors over 1 s → read_bps 1_024_000 */ }
#[test] fn temps_attach_by_serial() { }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test disk`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it**

The elapsed time is injected, so the tests control it.

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test disk`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect/disk.rs crates/ghostd/tests/disk.rs
git commit -m "feat(ghostd): add drive collector keyed by model and serial"
```

---

### Task 11: History buffers and the sampler

**Files:**
- Create: `crates/ghostd/src/history.rs`, `crates/ghostd/src/sampler.rs` (unit tests inside both)

**Interfaces:**
- Produces:
  - `History`: `new(len)`, `push(key, v)`, `series() -> BTreeMap<String, Vec<f32>>` (oldest first, at most `len`), and `resize(len)`, which keeps the newest samples.
  - `Sampler`:
    - `Sampler::new(cpu, mem, sensors, gpu, disks)`, each a `Box<dyn Source<…>>`.
    - `tick(&mut self, hist: &mut History) -> Vec<ServerMsg>` returns five `Data` messages, in the order `cpu, mem, sensors, gpu, disks`, all with the same increasing `seq`.
    - Each tick pushes the history keys `load`, `ccd0`, `ccd1` and `gpu_load`, and calls `attach_temps` before building `disks`.
  - History length is `history_minutes × 60`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn tick_order_and_seq() { /* topics == [cpu, mem, sensors, gpu, disks]; seq +1 per tick */ }
#[test] fn history_caps() { /* len 3, 5 ticks → 3 newest */ }
#[test] fn resize_keeps_newest() { }
#[test] fn ccd_keys_present() { }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --lib -- sampler history`
Expected: FAIL to compile.

- [ ] **Step 3: Implement it**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --lib -- sampler history`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/history.rs crates/ghostd/src/sampler.rs
git commit -m "feat(ghostd): add sampler and history ring buffers"
```

---

### Task 12: The server, provider relay, `main`, fake mode, perf and notifications

**Files:**
- Create: `crates/ghostd/src/{server.rs,providers.rs,fake.rs,selfmon.rs,notify.rs}`
- Modify: `crates/ghostd/src/main.rs`
- Test: `crates/ghostd/tests/server.rs`

**Interfaces:**
- Consumes: everything from Tasks 2–11.
- Produces:
  - **CLI:**
    - `ghostd run [--socket PATH] [--config PATH] [--widgets-dir DIR] [--fake] [--tick-ms N]`. Defaults: the socket is `$XDG_RUNTIME_DIR/ghostd.sock`, the tick is 1000 ms, and the widgets dir is `$GHOST_WIDGETS_DIR`, else `~/.local/share/ghost-shell/widgets`.
    - `ghostd default-config`.
    - `ghostd probe <topic>`.
  - `serve(listener, hub)`. The first message decides the connection's role:
    - **UI client (`Hello`):**
      1. The server replies with `Hello`, `Config`, then `Layout`.
      2. On each `Subscribe` (which replaces the subscription set), it sends `History` for the subscribed built-ins, then the last value of each subscribed topic, then streams those topics.
      3. `Config` and `Layout` are re-sent whenever the config or the widget set changes. Watcher errors and layout `errors` go out as `ConfigError`.
    - **Provider (`Provide`):**
      - Every topic must pass `is_valid_topic`, must not be built-in, and must not be claimed by another live provider.
      - Violations get `Error { message }` and a close. The exact messages are `topic "cpu" is built in`, `topic "x.y" is already provided by "other"`, and `invalid topic name "X"`.
      - `Publish` is accepted only for declared topics, at most 64 KiB per message. The server caches the last value, and relays it to subscribers as `Data` with a per-topic `seq`.
      - When a provider disconnects, its topics are released and keep their last value.
    - **Handshake timeout:** no `Hello` or `Provide` within 2 s gives `Error { message: "expected hello or provide with proto 1" }` and a close. A wrong `proto` gets the same message.
    - **Slow UI clients:** more than 256 KiB of unsent output disconnects them.
    - **Socket file:** a stale one is replaced, and the socket is created with mode 0600.
  - `providers::supervise(specs)` starts each `ProviderSpec.exec` once. When a process with `restart = true` exits, it's restarted after 1, 2, 4… up to 30 s. Each process gets the env `GHOST_SOCKET=<path>`.
  - `selfmon` emits `perf`. The shell's pid is the peer pid (`peer_cred()`) of the UI client subscribed to `perf`.
  - `notify::send_warning(summary, body)` sends a zbus Notify with app_name `"ghost-shell"`. A failure is only logged.
  - **Fake mode:**
    - sine-based data for every built-in topic, with 2 CCDs of 16 threads;
    - one `Dead` "WD GREEN 960G" and one sensor whose value is `None`;
    - the GPU named "NVIDIA GeForce RTX 4080 SUPER".

- [ ] **Step 1: Write the failing tests** (`serve` with fake sources at `--tick-ms 50`, a temp socket and temp widget dirs)

```rust
#[tokio::test] async fn ui_handshake_then_subscribed_topics_only() { /* hello → hello, config, layout; subscribe [cpu] → history, then ≥2 cpu data lines, 0 gpu */ }
#[tokio::test] async fn wrong_proto_and_silence_get_errors() { /* proto 9 → error+EOF; silence → error within 2.5 s */ }
#[tokio::test] async fn provider_publish_reaches_subscriber() { /* P: provide ["weather.alerts"], publish {n:1}; UI subscribed → data topic weather.alerts data.n == 1 */ }
#[tokio::test] async fn late_subscriber_gets_last_value() { /* publish, then subscribe → receives the cached value immediately */ }
#[tokio::test] async fn provider_rules() { /* "cpu" → `topic "cpu" is built in`; duplicate → `already provided by`; publish undeclared → error */ }
#[tokio::test] async fn provider_crash_does_not_affect_others() { /* drop P; UI keeps receiving cpu; a new P' can claim the topic */ }
#[tokio::test] async fn reconnect_works() { /* drop UI, reconnect → full handshake again */ }
#[tokio::test] async fn config_change_resends_config_and_layout() { }
#[tokio::test] async fn supervised_provider_restarts() { /* exec ["sh","-c","exit 0"] with tick 50 ms → started ≥2 times within 3 s */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test server`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the module and wire `main`**

Use a current-thread runtime and an interval with `MissedTickBehavior::Skip`. The Task 4 watcher feeds the config. Widget dirs are rediscovered on every config reload.

- [ ] **Step 4: Run them, then do a live check**

```bash
cargo test -p ghostd --test server
cargo run -q -p ghostd -- run --socket /tmp/gs.sock --widgets-dir widgets & sleep 2
uv run python -I -c "import socket,time; s=socket.socket(socket.AF_UNIX); s.connect('/tmp/gs.sock'); s.send(b'{\"t\":\"hello\",\"proto\":1,\"topics\":[]}\n{\"t\":\"subscribe\",\"topics\":[\"cpu\",\"disks\"]}\n'); time.sleep(2.5); print(s.recv(1<<20).decode()[:3000])"; kill %1
```
Expected: PASS. The live check shows `hello`, `config`, `layout`, `history`, then real `cpu` and `disks` data, including `"state":"dead"`.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd
git commit -m "feat(ghostd): serve telemetry, layout and external providers over a unix socket"
```

---

### Task 13: Shell logic modules (Node-tested)

**Files:**
- Create: `shell/kit/GhostKit/lib/{format.mjs,segments.mjs,theme.mjs,scale.mjs,labels.mjs,layout.mjs}`
- Test: `shell/tests/*.test.mjs`

**Interfaces:**
- Produces:
  - **`format.mjs`:**
    - `pct(22.6)` → `"23%"`;
    - `tempC(null)` → `"--"`;
    - `tempShort(64)` → `"64°"`;
    - `gib(kib, dp=1)`;
    - `rate(bps)`: below 1e3 `"830 B/s"`, below 1e6 `"830.0 KB/s"`, otherwise `"12.4 MB/s"`;
    - `ghz(mhz)`, `rpm(v)`, `ddmmyy(date)`.
  - **`segments.mjs`:**
    - `cellCount(w, cell=4, gap=2)`;
    - `litCells(v, lo, hi, cells, min=0)`;
    - `level(frac, warnAt, critAt)`, using ≥ comparisons;
    - `levelLow(v, warnAt, critAt)` for batteries, using ≤ comparisons;
    - `cellColor(level, row, lit, rows, t)`.
  - **`theme.mjs`:** `TAGS` (the Global Constraints table), `tag(key, mode)`, `withAlpha(hex, pct)` → `#AARRGGBB`, and `isMonospace(w0, w1)`.
  - **`scale.mjs`:** `designUnit(logicalWidth)` = `logicalWidth / 1280`, and `pickScreen(screens, model)`. An empty `model` gives the first screen, and a screen matching nothing also falls back to the first screen; `[]` gives `null`.
  - **`labels.mjs`:** `cpuModelShort`, `gpuNameShort`, `ccdTitle`, `reading(snapshot, id)`.
  - **`layout.mjs`:** `computeRects(node, rect, gutter) → [{ path, widget, source, settings, error, x, y, w, h }]`.
    - Within a split, fixed `size` values are taken along the split's direction, and flex children share what's left equally.
    - When the fixed sizes plus gutters exceed the available space, the fixed sizes scale down proportionally and the result includes `overflow: true`.
    - `path` is like `"[0][1]"`.
    - A `tabs` node produces one rect carrying `tabs: [{ title, widget, source, settings, error }]` and `default`.

- [ ] **Step 1: Write the failing tests**

```js
test('pct rounds', () => assert.equal(pct(22.6), '23%'));
test('rate tiers', () => { assert.equal(rate(830), '830 B/s'); assert.equal(rate(830000), '830.0 KB/s'); assert.equal(rate(12400000), '12.4 MB/s'); });
test('cells and levels', () => { assert.equal(cellCount(266), 44); assert.equal(level(.75, .75, .9), 'warn'); assert.equal(level(.9, .75, .9), 'crit'); assert.equal(levelLow(20, 20, 10), 'warn'); assert.equal(levelLow(10, 20, 10), 'crit'); });
test('tags', () => { assert.equal(tag('cpu', 'mixed'), '演算'); assert.equal(tag('cpu', 'katakana'), 'コア'); assert.equal(tag('cpu', 'off'), ''); });
test('alpha', () => assert.equal(withAlpha('#0a0f11', 55), '#8C0A0F11'));
test('design unit at 1.15', () => assert.ok(Math.abs(designUnit(2560 / 1.15) - 1.739) < 0.001));
test('screen pick', () => { assert.equal(pickScreen([{ model: 'A' }, { model: 'Gigabyte M27Q' }], 'M27Q').model, 'Gigabyte M27Q'); assert.equal(pickScreen([{ model: 'A' }], '').model, 'A'); assert.equal(pickScreen([], ''), null); });
test('default v1 geometry', () => {
  const r = computeRects(DEFAULT_V1_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12);
  assert.deepEqual(pick(r, 'processor'), { x: 12, y: 42, w: 300, h: 250 });
  assert.equal(pick(r, 'storage').h, 666 - 300 - 184 - 24);
});
test('overflow shrinks instead of overlapping', () => { const r = computeRects({ direction: 'column', children: [{ widget: 'a', size: 400 }, { widget: 'b', size: 400 }] }, { x: 0, y: 0, w: 100, h: 500 }, 12); assert.ok(r.overflow); assert.ok(r[1].y >= r[0].y + r[0].h); });
test('tabs node is one rect', () => { const r = computeRects({ tabs: [{ title: 'A', widget: 'a' }, { title: 'B', widget: 'b' }], default: 1 }, { x: 0, y: 0, w: 300, h: 200 }, 12); assert.equal(r.length, 1); assert.equal(r[0].tabs.length, 2); assert.equal(r[0].default, 1); });
test('short names', () => { assert.equal(cpuModelShort('AMD Ryzen 9 9950X3D 16-Core Processor'), 'RYZEN 9 9950X3D'); assert.equal(gpuNameShort('NVIDIA GeForce RTX 4080 SUPER'), 'RTX 4080 SUPER'); assert.equal(ccdTitle({ id: 0, vcache: true }), 'CCD0 · V-CACHE'); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/`
Expected: FAIL.

- [ ] **Step 3: Implement the modules**

They're plain functions with no dependencies.

- [ ] **Step 4: Run them and watch them pass**

Run: `node --test shell/tests/`
Expected: PASS (11 tests).

- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib shell/tests
git commit -m "feat(shell): add tested formatting, segment, theme, scale and layout logic"
```

---

### Task 14: Shell skeleton (GhostKit singletons, desktop surface, dot grid, scanlines)

**Files:**
- Create:
  - `shell/shell.qml`
  - `shell/kit/GhostKit/{qmldir,Data.qml,Theme.qml,FrameClock.qml,Fmt.qml,DotGrid.qml,Scanlines.qml}`
  - `shell/kit/GhostKit/shaders/{dotgrid.frag,scanlines.frag}`
  - `shell/host/Desktop.qml`
  - `scripts/qml-smoke.sh`
  - `Makefile` (targets `shaders`, `test`, `smoke`)

**Interfaces:**
- Consumes: the Task 1 decision, the Task 12 protocol, the Task 13 modules.
- Produces:
  - **The `GhostKit` QML module.** Its `qmldir` declares `module GhostKit` and the singletons `Data`, `Theme`, `FrameClock` and `Fmt`. `qs` runs with `QML_IMPORT_PATH=<shell>/kit`, which is how third-party widgets import the same module.
  - **`Data` singleton:**
    - properties `connected`, `stale` (no message for 3 s), `config`, `layout`, `topics` (an object keyed by topic, re-assigned on every update so bindings fire), `history`, and the signal `configError(string)`;
    - the socket is `GHOST_SOCKET`, else `$XDG_RUNTIME_DIR/ghostd.sock`;
    - on connect it sends `hello` with no topics and logs `ghostd connected`;
    - on each `layout` it sends `subscribe` with the union of every slot's `requires` plus `perf`;
    - it appends to the `load`, `ccd0`, `ccd1` and `gpu_load` history and trims it to `history_minutes × 60`;
    - it reconnects after 1 s, doubling to at most 10 s.
  - **`Theme` singleton:**
    - `u` and `d(px)`;
    - the colour tokens, `panelFill`, `font`, `fontCjk` and `japanese`;
    - `loadWarn` and `loadCrit` (fractions), `tempWarn` and `tempCrit`, `batteryWarn` and `batteryCrit`;
    - `tag(key)`;
    - Global Constraints defaults until a config arrives.
  - **`FrameClock` singleton:** `frame` at the Task 1 rate (default 15 fps), `blinkOpacity` from `[1, .5, 0, .5]` on a 250 ms timer, and `acquire()`/`release()` with a refcount so the timers stop when unused.
  - **`Fmt` singleton:** a thin wrapper re-exporting `format.mjs` and `labels.mjs`, so widgets don't import files by path.
  - **`Desktop`:**
    - it tracks `Data.config.dashboard.default_view` as `currentView`; the view shortcuts arrive in Plan 2;
    - a `PanelWindow` on `pickScreen(Quickshell.screens, Data.config.display.model)`, on the Task 1 layer, with namespace `"ghost-shell-desktop"`, `ExclusionMode.Ignore`, all four anchors and colour `Theme.ground`;
    - it sets `Theme.u = width / 1280` (`GHOST_FORCE_U` overrides this);
    - it stacks `DotGrid`, a `Loader` slot for the view (Task 16), `Scanlines`, and a perf overlay when `GHOST_DEBUG=1`;
    - other screens get a window with only `DotGrid`.
  - **Shaders:** `dotgrid.frag` (`vec4 dotColor; float spacing; float radius;`) and `scanlines.frag` (`float strength; float period; float lineWidth;`), in design px × `u`.
  - **Stale rule:** while `!Data.connected || Data.stale`, kit components show values in `dim` and meters with nothing lit.

- [ ] **Step 1: Write the smoke script and watch it fail**

`scripts/qml-smoke.sh`:
1. Runs `cargo build -q -p ghostd`.
2. Starts `target/debug/ghostd run --fake --socket /tmp/ghost-smoke.sock --widgets-dir widgets` and `GHOST_SOCKET=/tmp/ghost-smoke.sock QML_IMPORT_PATH=shell/kit qs -p shell`, logging to `/tmp/ghost-smoke.log`.
3. Waits 6 s.
4. Kills ghostd, waits 3 s, restarts it, and waits 5 s.
5. Stops both.
6. **Fails** if the log matches `ReferenceError|TypeError|is not defined|Cannot assign|Error:|failed to load|Unable to`.
7. **Fails** unless `ghostd connected` appears at least twice.

Run: `make shaders && scripts/qml-smoke.sh`
Expected: FAIL (`shell.qml` is missing).

- [ ] **Step 2: Implement the module, the singletons, the shaders, Desktop and `shell.qml`**

- [ ] **Step 3: Run the smoke test and the logic tests**

Run: `make shaders && scripts/qml-smoke.sh && node --test shell/tests/`
Expected: `smoke: ok (2 connects, 0 errors)` and PASS.

- [ ] **Step 4: Check it visually**

Take a screenshot with `spectacle -b -n -f -o /tmp/gs-task14.png` and look at it. Expected:
- a `#0a0f11` screen with faint teal dots on the 24-px grid;
- faint scanlines;
- windows draw above it.

- [ ] **Step 5: Commit**

```bash
git add shell Makefile scripts/qml-smoke.sh
git commit -m "feat(shell): add GhostKit singletons and the desktop surface"
```

---

### Task 15: GhostKit components

**Files:**
- Create: `shell/kit/GhostKit/{Panel.qml,SegMeter.qml,BlockGraph.qml,AlertBox.qml,Digits.qml,Kv.qml}`, `shell/kit/GhostKit/shaders/stripes.frag`, `shell/host/Gallery.qml`
- Modify: `shell/kit/GhostKit/qmldir` (export the components), `shell/host/Desktop.qml` (`GHOST_VIEW=gallery` loads `Gallery`)

**Interfaces:**
- Consumes: Theme, FrameClock, the `segments.mjs` and `theme.mjs` modules.
- Produces (the public building blocks of the widget API):
  - **`Panel { label; tagKey; meta; default property alias content; padding: Theme.d(12) }`**
    - The background is a `Shape` polygon with the 12 px chamfers on the top-right and bottom-left, filled with `Theme.panel` at `panelFill`.
    - The frame is a 1 px `hair` edge plus 1.5 px signal brackets with 14 px arms on the top-left and bottom-right.
    - The header is 26 high: the square, the label, the tag and the meta.
    - All text elides on the right.
  - **`SegMeter { value (0..1); warnAt; critAt; low: false }`**: 7 high. When `low` is true, it uses `levelLow` (for batteries).
  - **`BlockGraph { values; lo: 0; hi: 100; cols: 0; cellW: 3; cellH: 2; gap: 1; warnAt; critAt; minLit: 1 }`**: the newest value on the right, built from Repeaters of `Rectangle`, and repainted only when `values` changes.
  - **`AlertBox { text; trailing; level: "warn" | "crit"; chipKey }`**
    - It has a 3 px edge, and its stripes come from `stripes.frag`, phased by `FrameClock.frame` at one 16 px tile per 1.6 s.
    - The text runs the stepped arrival (`0%:.3 6%:1 10%:.46 14%:1 40%:.82 70%:.64`, 1.6 s), then binds to `FrameClock.blinkOpacity`.
    - It holds `FrameClock.acquire()` while visible.
  - **`Digits { text; pixelSize; weight }`**: a single `Text` when the font is monospace. Otherwise each digit sits in a cell as wide as "0".
  - **`Kv { key; value }`**: the key in dim at 8.5 px with tracking .14 em, above the value in `hi` at 10 px.

- [ ] **Step 1: Write the gallery and watch the smoke test fail**

The gallery shows:
- meters at 0.2, 0.8 and 0.95, and a `low` meter at 15;
- a `BlockGraph` ramp;
- a warn AlertBox and a crit AlertBox;
- `Digits` "20:07";
- three Panels.

Run: `GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: FAIL.

- [ ] **Step 2: Implement the components and the stripe shader**

- [ ] **Step 3: Run the smoke test again**

Run: `make shaders && GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: `smoke: ok`.

- [ ] **Step 4: Compare it with the mockup and measure the animation cost**

Take a screenshot and compare it with the reference mockup. Check:
- the corners and brackets are in the right places;
- the meters are teal, amber and red, and the battery meter at 15 is amber;
- the stripes move right, and the text blinks in held steps;
- there is no glow.

Then measure for 60 s with the gallery up.
Expected: `cpu_pct` ≤ 1.50.

- [ ] **Step 5: Commit**

```bash
git add shell/kit shell/host
git commit -m "feat(shell): add GhostKit panel, meter, graph, alert and digit components"
```

---

### Task 16: Widget host, layout view and the first built-in widgets

**Files:**
- Create:
  - `shell/host/{LayoutView.qml,WidgetHost.qml,TabsHost.qml,ErrorCard.qml}`
  - `widgets/placeholder/{manifest.toml,Widget.qml}`
  - `widgets/processor/{manifest.toml,Widget.qml}`
  - `widgets/graphics/{manifest.toml,Widget.qml}`
  - `widgets/memory/{manifest.toml,Widget.qml}`
  - `widgets/broken-example/{manifest.toml,Widget.qml}`. This is a deliberately broken test fixture, excluded from installs.
- Modify: `shell/host/Desktop.qml` (load `LayoutView` for `Data.layout.views[Data.config.dashboard.default_view]`)

**Interfaces:**
- Consumes:
  - `Data.layout` (Task 12);
  - `computeRects` (Task 13);
  - GhostKit (Tasks 14–15);
  - the topics `cpu`, `gpu`, `mem` and `sensors`, and the history keys;
  - the sensor ids `k10temp/Tctl`, `k10temp/Tccd1`, `k10temp/Tccd2` and `nct6799/fan2`.
- Produces:
  - **`LayoutView { node }`**: calls `computeRects(node, { x: margin, y: margin + 30, w: 1280 − 2·margin, h: 720 − 2·margin − 30 }, gutter)` and places one `WidgetHost` per rect, in `Theme.d()` units.
  - **`WidgetHost { slot }`**:
    - When `slot.error` is set, it shows `ErrorCard { widgetId; message }`: a Panel whose label is the widget id, containing a crit `AlertBox` with the message.
    - Otherwise it calls `Qt.createComponent(slot.source)`.
      - On `Component.Error`, it logs `widget "<id>" failed to load: <first line of errorString()>` and shows an `ErrorCard` with that text.
      - On success, it calls `createObject(host, { settings: slot.settings })`.
  - **`TabsHost { node }`**:
    - a 26-high tab strip in the Panel header style; the selected tab has signal fill and dark text, as in the mockup;
    - the body hosts **only the active tab's** `WidgetHost`, so inactive widgets cost nothing;
    - clicking a tab switches it, and the selection is kept per view for the session;
    - `LayoutView` uses `TabsHost` for rects that carry `tabs`.
  - **The widget contract**, which `docs/widgets.md` (Plan 4) will document:
    - the root item fills its parent;
    - it declares `property var settings`;
    - it imports `GhostKit`;
    - it reads data only through `Data.topics[...]` and `Data.history[...]`;
    - it animates only through `FrameClock`.
  - **The manifests:**

    | Widget | `kinds` | `requires` | Settings |
    |---|---|---|---|
    | `placeholder` | `["panel"]` | none | `label` string, default `"Empty"` |
    | `processor` | `["panel"]` | `cpu`, `sensors` | none |
    | `graphics` | `["panel"]` | `gpu` | none |
    | `memory` | `["panel"]` | `mem` | `top` int, default 6 |

    All of them have `api = 1`.
  - **Widget content** (spec §6.3):
    - **Processor:**
      - meta `cpuModelShort`;
      - two CCD groups, each a `ccdTitle` with its temperature above a `BlockGraph` (cols 16, cellH 3, gap 2, warn .75, crit .9). **The groups stretch.**
      - a load-history `BlockGraph` 40 high;
      - a `Kv` row: LOAD, TCTL, FREQ, CPU FAN.
    - **Graphics:**
      - meta `gpuNameShort`;
      - a big `Digits` load %;
      - `SegMeter` rows for VRAM, TEMP (temp thresholds), POWER (`power_w / power_limit_w`) and FAN;
      - the `gpu_load` history, **which stretches**;
      - everything shows `--` when `present` is false.
    - **Memory:**
      - meta `gib(total) + " GIB"`;
      - a big used GiB with `/ total`;
      - RAM and SWAP meters;
      - the top processes, **which stretch and clip whole rows**.

- [ ] **Step 1: Write the broken fixture and a failing smoke variant**

`widgets/broken-example/Widget.qml` holds deliberately invalid QML.

A test config at `/tmp/ghost-layout-test.toml` sets `layout.views.v1` to a row of:
- the slots `processor`, `memory`, `broken-example` and `not-installed`;
- a `tabs` node holding `graphics` and `processor` (`default = 0`).

Running `GHOST_CONFIG=/tmp/ghost-layout-test.toml scripts/qml-smoke.sh` (the script passes `--config "$GHOST_CONFIG"`) must:
- log `widget "broken-example" failed to load` and `no widget named "not-installed" is installed`;
- not match any other error pattern.

The smoke script whitelists exactly these two messages.

Run it.
Expected: FAIL (there's no host yet).

- [ ] **Step 2: Implement the host, the error card and the four widgets**

- [ ] **Step 3: Run the smoke test with the default and the test layouts**

Run: `make shaders && scripts/qml-smoke.sh && GHOST_CONFIG=/tmp/ghost-layout-test.toml scripts/qml-smoke.sh`
Expected: `smoke: ok` twice.

- [ ] **Step 4: Check it visually**

Take a screenshot with the test layout. Expected:
- the live panels, two red error cards with their messages, and a tab strip showing GRAPHICS selected;
- clicking PROCESSOR swaps the tab body;
- nothing else is affected.

- [ ] **Step 5: Commit**

```bash
git add shell/host widgets
git commit -m "feat(shell): add widget host, tabs host and error cards with processor, graphics, memory widgets"
```

---

### Task 17: Storage and Sensors widgets, and the default V1

**Files:**
- Create: `widgets/storage/{manifest.toml,Widget.qml}`, `widgets/sensors/{manifest.toml,Widget.qml}`

**Interfaces:**
- Consumes: the host (Task 16) and the topics `disks` and `sensors`.
- Produces:
  - **`storage`:** `requires = ["disks"]`, settings `show_unmounted` bool (default true).
    - `Live` drives get two lines: `label · mounts` with the temperature, then a `SegMeter` of used % (warn .85, crit .95) with the percentage.
    - `Dead` drives get `AlertBox { level: "crit"; chipKey: "crit"; text: label + " · " + kernel_name; trailing: "DEAD" }`.
    - `Unmounted` drives get a dim line ending in `UNMOUNTED`.
    - A footer shows R/W totals.
  - **`sensors`:** `requires = ["sensors"]`, no settings.
    - Temperatures, with level colours from the temperature thresholds (or the per-sensor overrides), then fans.
    - Unavailable values show `--` in dim.
  - The default V1 from Task 3 now renders fully, with `placeholder`s standing in for the Plan 2 widgets.

- [ ] **Step 1: Watch the smoke test fail**

Run: `scripts/qml-smoke.sh`
Expected: FAIL, because the default layout references `storage` and `sensors`, which are now missing. The error cards appear in the log.

- [ ] **Step 2: Implement both widgets**

- [ ] **Step 3: Run the smoke test against fake and real data**

Run: `scripts/qml-smoke.sh`, then repeat against the real `ghostd run`.
Expected: `smoke: ok` both times.

- [ ] **Step 4: Check it visually at two scales**

Take screenshots with the real daemon at the native `u` and at `GHOST_FORCE_U=1.0`. Check:
- the V1 geometry matches the mockup;
- the WD Green blinks as a red alert line, in held steps;
- the unmounted drives are dim;
- the dead NVMe's sensor row shows `--`;
- nothing overflows.

- [ ] **Step 5: Commit**

```bash
git add widgets/storage widgets/sensors
git commit -m "feat(widgets): add storage and sensors widgets completing the default v1"
```

---

### Task 18: Install, run as user services, and check the acceptance criteria

**Files:**
- Create: `packaging/systemd/{ghostd.service,ghost-shell.service}`, `scripts/install.sh`, `README.md`, `docs/superpowers/notes/2026-10-20-plan1-acceptance.md`
- Modify: `Makefile` (add `release` and `install`)

**Interfaces:**
- Produces:
  - **`scripts/install.sh`** (idempotent):
    1. `cargo build --release`.
    2. Install `ghostd` to `~/.local/bin/`.
    3. Copy `shell/` to `~/.local/share/ghost-shell/shell/`, with the compiled `.qsb` files.
    4. Copy `widgets/` to `~/.local/share/ghost-shell/widgets/`, excluding `broken-example`.
    5. If Kode Mono is missing, download `KodeMono[ROND,wdth,wght].ttf` and `OFL.txt` from `https://github.com/google/fonts/raw/main/ofl/kodemono/` into `~/.local/share/fonts/ghost-shell/`, then run `fc-cache -f`.
    6. Install the units and run `systemctl --user daemon-reload`.

    It doesn't enable anything at login, and it doesn't touch Plasma, gridlock or conky (Plan 4).
  - **`ghostd.service`:** `ExecStart=%h/.local/bin/ghostd run`, `Restart=on-failure`, `PartOf=graphical-session.target`.
  - **`ghost-shell.service`:** `Environment=QML_IMPORT_PATH=%h/.local/share/ghost-shell/shell/kit`, `ExecStart=/usr/bin/qs -p %h/.local/share/ghost-shell/shell`, `Wants=ghostd.service`, `After=ghostd.service`, `Restart=on-failure`, `PartOf=graphical-session.target`.
  - **`README.md`:** build, test, install, run, the config location (`ghostd default-config > ~/.config/ghost-shell/config.toml`), and a short "write your own widget" stub that points at `widgets/placeholder` as the smallest example.

- [ ] **Step 1: Install and start it**

**Ask the user before stopping gridlock for this session.**

Run: `scripts/install.sh && systemctl --user start ghostd ghost-shell && sleep 5 && systemctl --user is-active ghostd ghost-shell`
Expected: `active` twice. V1 appears on the desktop.

- [ ] **Step 2: Measure the budgets**

Run `scripts/measure-cpu.sh` for 60 s on each pid.
Expected: a combined `cpu_pct` ≤ 2.00 (the dead-drive alert animates) and a combined RSS ≤ 307200 KiB.

- [ ] **Step 3: Check the live config and the layout**

1. Write `~/.config/ghost-shell/config.toml` from `ghostd default-config`, and add this machine's specifics: `display.model = "M27Q"`.
2. Set `panel_fill = 30` and save.
   Expected: the panels become more transparent within 1 s.
3. Set `warn = "orange"`.
   Expected: a notification reading `appearance.colors.warn: expected #rrggbb, got "orange"`, while the colours stay unchanged.
4. Swap the `storage` and `sensors` slots in `layout.views.v1`.
   Expected: they move on save.
5. Restore everything.

- [ ] **Step 4: Write the acceptance note**

Record the measurements, screenshots and deviations. Stop the services, and restart gridlock if the user wants it back.

- [ ] **Step 5: Commit**

```bash
git add packaging scripts/install.sh README.md Makefile docs/superpowers/notes/2026-10-20-plan1-acceptance.md
git commit -m "chore: add install script, user services and plan 1 acceptance notes"
```

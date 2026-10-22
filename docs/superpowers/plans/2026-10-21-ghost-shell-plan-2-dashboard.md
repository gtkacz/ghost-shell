# ghost-shell Plan 2 — Dashboard Implementation Plan (Part 2a: views, occlusion, time, weather, media)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** By the end of Part 2a:
- the dashboard has its three default views (V1, V2, MONITOR), switched by desktop-only shortcuts with the CRT print transition;
- a covered dashboard renders zero frames and the daemon collects only subscribed topics;
- the clock, weather (with INMET alerts, forecast, sun, moon, AQI) and media (MPRIS, idle states, spectrum, audio rows, MIC LIVE) widgets replace their placeholders;
- every new collector states its cadence and measured cost, and the three §15 budget states are measured.

**Architecture:**
- **Daemon (`ghostd`):**
  - built-in collectors run only for topics some UI client subscribes to;
  - a new kind of built-in source, the **feed**, is an async task that publishes a topic on demand: `host`, `desktop`, `weather`, `sky`, `audio.spectrum`;
  - a session-bus service receives coverage and focus reports from a KWin script, registers the view shortcuts with KGlobalAccel, and publishes view actions on the `desktop` topic.
- **Alert provider:** `ghost-alerts-inmet` is a separate binary that speaks the open provider protocol and publishes `weather.alerts`. ghostd starts it from `location.alerts`.
- **KWin script (`ghost-shell-desktop`):** keeps the dashboard above Plasma's desktop window (the R46 keeper), and reports which outputs are fully covered and whether no window has focus.
- **Shell:**
  - `Data` notifies per topic and subscribes on demand;
  - a `Views` singleton owns view state;
  - `PrintSwap` runs the 24-step transition;
  - the desktop unloads its view and suspends the `FrameClock` while covered.
- **Widgets:** media and audio use Quickshell's own MPRIS and PipeWire services, as spec §13 allows; everything else reads ghostd topics.

**Tech Stack:**
- Rust 1.98. New crates:
  - `ureq` 3 (rustls) for the two HTTPS clients;
  - `jiff` for INMET's Brasília times;
  - `pipewire` (bindings; builds against `pipewire-devel` and `clang-devel`, both installed here) and `realfft` for the spectrum;
  - zbus with the features its `#[interface]`/`#[proxy]` macros need.
- Quickshell 0.2.1 on Qt 6.11:
  - `Quickshell.Services.Mpris`;
  - `Quickshell.Services.Pipewire` (`PwNodePeakMonitor`, `PwObjectTracker`);
  - `Quickshell.DesktopEntries`, `Quickshell.processId`;
  - `Quickshell.Io.FileView`;
  - QtQuick.Shapes, ShaderEffect (`qsb-qt6`).
- KWin 6 JavaScript scripting.
- Node 24 (`node --test shell/tests/*.test.mjs`).
- `dbus-run-session` and `busctl` for private-bus tests.

**Spec:** `docs/superpowers/specs/2026-10-20-ghost-shell-design.md`: §6 dashboard, §5 visual language, §12 data sources, §13 extensibility, §15 performance. The reference mockup `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html` is the visual truth. Plan 1, `docs/superpowers/plans/2026-10-20-ghost-shell-plan-1-foundation.md`, and its ledger rulings (`.superpowers/sdd/2026-10-20-ghost-shell-plan-1-foundation/progress.md`, R1–R46) describe what Plan 1 actually shipped. Read them with this plan.

**Prerequisite:** Plan 1 is complete (Tasks 14–18 merged). Where this plan names Plan 1 shell interfaces (`Data`, `Theme`, `FrameClock`, `Panel`, `SegMeter`, `BlockGraph`, `AlertBox`, `Digits`, `Kv`, `LayoutView`, `WidgetHost`, `TabsHost`, `Gallery`, `scripts/qml-smoke.sh`), it uses the names that Plan 1's task text and rulings R3, R4, R8, R9, R42, R43, R45 and R46 define.

## Scope and assumptions

Plan 1's roadmap gives Plan 2 four things:
- the remaining panels (network, uplink, silicon, process tree, media, weather, clock);
- the V2 and MONITOR views, print transitions and desktop-only shortcuts;
- text scramble and the Japanese chips;
- the greeting.

The Plan 1 ledger adds two more: occlusion detection (R10, R46 and the user's 2026-10-21 answer) and the default views. Together that is too much for one plan, so Plan 2 is split into three independently shippable parts:

| Part | Content | In this document |
|---|---|---|
| **2a** | Views and transitions, desktop-only shortcuts, occlusion, demand-driven collection, chips and scramble, clock, weather + INMET provider + sun/moon/AQI, media + spectrum + audio rows + MIC LIVE, default V2/MONITOR | full task list (18 tasks) |
| **2b** | Network, MONITOR telemetry lanes, process table and process tree, uplink (sock_diag, offline GeoIP, Natural Earth globe), silicon + RAPL package power, drive wear and power-on hours, device batteries | outline |
| **2c** | The greeting (login, unlock, wake) | outline |

Assumptions, each marked **[spec]** (derived from the spec or Plan 1) or **[assumption]** (mine, for the user to confirm or override):

1. **[spec]** The dashboard **Notifications** list belongs to **Plan 3**, with the notification server. Spec §11 makes ghost-shell the `org.freedesktop.Notifications` server, and the list is that server's history (read rules, bell count). Plan 1's roadmap also puts notifications in Plan 3. V1 and V2 keep a `placeholder {label="Notifications"}` until then. The INMET and dead-drive *notifications* (§11, built-in sources) also go to Plan 3.
2. **[spec]** The media and audio widgets use Quickshell's MPRIS and PipeWire services directly, which §13 allows ("Widgets may also use Quickshell's own services"). The spectrum still needs an FFT, which Quickshell lacks, so ghostd computes it (spec §12: "cava-style FFT [call]").
3. **[spec]** Built-in topics come from ghostd. The forecast (Open-Meteo) is built in (§4, §12). Weather alerts are an external provider (§13).
4. **[decided 2026-10-21]** The view shortcuts are registered with KGlobalAccel by ghostd, so `shortcuts.*` in config.toml stays authoritative and reloads live. The alternative is KWin's `registerShortcut`, where config.toml only supplies a default and System Settings wins after the first change. The "on the desktop" test comes from the KWin script's focus report. §18 leaves both to the builder.
5. **[assumption]** "The daemon collects only what the bar shows" (§15) is implemented as "the daemon samples only subscribed topics". While covered, the shell subscribes only to `desktop`, `perf` and the bar's topics. The bar arrives in Plan 3, so in 2a "the bar's topics" is the empty set.
6. **[assumption]** If Plan 1's Task 18 ships the R46 keeper as its own KWin script, Task 4 here extends that script rather than creating a second one. The path below is the one this plan assumes.
7. **[spec]** The ledger rulings stand:
   - R8/R46: layer Bottom, namespace `desktop`;
   - R9: one 67 ms `FrameClock` and no QML Animation types;
   - R30/P10: collectors run off the reactor;
   - R40/R41: process list every 2 samples, GPU fan every 5;
   - R42: smoke tests never touch the user's desktop session;
   - R3: visual checks use `GHOST_SNAPSHOT`, never spectacle;
   - R6: never maximise windows or send Meta+D on the user's live session; such checks are user steps.
8. **[spec]** The default views reproduce §6.2. Widgets that arrive in 2b (uplink, silicon, process tree, network, telemetry lanes) stay `placeholder`s in the 2a defaults, the same way Plan 1 handled its gaps.

## Global Constraints

Every constraint of Plan 1 that is restated here still applies. Plan 2 adds the rest.

- **Pluggable by contract:**
  - Built-in widgets use only the public contract: `manifest.toml`, `Widget.qml`, optional helper files in the widget's own folder, the `GhostKit` imports and an injected `settings` object.
  - Data is read through `Data.topic(name).value`. Topics a widget needs only sometimes are requested with `Data.want(name)` / `Data.release(name)` (Task 5).
  - Placement comes only from `layout.*`. Views are named layouts.
  - Topic names match `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`, ≤ 64 characters. Widget API version is `1`.
- **Design units:** mockup px at 1280×720 × `u = surfaceLogicalWidth / 1280`. Never hard-code the scale. Rects snap to whole pixels.
- **Palette defaults:**
  - `ground #0a0f11`, `panel #0a0f11` at 55 %, `hair #2a3a3d`, `hair2 #1a2527`;
  - `text #b7d1cd`, `hi #e4f1ef`, `dim #587170`;
  - `signal #a3d4cc`, `warn #e9a84c`, `crit #ff3b45`;
  - `dot #a3d4cc` at 16 % on a 24 px grid;
  - scanlines: a 1 px line every 3 px at 15 % black.
- **Colour only for attention:**
  - loads ≥ 75 % warn and ≥ 90 % crit;
  - temperatures ≥ 80 °C warn and ≥ 90 °C crit;
  - batteries ≤ 20 % warn and ≤ 10 % crit;
  - **US AQI > 100 warn and > 150 crit (strictly greater, spec §5.2)**;
  - a dead drive is crit;
  - **INMET *Perigo* and *Grande Perigo* are crit, *Perigo Potencial* is warn**;
  - **a microphone in use is warn**.
- **Panels:**
  - 12 px chamfers on the top-right and bottom-left corners;
  - 1.5 px signal brackets with 14 px arms on the top-left and bottom-right corners;
  - a 1 px `hair` edge;
  - a header with a 5 px square, an UPPERCASE 10 px label (weight 600, tracking .16 em), the Japanese tag and `dim` meta.
  - The mockup also has headerless panels (V2 time band, MONITOR band) and frameless ones (V1 time hero).
- **Segmented bars:**
  - meter cells 4 px wide with 2 px gaps, 7 px tall;
  - the **thin** progress meter is 4 px tall (mockup `.sg.thin`);
  - graphs are columns of cells;
  - unlit cells are signal at 6 %.
- **Layout:** gutter 12, margin 12, a 30 px band reserved for the bar (Plan 3).
- **Font:** `appearance.font`, default "Kode Mono"; CJK fallback "Noto Sans CJK JP". Numbers never shift (`Digits`).
- **Japanese** (mode `kanji | katakana | mixed | off`, default `mixed`). Panel tags are kanji in `mixed`; **chips are katakana in `mixed`** (spec §5.3). `off` hides both.

  Panel tags, from the mockup's `JP` table. The rejected CORE reactor's `core` key is omitted.

  | Key | Kanji | Katakana |
  |---|---|---|
  | cpu | 演算 | コア |
  | gpu | 描画 | グラフィック |
  | mem | 記憶 | メモリ |
  | disk | 記録 | ストレージ |
  | sens | 温度 | センサー |
  | clock | 時刻 | タイム |
  | wx | 気象 | ウェザー |
  | media | 音声 | シグナル |
  | notif | 通知 | ログ |
  | net | 通信 | ネット |
  | scope | 衛星 | アップリンク |
  | die | 回路 | シリコン |
  | tree | 系譜 | プロセス |
  | telem | 遠隔 | テレメトリ |
  | switch | 窓切替 | スイッチ |

  Chips, from the mockup's `CHIPS` table, plus `pause` and `stop` (see Decisions):

  | Key | Kanji | Katakana |
  |---|---|---|
  | main | 主画面 | メイン |
  | mon | 監視 | モニター |
  | live | 実況 | ライブ |
  | online | 接続 | オンライン |
  | play | 再生 | プレイ |
  | alert | 警報 | アラート |
  | crit | 危険 | クリティカル |
  | pause | 一時停止 | ポーズ |
  | stop | 停止 | ストップ |

- **Never:**
  - glow or bloom (the mockup greeting's `box-shadow`/`text-shadow` are dropped);
  - glitch effects;
  - smooth fades on alert text;
  - Unicode glyphs as icons.
  - Every arrow-type icon is a chevron: about 6.8 × 3 in a 12-unit box, drawn at 0.8 em with a 1.6-unit square-capped stroke.
- **Boxed alert:**
  - a 3 px left edge;
  - a 16 px stripe tile moving one tile per 1.6 s, phased by elapsed time, not frame count;
  - a 1.6 s held-frame arrival flicker, then the cursor blink `1, .5, 0, .5` at 250 ms steps.
- **Text scramble:** only the first time a text is written: panel labels, a new track title, a newly arrived notification (Plan 3). **Never** on numbers, the clock or updating values.
- **Countdowns:** only a live `HH:MM:SS` to the start, then to the end.
- **Motion:**
  - One `FrameClock` Timer, at 67 ms; at 33 ms only while a visible spectrum is playing (spike decision 3).
  - No QML Animation types.
  - Data widgets repaint only on new data.
  - Transient sequences (the view print) may run a step Timer **only while the sequence runs** (≤ 640 ms per user action).
  - Spike figure for the budget arithmetic: one rendered frame/s costs about 0.035 % of a core, so a 30 fps spectrum costs about 1 % by itself.
- **Budgets (spec §15), shell + daemon, 60 s average:**
  - animating: ≤ 2 % of one core and ≤ 300 MB RSS;
  - still: ≤ 1 %;
  - **covered: zero dashboard frames and ≤ 0.5 %**.
  - The daemon now idles at ≈ 0.31 % with every collector running (README).
- **Daemon behaviour:**
  - no forks, scripts or busy polling in steady state, except starting each provider once;
  - built-in collectors and feeds run only while their topic is subscribed;
  - blocking reads and HTTP run on blocking threads;
  - **every new collector or feed states its cadence and is measured (wall and on-CPU per run) in its task, and the controller records the figures in README's Performance section**;
  - a broken source degrades one row.
- **Microphone:** the shell opens a capture stream on the microphone only while another app is recording (for the IN level), never otherwise (decision 2).
- **Network access** is limited to Open-Meteo forecast and air quality (ghostd) and INMET (the provider). Tests never touch the network.
- **D-Bus in tests:** zbus peer-to-peer connections, or `dbus-run-session`. Never the user's session bus.
- **Formats:** dates `DD.MM.YY`, 24-hour time, °C, weekdays `SUN…SAT`.
- **Config:** one file, reloaded live; the last good config is kept on error. New keys are validated with messages that name the key.
- **Commits:**
  - Conventional Commits with no trailers;
  - before each code commit run, for the touched crate, `cargo test -p <crate>`, `cargo fmt --check` and `cargo clippy -p <crate> -- -D warnings`, or `node --test shell/tests/*.test.mjs` for shell logic (R7);
  - stage by explicit path.

## Review Focus

1. **A window that only seems to cover the dashboard:**
   - a translucent window (opacity < 1), a minimised one, or one on another desktop or activity → not covered;
   - two windows leaving a 1 px gap → not covered;
   - two half-screen windows together → covered;
   - Show Desktop → uncovered, and counts as on the desktop.

   *Tests: Task 4.*
2. **A shortcut pressed in the wrong context:** with a window focused, during a transition, or with `monitor_view = ""`, nothing happens. A shell that connects after an action never replays it. *Tests: Tasks 3 and 7.*
3. **Offline, failing APIs, no location, a dead provider:**
   - weather keeps its last value and dims once stale;
   - with no location the weather UI is hidden;
   - an alert past its end disappears even if the provider died.

   *Tests: Tasks 9 and 12.*
4. **A player that quits mid-track, several players, a stream without a length:** media shows `STOPPED · LAST PLAYED …` with the remembered track; progress is hidden without a length; play reopens the player. *Tests: Task 15.*
5. **ghost-shell's own capture streams** (the spectrum's sink monitor, the mic peak meter) never raise MIC LIVE. The mic peak meter exists only while another app records, so the shell never keeps itself live. A muted mic with an app recording still reads LIVE. *Tests: Task 16.*

---

### Task 1: Chips, scramble, countdown and strict-level logic

**Files:**
- Modify: `shell/kit/GhostKit/lib/theme.mjs`, `shell/kit/GhostKit/lib/format.mjs`, `shell/kit/GhostKit/lib/segments.mjs`
- Create: `shell/kit/GhostKit/lib/scramble.mjs`
- Test: `shell/tests/theme.test.mjs`, `shell/tests/format.test.mjs`, `shell/tests/segments.test.mjs` (extend), `shell/tests/scramble.test.mjs` (new)

**Interfaces:**
- Consumes: the Plan 1 Task 13 modules.
- Produces:
  - **`theme.mjs`:** `TAGS` gains every tag key in the Global Constraints table. `CHIPS` holds the chip table. `chip(key, mode)` returns:
    - kanji for `kanji`;
    - katakana for `katakana` and `mixed`;
    - `''` for `off` or an unknown key.

    `TAGS.crit` stays until Task 6 moves `AlertBox` to chips.
  - **`format.mjs`:**
    - `countdown(nowMs, startMs, endMs)` counts `HH:MM:SS` to the start while `now < start`, then to the end, and clamps at `00:00:00`. Hours print every digit past 99.
    - `uptime(seconds)` uses the same `HH:MM:SS` shape. Hours are not wrapped at 24, as in the mockup.
    - `ago(seconds)` returns `NOW` below 60, then `<n>M`, `<n>H`, `<n>D` (floored).
    - `utcOffset(minutesEast)` returns `UTC−3` (with U+2212), `UTC+5:30` or `UTC+0`.
    - `dow(date)` returns `SUN…SAT`.
  - **`segments.mjs`:** `levelAbove(value, warnAt, critAt)` uses strict `>` and returns `'ok'` for non-finite values. It is used for AQI.
  - **`scramble.mjs`:** `SCRAMBLE_CHARS = '!<>-_/[]{}=+*^?#01ｱｲｳｴｵｶｷｸｹｺｻｼｽ'` and `scrambleFrame(target, progress, rand)`. Character `i` of `n` settles at `s = .3 + .7·i/n`. It is:
    - the target character when `progress ≥ s`;
    - a random `SCRAMBLE_CHARS` character (a space stays a space) when `progress > s − .45`;
    - otherwise a space.

    This is the mockup's `scramble`. `rand` is injected so tests are deterministic.

- [ ] **Step 1: Write the failing tests**

```js
test('chips are katakana in mixed', () => { assert.equal(chip('live', 'mixed'), 'ライブ'); assert.equal(chip('live', 'kanji'), '実況'); assert.equal(chip('crit', 'katakana'), 'クリティカル'); assert.equal(chip('alert', 'off'), ''); assert.equal(chip('nope', 'mixed'), ''); });
test('new tags', () => { assert.equal(tag('clock', 'mixed'), '時刻'); assert.equal(tag('wx', 'katakana'), 'ウェザー'); assert.equal(tag('core', 'mixed'), ''); });
test('scramble frames', () => { const r = () => 0; assert.equal(scrambleFrame('AB', 0, r), '! '); assert.equal(scrambleFrame('AB', .5, r), 'A!'); assert.equal(scrambleFrame('AB', 1, r), 'AB'); assert.equal(scrambleFrame('A B', .5, r), 'A !'); });
test('countdown to start then end', () => { assert.equal(countdown(0, 3661000, 7200000), '01:01:01'); assert.equal(countdown(3661000, 3661000, 7200000), '00:58:59'); assert.equal(countdown(9e6, 0, 7200000), '00:00:00'); assert.equal(countdown(0, 360000000, 4e8), '100:00:00'); });
test('ago buckets', () => { assert.equal(ago(59), 'NOW'); assert.equal(ago(60), '1M'); assert.equal(ago(3599), '59M'); assert.equal(ago(3600), '1H'); assert.equal(ago(86400), '1D'); });
test('utc offsets', () => { assert.equal(utcOffset(-180), 'UTC−3'); assert.equal(utcOffset(330), 'UTC+5:30'); assert.equal(utcOffset(0), 'UTC+0'); });
test('uptime keeps counting hours', () => assert.equal(uptime(180727), '50:12:07'));
test('aqi levels are strictly above', () => { assert.equal(levelAbove(100, 100, 150), 'ok'); assert.equal(levelAbove(101, 100, 150), 'warn'); assert.equal(levelAbove(150, 100, 150), 'warn'); assert.equal(levelAbove(151, 100, 150), 'crit'); assert.equal(levelAbove(null, 100, 150), 'ok'); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL (`chip`, `scrambleFrame`, `countdown`, `ago`, `utcOffset`, `uptime`, `levelAbove` are not exported).

- [ ] **Step 3: Implement the functions in the files named above**

- [ ] **Step 4: Run them and watch them pass**

Run: `node --test shell/tests/*.test.mjs`
Expected: PASS, every Plan 1 test included.

- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib shell/tests
git commit -m "feat(shell): add chip, scramble, countdown and strict-level logic"
```

---

### Task 2: Demand-driven sampling and on-demand feeds

**Files:**
- Modify: `crates/ghost-proto/src/lib.rs`, `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/sampler.rs`, `crates/ghostd/src/server/{mod.rs,state.rs,sampling.rs}`, `crates/ghostd/src/{lib.rs,main.rs,fake.rs}`, `crates/ghostd/tests/server.rs` (the `Hub` literal gains `feeds`)
- Create: `crates/ghostd/src/feeds/{mod.rs,host.rs}`
- Test: `crates/ghostd/tests/feeds.rs`; unit tests in `sampler.rs`

**Interfaces:**
- Consumes: `State::{subscribe, remove_ui, publish}`, `Sampler`, `Collectors`, `Hub`, `tests/common::FakeRoot`.
- Produces:
  - **`topics`:** `HOST = "host"`, `DESKTOP = "desktop"`, `WEATHER = "weather"`, `SKY = "sky"`, `SPECTRUM = "audio.spectrum"`. All are in `BUILTIN` (now 11 entries), so providers can't claim them before their feeds exist. `WEATHER_ALERTS = "weather.alerts"` is a well-known name that is **not** built in.
  - `HostSnapshot { hostname: String, kernel: String, boot_time_s: u64 }`.
  - `server::Publisher` (Clone + Send + Sync) is a handle around `Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>`, with:
    - `Publisher::new(f)`, which tests use;
    - `publish<T: Serialize>(&self, topic: &str, value: &T)`. The server's publisher caches, sequences and relays exactly like a provider publish.
  - `server::Demand = watch::Receiver<BTreeSet<String>>`: the union of every UI client's subscriptions. It is sent only when the union changes (on subscribe, disconnect, or a dropped slow client).
  - `server::FeedContext { publisher: Publisher, demand: Demand, config: watch::Receiver<ResolvedConfig> }`.
  - `server::Feed = Box<dyn FnOnce(FeedContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>`. `Hub` gains `pub feeds: Vec<Feed>`, and `serve` spawns each one once in a `JoinSet` owned by `serve`.
  - `feeds::wanted(demand: &Demand, topic: &str) -> bool` and `async fn feeds::until_wanted(demand: &mut Demand, topic: &str)`.
  - `Sampler::tick(&mut self, hist: &mut History, wanted: &BTreeSet<String>) -> Vec<ServerMsg>`:
    - samples `cpu`, `mem`, `gpu` and `disks` only when wanted;
    - samples `sensors` when `sensors` **or** `disks` is wanted, because drive temperatures come from the sensors;
    - emits `Data` only for wanted topics, in the old order, while `seq` still rises every tick;
    - pushes history only for sampled sources.
  - `selfmon` samples only while `perf` is wanted.
  - `feeds::host::feed(root: SysRoot) -> Feed` reads `proc/sys/kernel/hostname`, `proc/sys/kernel/osrelease` and the `btime` line of `proc/stat`, and publishes **once at start** **[call]**: none of them changes without a reboot or an explicit `hostnamectl`. Unreadable values give `""` / `0`.
  - `fake::feeds() -> Vec<Feed>`: `host` as `tkacz-workstation`, kernel `7.2.11-200.fc44.x86_64`, booted 8047 s ago (the mockup's 2:14:07). Later tasks append their fakes.
  - `main` builds the real feed list.
  - **Cost:** no per-tick work. While nothing is subscribed (covered with no bar), no built-in collector runs at all.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/feeds.rs — a fake sampler at --tick-ms 50 plus a test feed, as in tests/server.rs
#[tokio::test] async fn unsubscribed_builtins_are_not_published() { /* subscribe [mem] → ≥ 2 mem lines within 6 ticks, 0 cpu/sensors/gpu/disks */ }
#[tokio::test] async fn demand_is_the_union_of_ui_clients() { /* the test feed forwards each Demand value; A [cpu], B [gpu, host] → {cpu, gpu, host}; drop B → {cpu} */ }
#[tokio::test] async fn feed_output_reaches_subscribers_and_late_joiners() { /* the test feed publishes "host" once it is wanted; UI 1 [host] → data; UI 2 subscribes later → the cached value at once */ }
#[tokio::test] async fn providers_cannot_claim_new_builtin_names() { /* provide ["weather"] → `topic "weather" is built in`; provide ["weather.alerts"] → accepted */ }
#[tokio::test] async fn host_feed_reads_the_fake_root() { /* hostname "box\n", osrelease "7.2.11\n", proc/stat with "btime 1792500000" → HostSnapshot { "box", "7.2.11", 1792500000 } */ }
// sampler.rs
#[test] fn tick_samples_only_wanted_sources() { /* counting sources; wanted {cpu} → cpu sampled once, mem 0; messages == [cpu] */ }
#[test] fn disks_pull_in_sensors_without_publishing_them() { /* wanted {disks} → sensors sampled, messages == [disks] with temps attached */ }
```

Existing sampler tests pass `&all()`, where `all()` is the `BUILTIN` set.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test feeds; cargo test -p ghostd --lib -- sampler`
Expected: compile errors (`feeds`, `Publisher`, the new `tick` signature).

- [ ] **Step 3: Implement the demand set, the feed plumbing, the new `tick` and the host feed**

- [ ] **Step 4: Run them, plus the Plan 1 server suite**

Run: `cargo test -p ghostd --test feeds && cargo test -p ghostd --lib -- sampler && cargo test -p ghostd --test server`
Expected: PASS: feeds 5, the sampler tests including the 2 new ones, server 15.

- [ ] **Step 5: Measure and commit**

Measure the release daemon with one client subscribed to `[desktop, perf]` only for 60 s (`scripts/measure-cpu.sh`). Expected: ≤ 0.05 % of a core.

```bash
git add crates/ghost-proto/src crates/ghostd/src crates/ghostd/tests
git commit -m "feat(ghostd): sample only subscribed topics and run on-demand feeds"
```

---

### Task 3: Desktop service: coverage reports, view actions and global shortcuts

**Files:**
- Create: `crates/ghostd/src/feeds/{desktop.rs,shortcuts.rs}`, `crates/ghostd/src/keyseq.rs`
- Modify: `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/{lib.rs,main.rs}` (`main` runs the desktop feed in both real and fake mode), `crates/ghostd/src/config/load.rs`, `crates/ghostd/Cargo.toml`
- Test: `crates/ghostd/tests/desktop.rs`, `crates/ghostd/tests/config_load.rs` (2 tests), unit tests in `keyseq.rs`

**Interfaces:**
- Consumes: `Publisher`, `Feed`, `FeedContext` (Task 2); `Shortcuts { monitor, swap }` (Plan 1 config).
- Produces:
  - `DesktopSnapshot { covered_outputs: Vec<String>, active: bool, action: Option<ViewAction>, action_seq: u64 }` and `ViewAction { Monitor, Swap }` (snake_case). Before the first report the defaults are `covered_outputs: []` and `active: false`: the dashboard renders, and shortcuts stay off until the KWin script says no window has focus.
  - `DesktopState`:
    - `report(&mut self, covered_outputs: Vec<String>, active: bool) -> bool` returns whether anything changed;
    - `activate(&mut self, action: ViewAction) -> bool` succeeds only while `active`, setting `action` and incrementing `action_seq`;
    - `snapshot(&self) -> DesktopSnapshot`.
  - **D-Bus service:**
    - well-known name `org.ghostshell.Ghostd` on the session bus, object `/org/ghostshell/Desktop`, interface `org.ghostshell.Desktop1`;
    - `Report(covered_outputs: s, active: b)`, where `covered_outputs` holds output names joined by `,` and `""` means none **[call]**: KWin's `callDBus` marshals a JS array as `av`, not `as`;
    - `Activate(action: s)` takes `monitor` or `swap` and returns D-Bus `InvalidArgs` with `unknown view action "x"` for anything else. It is the path the shortcut handler uses, and it lets tests drive views on a private bus **[call]**.

    `DesktopService::new(publisher: Publisher)` implements the interface (Clone; shared `Arc<Mutex<DesktopState>>`) and publishes `desktop` on every change.
  - `feeds::desktop::feed() -> Feed` connects to the session bus, requests the name, serves `DesktopService`, publishes the default snapshot, and runs `shortcuts::run`. With no bus, or with the name already owned, it logs once and publishes the default.
  - **Key sequences:** `keyseq::parse(s: &str) -> Result<i32, KeySeqError>` parses one Qt key combination: `+`-separated modifiers `Shift 0x0200_0000`, `Ctrl 0x0400_0000`, `Alt 0x0800_0000`, `Meta 0x1000_0000` (case-insensitive), then exactly one key:
    - `A`–`Z` → `0x41`–`0x5A`;
    - `0`–`9` → `0x30`–`0x39`;
    - `F1`–`F24` → `0x0100_0030 + n − 1`;
    - `Space 0x20`, `Tab 0x0100_0001`, `Return 0x0100_0004`, `Escape 0x0100_0000`.

    The error text is `cannot parse "<s>" as a key sequence`. Config validation reports it as `shortcuts.monitor: cannot parse "Meta+" as a key sequence`.
  - **Shortcut registration:** `shortcuts::run(conn: zbus::Connection, config: watch::Receiver<ResolvedConfig>, desktop: DesktopService)` registers with `org.kde.kglobalaccel` at `/kglobalaccel`, interface `org.kde.KGlobalAccel`:
    - `doRegister(as)`, then `setShortcutKeys(as, a(ai), u)` for the action ids `["ghost-shell", "monitor", "ghost-shell", "Toggle the monitor view"]` and `["ghost-shell", "swap", "ghost-shell", "Next main view"]`, with flags `SetPresent | NoAutoloading`, so config.toml always wins;
    - listens to `globalShortcutPressed(s, s, x)` on `/component/ghost_shell`, interface `org.kde.kglobalaccel.Component`, and calls `activate`;
    - re-registers whenever `shortcuts.*` changes;
    - with no kglobalaccel present, logs once (`Activate` still works).
  - **Cost:** event-driven, nothing per tick.

- [ ] **Step 1: Confirm the KGlobalAccel interface (read-only)**

Run: `busctl --user introspect org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel | rg 'doRegister|setShortcutKeys'`, and look up `enum SetShortcutFlag` in the installed KF6 KGlobalAccel header, or in the kglobalaccel source tag matching `rpm -q kf6-kglobalaccel`.

Expected:
- `.doRegister method as`;
- `.setShortcutKeys method asa(ai)u a(ai)`;
- the numeric values of `SetPresent` and `NoAutoloading`.

Put those values in named constants. If a signature differs, stop and report it.

- [ ] **Step 2: Write the failing tests**

```rust
// keyseq.rs
#[test] fn parses_key_sequences() { assert_eq!(parse("Meta+M").unwrap(), 0x1000_004D); assert_eq!(parse("Meta+Shift+M").unwrap(), 0x1200_004D); assert_eq!(parse("ctrl+alt+f12").unwrap(), 0x0D00_003B); }
#[test] fn rejects_bad_key_sequences() { assert_eq!(parse("Meta+").unwrap_err().to_string(), "cannot parse \"Meta+\" as a key sequence"); assert!(parse("Meta+M+K").is_err()); assert!(parse("Hyper+M").is_err()); }
// tests/desktop.rs
#[test] fn report_changes_only_when_values_change() { }
#[test] fn actions_count_only_on_the_desktop() { /* default → activate false, seq 0; report([], true) → activate(Monitor) true, seq 1, action Some(Monitor); report(["DP-1"], false) → activate false */ }
#[tokio::test] async fn dbus_report_reaches_the_publisher() { /* zbus p2p over UnixStream::pair; Report("DP-1,HDMI-A-1", false) → published DesktopSnapshot { covered_outputs: ["DP-1","HDMI-A-1"], active: false, .. } */ }
#[tokio::test] async fn dbus_activate_rejects_unknown_actions() { /* Activate("nope") → error text `unknown view action "nope"` */ }
// tests/config_load.rs
#[test] fn bad_shortcut_names_the_key() { assert_eq!(load_str_with_user("[shortcuts]\nmonitor = \"Meta+\"\n", "u").unwrap_err().message, "shortcuts.monitor: cannot parse \"Meta+\" as a key sequence"); }
#[test] fn default_shortcuts_parse() { assert!(load_str_with_user("", "u").is_ok()); }
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p ghostd --test desktop; cargo test -p ghostd --lib -- keyseq`
Expected: compile errors.

- [ ] **Step 4: Implement `keyseq`, `DesktopState`, `DesktopService`, the feed, `shortcuts::run` and the validation**

- [ ] **Step 5: Run them and watch them pass**

Run: `cargo test -p ghostd --test desktop && cargo test -p ghostd --lib -- keyseq && cargo test -p ghostd --test config_load`
Expected: PASS (desktop 4, keyseq 2, config_load + 2).

- [ ] **Step 6: Live check (ask the user first)**

This registers Meta+M and Meta+Shift+M in the user's KDE shortcuts, so ask the user before running it.

Run `target/debug/ghostd run --socket /tmp/gs.sock`, then `busctl --user call org.ghostshell.Ghostd /org/ghostshell/Desktop org.ghostshell.Desktop1 Report sb "" true` and press Meta+M.

Expected: a UI client subscribed to `desktop` (the Plan 1 Task 12 snippet) sees `action_seq` go from 0 to 1 with `"action":"monitor"`.

- [ ] **Step 7: Commit**

```bash
git add crates/ghost-proto/src crates/ghostd
git commit -m "feat(ghostd): add the desktop service with view actions and global shortcuts"
```

---

### Task 4: KWin desktop script (keeper, coverage, focus)

**Files:**
- Create: `kwin/ghost-shell-desktop/metadata.json`, `kwin/ghost-shell-desktop/contents/code/main.js`, `scripts/kwin-script.sh`, `shell/tests/kwin-desktop.test.mjs`
- Modify: `scripts/install.sh` (copy the package to `~/.local/share/kwin/scripts/ghost-shell-desktop/`; **never enable it**)

**Interfaces:**
- Consumes: D-Bus `Report(s, b)` (Task 3).
- Produces:
  - **metadata.json:** `KPlugin.Id` `ghost-shell-desktop`, `X-Plasma-API` `javascript`, `X-Plasma-MainScript` `code/main.js`, `KPackageStructure` `KWin/Script`, licence `GPL-3.0-or-later`.
  - **`main.js`:**
    - **Keeper:** a port of gridlock-keeper (ledger R46). Every desktop window with `resourceClass === "quickshell"` that sits below the top-most `plasmashell` desktop window is raised. It reacts to those windows' `stackingOrderChanged` (KWin raises only after `windowActivated`).
    - `coveredBy(area, rects) -> bool`: true iff the union of `rects` covers `area` completely. It uses integer rectangle subtraction and is pure.
    - `countsAsCover(w) -> bool`:
      - `(w.normalWindow || w.dialog) && !w.minimized && w.opacity >= 1`;
      - on the current desktop (`onAllDesktops`, or `desktops` contains `workspace.currentDesktop`);
      - on the current activity (empty `activities`, or containing `workspace.currentActivity`).

      Desktop windows (ours and Plasma's) and docks never count.
    - `currentState() -> { covered: string[], active: bool }`:
      - `covered` lists the `output.name` of each output whose `workspace.clientArea(KWin.MaximizeArea, output, workspace.currentDesktop)` is `coveredBy` the covering windows' `frameGeometry`; it is `[]` while `workspace.showingDesktop`.
      - `active = workspace.showingDesktop || workspace.activeWindow === null || workspace.activeWindow.desktopWindow`.
    - `report()` calls `callDBus("org.ghostshell.Ghostd", "/org/ghostshell/Desktop", "org.ghostshell.Desktop1", "Report", covered.join(","), active)`, but only when the pair differs from the last one sent.
    - **Triggers:**
      - workspace `windowAdded`, `windowRemoved`, `windowActivated`, `currentDesktopChanged`, `currentActivityChanged`, `showingDesktopChanged`, `screensChanged`;
      - per window: `frameGeometryChanged`, `minimizedChanged`, `opacityChanged`, `desktopsChanged`, `activitiesChanged`, `fullScreenChanged`.

      KWin's JS API names must be checked against KWin 6's scripting docs while implementing. A ghostd that restarts gets its first report at the next window event **[call]**: a KWin script can't watch bus names, and polling is banned.
  - **Coverage is about window geometry.** A window that is translucent through its *content* (its own alpha) counts as opaque, so it covers (decision 3). KWin's `alpha` flag is also true for opaque CSD windows. An allowlist of apps that never count as covering is a possible later addition, if the user reports translucent windows over the desktop.
  - **`scripts/kwin-script.sh install|enable|disable`:**
    - `install` runs `kpackagetool6 --type KWin/Script --install`, or `--upgrade` when already installed;
    - `enable` and `disable` run `kwriteconfig6 --file kwinrc --group Plugins --key ghost-shell-desktopEnabled true|false`, then `qdbus6 org.kde.KWin /KWin reconfigure`.

    **`enable` changes the user's KWin config, so ask the user first.**

- [ ] **Step 1: Write the failing tests** (load `main.js` into a `vm` context with stub `workspace`, `KWin`, `callDBus` and signal objects)

```js
test('a fullscreen window covers its output', ...);           // covered == ['DP-1']
test('two half-screen windows together cover', ...);
test('a one-pixel gap is not covered', ...);
test('minimised, translucent and other-desktop windows do not cover', ...); // opacity 0.9, minimized, desktops [d2]
test('desktop windows and docks never cover', ...);
test('show desktop uncovers and counts as on the desktop', ...);
test('active when nothing or a desktop window is focused', ...);
test('reports only on change', ...);                          // two identical events → one callDBus
test('keeper raises quickshell desktop windows above plasmashell', ...); // order [qs, plasma] → raiseWindow(qs)
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL (`main.js` is missing).

- [ ] **Step 3: Implement the script, its metadata and `kwin-script.sh`**

- [ ] **Step 4: Run them and watch them pass**

Run: `node --test shell/tests/*.test.mjs`
Expected: PASS.

- [ ] **Step 5: Live check (user steps, R6)**

With the user's consent, run `scripts/kwin-script.sh install && scripts/kwin-script.sh enable`, and run a dev ghostd. Ask the user to:
1. maximise a window;
2. restore it;
3. press Meta+D.

Expected: a `desktop` subscriber sees `covered_outputs` become `["DP-1"]`, then `[]`, then `active: true`. Clicking the empty desktop keeps the dashboard above Plasma's wallpaper.

- [ ] **Step 6: Commit**

```bash
git add kwin scripts/kwin-script.sh scripts/install.sh shell/tests/kwin-desktop.test.mjs
git commit -m "feat(kwin): keep the dashboard above plasma and report coverage and focus"
```

---

### Task 5: Data: per-topic values, on-demand topics and covered subscriptions

**Files:**
- Create: `shell/kit/GhostKit/TopicValue.qml`, `shell/kit/GhostKit/lib/subscriptions.mjs`, `shell/tests/subscriptions.test.mjs`
- Modify: `shell/kit/GhostKit/{Data.qml,qmldir}`, `widgets/{processor,graphics,memory,storage,sensors}/Widget.qml`, any `shell/host/*.qml` that reads `Data.topics`

**Interfaces:**
- Consumes: Plan 1's `Data` (Task 14, R43) and the five machine widgets.
- Produces:
  - **`Data.topic(name: string): TopicValue`.** There is one object per name for the whole session. `TopicValue` (an internal type in qmldir) has `readonly property var value` (null until the first message), `readonly property int seq` and `readonly property double receivedMs`. A message re-evaluates only the bindings on its own topic's object.

    This replaces `Data.topics`. Re-assigning one shared object re-ran every topic binding on every message, which a 30 Hz spectrum makes unaffordable. `Data.topics` is **removed**, so a leftover read fails loudly. `Data.history` is unchanged.
  - `Data.want(name)` / `Data.release(name)` are refcounted. An invalid name logs `invalid topic name "X"` and is ignored.
  - `Data.covered: bool` (default false; Task 8 sets it).
  - **`subscriptions.mjs`:**
    - `requiresOf(layout) -> string[]` collects the union of `requires` over every slot and tab of **every** view, so history stays continuous across view switches. This is Plan 1's behaviour, kept.
    - `subscriptionFor({ requires, wanted, covered, barRequires }) -> string[]` returns a sorted, de-duplicated list:
      - visible: `requires ∪ wanted ∪ barRequires ∪ {desktop, perf}`;
      - covered: `barRequires ∪ {desktop, perf}`.

      `Data` passes `barRequires = []` until Plan 3.
  - `Data` sends `subscribe` only when that list changes.

- [ ] **Step 1: Write the failing tests**

```js
test('visible subscribes to every slot, wanted topics, desktop and perf', () => assert.deepEqual(subscriptionFor({ requires: ['mem', 'cpu', 'cpu'], wanted: ['audio.spectrum'], covered: false, barRequires: [] }), ['audio.spectrum', 'cpu', 'desktop', 'mem', 'perf']));
test('covered keeps only desktop, perf and the bar', () => assert.deepEqual(subscriptionFor({ requires: ['cpu'], wanted: ['audio.spectrum'], covered: true, barRequires: ['weather'] }), ['desktop', 'perf', 'weather']));
test('requires span every view and tab', () => assert.deepEqual(requiresOf({ views: { v1: { requires: ['cpu'], widget: 'processor' }, v3: { tabs: [{ requires: ['gpu'] }, { requires: ['mem', 'gpu'] }] } } }), ['cpu', 'gpu', 'mem']));
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Implement `subscriptions.mjs`, `TopicValue`, the new `Data` API, and migrate the widgets to `Data.topic(...)`**

- [ ] **Step 4: Run the tests and the smoke checks**

Run: `node --test shell/tests/*.test.mjs && make shaders && scripts/qml-smoke.sh && GHOST_CONFIG=/tmp/ghost-layout-test.toml scripts/qml-smoke.sh; rg -n 'Data\.topics' shell widgets`
Expected: PASS, `smoke: ok` twice, and no `rg` output.

- [ ] **Step 5: Measure and commit**

Measure the shell for 60 s on the default V1 against `ghostd run --fake`, at the base commit and after this task. Expected: CPU after ≤ before. The controller records both in README.

```bash
git add shell widgets
git commit -m "perf(shell): notify per topic and subscribe on demand"
```

---

### Task 6: GhostKit clock modes, chips, glyphs and first-write scramble

**Files:**
- Create: `shell/kit/GhostKit/{FirstWrite.qml,ScrambleText.qml,Chip.qml,Glyph.qml}`
- Modify: `shell/kit/GhostKit/{FrameClock.qml,Theme.qml,Panel.qml,AlertBox.qml,qmldir}`, `shell/kit/GhostKit/lib/theme.mjs` and `shell/tests/theme.test.mjs` (remove `TAGS.crit`), `shell/host/Gallery.qml`

**Interfaces:**
- Consumes: Task 1 (`chip`, `scrambleFrame`); Plan 1's `FrameClock` (R9), `Panel`, `AlertBox`.
- Produces:
  - **`FrameClock`** (still one Timer):
    - `elapsedMs`, which every derived motion uses, so the tick rate never changes speeds;
    - `acquireFast()` / `releaseFast()`: while any fast holder exists, the Timer runs at 33 ms (spike decision 3); otherwise at 67;
    - `epochSecond` with `acquireSeconds()` / `releaseSeconds()`: while the Timer runs, it updates on the tick that crosses a wall-clock second; while the Timer is stopped and seconds holders exist, a single-shot Timer re-armed to the next second boundary is the only source, so two clocks never both produce frames;
    - `suspended: bool`: while true every timer stops and nothing changes.
  - **`FirstWrite` singleton:** `claim(key: string): bool` is true the first time a key is claimed since the last `reset()`. Plan 2c's greeting calls `reset()`.
  - **`ScrambleText`** (a `Text`) with `scrambleKey` (default: its text) and `durationMs` (default 520, the mockup's). On first appearance, if `FirstWrite.claim(scrambleKey)`, it shows `scrambleFrame(text, elapsed / durationMs, Math.random)` on FrameClock ticks, holding `acquire()` only while it runs. Later text changes are shown directly.
  - `Theme.chip(key)` delegates to `chip(key, japanese)`. `Theme` also gains `aqiWarn` and `aqiCrit` from `thresholds`.
  - **`Chip { key }`:** `Theme.chip(key)` in a box: 9 px, weight 500, tracking .04 em, signal text, a 1 px border of signal at 35 %, padding 0 × 5, line height 14 (mockup `.chip`). It is hidden when the text is empty.
  - **`Glyph { name; color; size }`:** a 12-unit `PathSvg` scaled to `size`. Stroked shapes use 1.6 units with square caps; filled ones are noted. Paths:
    - `chevron-up` `M2.6 7.6 6 4.6l3.4 3`;
    - `chevron-down` `M2.6 4.4 6 7.4l3.4-3`;
    - `chevron-left` `M7.6 2.6 4.6 6l3 3.4`;
    - `chevron-right` `M4.4 2.6 7.4 6l-3 3.4`;
    - `swap` `M5.2 2.6 2.2 6l3 3.4M6.8 2.6l3 3.4-3 3.4`;
    - `prev` `M2.6 3v6M9 2.8 5.6 6 9 9.2`;
    - `next` `M9.4 3v6M3 2.8 6.4 6 3 9.2`;
    - `play` `M3 2v8l7-4z` (filled);
    - `pause`: rectangles `2.5,2,2.4,8` and `7.1,2,2.4,8` (filled);
    - `search`: a circle at (5,5) with r 3.6, plus `M7.7 7.7 11 11`;
    - `scroll` `M3 4.6 6 2.2l3 2.4M3 7.4 6 9.8l3-2.4`.

    These come from the mockup.
  - **`Panel`** gains `header: bool` (default true) and `framed: bool` (default true), for the mockup's headerless and frameless panels. Its label becomes a `ScrambleText` keyed `"panel:" + label`, with a duration of 500 + random·300 ms (mockup `writeLabels`).
  - **`AlertBox`** shows its chip with `Chip { key: chipKey }` (katakana in `mixed`), and phases its stripes by `elapsedMs`, one 16 px tile per 1600 ms. `TAGS.crit` is removed.

- [ ] **Step 1: Extend the gallery and watch the smoke test fail**

Add to the gallery:
- every chip key;
- every glyph;
- a `ScrambleText`;
- a headerless panel and a frameless panel;
- a toggle that holds `acquireFast()`;
- an `epochSecond` readout.

Update `theme.test.mjs`: `tag('crit', 'mixed') === ''`.

Run: `node --test shell/tests/*.test.mjs; GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: FAIL.

- [ ] **Step 2: Implement the components and the `FrameClock` additions**

- [ ] **Step 3: Run the tests and the smoke check**

Run: `node --test shell/tests/*.test.mjs && make shaders && GHOST_VIEW=gallery scripts/qml-smoke.sh`
Expected: PASS and `smoke: ok`.

- [ ] **Step 4: Check it visually and measure the clock modes**

Take a `GHOST_SNAPSHOT` (R3) of the gallery. Check:
- the glyphs match the mockup's icons;
- the chips read katakana;
- each label scrambles once.

Then count frames with `QSG_RENDER_TIMING=1` over 30 s:
- ≈ 15 frames/s with the stripes alone;
- ≈ 30 frames/s with fast mode held;
- the blink period is 1 s in both cases.

- [ ] **Step 5: Commit**

```bash
git add shell/kit shell/host/Gallery.qml shell/tests/theme.test.mjs
git commit -m "feat(shell): add chips, glyphs, first-write scramble and clock modes to GhostKit"
```

---

### Task 7: Views and the CRT print transition

**Files:**
- Create: `shell/kit/GhostKit/lib/views.mjs`, `shell/kit/GhostKit/Views.qml`, `shell/host/PrintSwap.qml`, `shell/tests/views.test.mjs`
- Modify: `shell/kit/GhostKit/qmldir`, `shell/host/{Desktop.qml,TabsHost.qml}`, `scripts/qml-smoke.sh`

**Interfaces:**
- Consumes: `Data.topic("desktop")` (Tasks 3 and 5), `FirstWrite` and `ScrambleText` (Task 6), `LayoutView` (Plan 1).
- Produces:
  - **`views.mjs`:**
    - `PRINT_STEPS = 24`, `MONITOR_MS = 640`, `MAIN_MS = 380` (spec §6.1).
    - `initialViews(dashboard) -> { current, lastMain }`, both equal to `default_view`.
    - `applyAction(state, action, dashboard) -> { current, lastMain, durationMs } | null` (null means no change):
      - `monitor`: null when `monitor_view === ""`. On the monitor view it returns to `lastMain`; otherwise it goes to `monitor_view` and sets `lastMain` to `current`. Both take 640 ms.
      - `swap`: goes to the main view after `lastMain` in `main_views`, wrapping, and returns null when that is `current`. From the monitor view it takes 640 ms, otherwise 380.
    - `reconcileViews(state, dashboard, viewNames)`: a `current` that no longer exists becomes `default_view`, and so does a `lastMain` missing from `main_views`. There is no transition.
    - `revealRows(step, steps, height) = Math.round(height * step / steps)`.
  - **`Views` singleton (GhostKit, public, so Plan 3's view chip uses it):**
    - `current`, `lastMain`, `isMonitor`, `busy`, `durationMs`;
    - `toggleMonitor()` and `swap()`;
    - `tabIndex(view, path, fallback)` and `setTabIndex(view, path, index)`. These hold tab selections for the session, because views are now destroyed when left.
    - It acts on each increase of `desktop.action_seq`. The first value it sees only sets the baseline, so a cached action never replays. Actions are ignored while `busy`.
    - It logs `view <from> -> <to> (<ms> ms)`.
  - **`PrintSwap`:**
    - holds the outgoing and incoming `LayoutView`;
    - runs one Timer at `durationMs / 24` **only during the transition** **[call]**: the spec fixes 24 steps in 640 or 380 ms, which no 15 Hz clock can draw, and the sequence is transient;
    - step `i` shows the incoming view's top `revealRows(i)` rows and the outgoing view below them, with a 2 px scanline of `hi` at 55 % (mockup) at the edge;
    - after step 24 the outgoing view is destroyed, so only one view's widgets ever run;
    - the incoming panel labels scramble through `FirstWrite`.
  - `TabsHost` stores its selection through `Views.tabIndex` / `Views.setTabIndex`.
  - **`qml-smoke.sh`:** `GHOST_SMOKE_BUS=private` runs ghostd and the shell inside `dbus-run-session`. `GHOST_SMOKE_ACTIONS="monitor monitor swap"` sends `Report "" true`, then each action through `busctl --user call … Activate s <action>`, 1 s apart.

- [ ] **Step 1: Write the failing tests**

```js
const dash = { main_views: ['v1', 'v2'], monitor_view: 'monitor', default_view: 'v1' };
test('monitor toggles and returns to the main view it came from', () => { assert.deepEqual(applyAction({ current: 'v2', lastMain: 'v2' }, 'monitor', dash), { current: 'monitor', lastMain: 'v2', durationMs: 640 }); assert.deepEqual(applyAction({ current: 'monitor', lastMain: 'v2' }, 'monitor', dash), { current: 'v2', lastMain: 'v2', durationMs: 640 }); });
test('swap cycles main views at 380 ms', () => assert.deepEqual(applyAction({ current: 'v2', lastMain: 'v2' }, 'swap', dash), { current: 'v1', lastMain: 'v1', durationMs: 380 }));
test('swap from monitor prints the next main view at 640 ms', () => assert.deepEqual(applyAction({ current: 'monitor', lastMain: 'v1' }, 'swap', dash), { current: 'v2', lastMain: 'v2', durationMs: 640 }));
test('no monitor view or one main view is a no-op', () => { assert.equal(applyAction({ current: 'v1', lastMain: 'v1' }, 'monitor', { ...dash, monitor_view: '' }), null); assert.equal(applyAction({ current: 'v1', lastMain: 'v1' }, 'swap', { ...dash, main_views: ['v1'] }), null); });
test('a removed view falls back to the default', () => assert.deepEqual(reconcileViews({ current: 'v3', lastMain: 'v3' }, dash, ['v1', 'v2', 'monitor']), { current: 'v1', lastMain: 'v1' }));
test('reveal rows are whole pixels and end at the full height', () => { assert.equal(revealRows(1, 24, 666), 28); assert.equal(revealRows(24, 24, 666), 666); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Implement `views.mjs`, `Views`, `PrintSwap`, and the `Desktop`/`TabsHost` changes**

- [ ] **Step 4: Run the tests and a private-bus smoke check**

`/tmp/ghost-views-test.toml` defines `v1`, `v2` and `monitor`, each a single `placeholder` slot, plus `dashboard.main_views = ["v1", "v2"]` and `monitor_view = "monitor"`.

Run: `node --test shell/tests/*.test.mjs && GHOST_CONFIG=/tmp/ghost-views-test.toml GHOST_SMOKE_BUS=private GHOST_SMOKE_ACTIONS="monitor monitor swap" scripts/qml-smoke.sh`
Expected: PASS. The shell log shows, in order, `view v1 -> monitor (640 ms)`, `view monitor -> v1 (640 ms)` and `view v1 -> v2 (380 ms)`, then `smoke: ok`.

- [ ] **Step 5: Commit**

```bash
git add shell scripts/qml-smoke.sh
git commit -m "feat(shell): switch views with the crt print transition"
```

---

### Task 8: Occlusion: zero frames while covered

**Files:**
- Create: `scripts/measure-covered.sh`
- Modify: `shell/host/{Desktop.qml,PrintSwap.qml}`

**Interfaces:**
- Consumes: the `desktop` topic (Task 3), the KWin report (Task 4), `Data.covered` (Task 5), `FrameClock.suspended` (Task 6), `Views` (Task 7).
- Produces:
  - **`Desktop`:**
    - `covered = (Data.topic("desktop").value?.covered_outputs ?? []).includes(screen.name)`;
    - logs `desktop on <screen.name>` at start, and `dashboard covered` or `dashboard visible` on each change.
  - **While covered:**
    - the view loader is inactive, so every widget is destroyed and releases its wants and clock holders;
    - `FrameClock.suspended = true` and `Data.covered = true`;
    - a running transition jumps to its end.

    The dot grid and scanlines stay; they are static.
  - **When visible again:** the current view loads with no transition **[call]**: windows moving away is not a view change.
  - **`scripts/measure-covered.sh [seconds=60]`:**
    1. Under `dbus-run-session`, starts a release `ghostd run` (real collectors, a temp config, no providers) and the shell with `QSG_RENDER_TIMING=1`.
    2. After 8 s, reads the shell's output name and calls `Report "<name>" false`.
    3. Waits 5 s, then measures both pids with `measure-cpu.sh` and counts `syncAndRender: frame rendered` lines over the window.
    4. Calls `Report "" false` and checks that `dashboard visible` appears within 2 s.

    It prints `covered: frames=<n> shell_cpu=<x> daemon_cpu=<y> total=<z>`.

- [ ] **Step 1: Write `scripts/measure-covered.sh` and run it before the change**

Run: `cargo build --release -p ghostd && scripts/measure-covered.sh 30`
Expected: FAIL, with `frames` > 0 (the uncovered dashboard keeps drawing, as the spike showed).

- [ ] **Step 2: Implement the covered state in `Desktop` and `PrintSwap`**

- [ ] **Step 3: Measure again**

Run: `scripts/measure-covered.sh 60`
Expected: `frames=0` and `total` ≤ 0.50, then `dashboard visible`.

- [ ] **Step 4: Commit**

```bash
git add shell/host scripts/measure-covered.sh
git commit -m "perf(shell): stop rendering and collecting while the dashboard is covered"
```

---

### Task 9: Weather feed (Open-Meteo forecast and air quality)

**Files:**
- Create: `crates/ghostd/src/feeds/{weather.rs,http.rs}`, `crates/ghostd/tests/weather.rs`, `crates/ghostd/tests/fixtures/{open-meteo-forecast.json,open-meteo-air.json}`
- Modify: `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/{main.rs,fake.rs}`, `crates/ghostd/src/config/load.rs`, `Cargo.toml`, `crates/ghostd/Cargo.toml` (`ureq`)
- Test: `crates/ghostd/tests/config_load.rs` (2 tests)

**Interfaces:**
- Consumes: `Feed`, `FeedContext`, `Publisher`, `until_wanted` (Task 2); `location.{lat, lon}` (Plan 1 config).
- Produces:
  - **Types:**
    - `WeatherSnapshot { available: bool, stale: bool, updated_s: u64, temp_c: f32, code: u8, condition: String, is_day: bool, humidity_pct: u8, wind_kmh: f32, high_c: f32, low_c: f32, hours: Vec<WeatherHour>, rain_peak: Option<RainPeak>, aqi: Option<u32> }`;
    - `WeatherHour { at_s: u64, temp_c: f32, rain_pct: u8 }`;
    - `RainPeak { pct: u8, at_s: u64 }`.

    Without lat/lon, `available = false` and everything else is default.
  - **HTTP:** `http::Fetch` (`Send + Sync`) with `fn get(&self, url: &str) -> Result<String, FetchError>`; `http::Ureq` uses a 15 s timeout (the user's `storm_alert.py` uses 15 s) and the User-Agent `ghost-shell/<crate version>`.
  - **URLs:**
    - `weather::forecast_url(lat, lon)` is exactly `https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&current=temperature_2m,relative_humidity_2m,weather_code,wind_speed_10m,is_day&hourly=temperature_2m,precipitation_probability&daily=temperature_2m_max,temperature_2m_min&timezone=auto&timeformat=unixtime&forecast_days=2`.
    - `weather::air_url(lat, lon)` is exactly `https://air-quality-api.open-meteo.com/v1/air-quality?latitude={lat}&longitude={lon}&current=us_aqi&timeformat=unixtime`.
  - **Parsing:**
    - `weather::parse_forecast(json: &str, now_s: u64) -> Result<Forecast, WeatherError>`:
      - `hours` are the 6 hourly entries from the next full hour on;
      - `rain_peak` is the highest `precipitation_probability` in `[now, now + 12 h]` (the mockup's `RAIN 95% AT 22H`), the earliest on ties, and `None` when the highest is 0;
      - `high_c` and `low_c` come from `daily[0]`.
    - `weather::parse_air(json: &str) -> Result<u32, WeatherError>` reads `current.us_aqi`.
    - `weather::condition(code: u8) -> &'static str` maps the WMO codes (Open-Meteo's table):

      | Code | Label |
      |---|---|
      | 0 | `CLEAR` |
      | 1 | `MAINLY CLEAR` |
      | 2 | `PARTLY CLOUDY` |
      | 3 | `OVERCAST` |
      | 45 | `FOG` |
      | 48 | `RIME FOG` |
      | 51, 53, 55 | `DRIZZLE` |
      | 56, 57 | `FREEZING DRIZZLE` |
      | 61 | `LIGHT RAIN` |
      | 63 | `RAIN` |
      | 65 | `HEAVY RAIN` |
      | 66, 67 | `FREEZING RAIN` |
      | 71 | `LIGHT SNOW` |
      | 73 | `SNOW` |
      | 75 | `HEAVY SNOW` |
      | 77 | `SNOW GRAINS` |
      | 80, 81 | `SHOWERS` |
      | 82 | `VIOLENT SHOWERS` |
      | 85, 86 | `SNOW SHOWERS` |
      | 95 | `THUNDERSTORM` |
      | 96, 99 | `THUNDERSTORM WITH HAIL` |
      | anything else | `UNKNOWN` |
  - **`weather::feed(fetch: Arc<dyn Fetch>) -> Feed`:**
    - runs only while `weather` is wanted;
    - fetches the forecast every 15 min and air quality every 30 min (spec §12), on `spawn_blocking`;
    - after a failed fetch, retries after 60 s, doubling up to that cadence;
    - sets `stale = true` once the last good forecast is older than 30 min, two cadences;
    - refetches at once when the location changes;
    - publishes after each successful fetch and on each location change.
  - **Config validation:** `location: lat and lon must be set together`; `location.lat: must be -90..=90, got 95`; `location.lon: must be -180..=180, got 200`.
  - **Fake feed** (mockup values): 18 °C, code 95, H 22, L 14, HUM 88, wind 31 km/h; hours with rain 80/95/70/40/20/10 % and temperatures 18/17/17/16/15/15; rain peak 95 % at the second hour; AQI 42.
  - **Cost:** two HTTPS requests per 30 min. Record the RSS that ureq and rustls add, and the CPU per fetch.

- [ ] **Step 1: Write the fixtures and the failing tests**

The fixtures follow Open-Meteo's documented `timeformat=unixtime` response shape. `current.time` is `1792609200`, and the hourly series start at `1792609200` (`2026-10-21T19:00Z`).

```rust
#[test] fn urls_are_exact() { assert!(forecast_url(-23.55, -46.63).starts_with("https://api.open-meteo.com/v1/forecast?latitude=-23.55&longitude=-46.63&current=")); assert_eq!(air_url(-23.55, -46.63), "https://air-quality-api.open-meteo.com/v1/air-quality?latitude=-23.55&longitude=-46.63&current=us_aqi&timeformat=unixtime"); }
#[test] fn parses_current_today_and_six_hours() { /* now 1792610000 → temp 18.0, code 95, condition "THUNDERSTORM", humidity 88, wind 31.0, high 22.0, low 14.0, hours.len() 6, hours[0].at_s 1792612800 */ }
#[test] fn rain_peak_is_highest_then_earliest_within_twelve_hours() { }
#[test] fn rain_peak_is_none_when_dry() { }
#[test] fn wmo_codes_map_to_labels() { assert_eq!(condition(95), "THUNDERSTORM"); assert_eq!(condition(3), "OVERCAST"); assert_eq!(condition(42), "UNKNOWN"); }
#[test] fn parses_us_aqi() { assert_eq!(parse_air(AIR).unwrap(), 42); }
#[tokio::test(start_paused = true)] async fn fetches_only_while_wanted_and_on_cadence() { /* counting Fetch: unwanted 1 h → 0; wanted → forecast + air at once; +15 min → forecast; +30 min → forecast + air */ }
#[tokio::test(start_paused = true)] async fn failure_keeps_the_last_value_and_goes_stale_after_two_cadences() { }
#[tokio::test(start_paused = true)] async fn no_location_is_unavailable_without_fetching() { }
#[tokio::test(start_paused = true)] async fn a_location_change_refetches_at_once() { }
// config_load.rs
#[test] fn lat_without_lon_is_rejected() { assert_eq!(load_str_with_user("[location]\nlat = -23.55\n", "u").unwrap_err().message, "location: lat and lon must be set together"); }
#[test] fn lat_out_of_range_names_the_key() { assert_eq!(load_str_with_user("[location]\nlat = 95.0\nlon = 0.0\n", "u").unwrap_err().message, "location.lat: must be -90..=90, got 95"); }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test weather`
Expected: compile errors.

- [ ] **Step 3: Implement `http`, `weather`, the fake feed and the validation**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --test weather && cargo test -p ghostd --test config_load`
Expected: PASS (weather 10, config_load + 2).

- [ ] **Step 5: Live check and measurement**

This makes one request each to Open-Meteo. Run a dev daemon with `location = { lat = -23.55, lon = -46.63 }` and subscribe to `weather`.

Expected: a `weather` message with `"available":true`, plausible values, and an `aqi`. Record the fetch's wall and CPU time and the RSS delta.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghost-proto/src crates/ghostd
git commit -m "feat(ghostd): add the open-meteo weather and air quality feed"
```

---

### Task 10: Sky feed (sun and moon, computed locally)

**Files:**
- Create: `crates/ghostd/src/feeds/sky.rs` (unit tests inside)
- Modify: `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/{main.rs,fake.rs}`

**Interfaces:**
- Consumes: `Feed` (Task 2); `location.{lat, lon}`.
- Produces:
  - `SkySnapshot { available: bool, sunrise_s: Option<u64>, sunset_s: Option<u64>, moon_phase: f32, moon_illumination: f32, moon_name: String }`.
  - `sky::sun_times(lat: f64, lon: f64, date: (i32, u8, u8)) -> Option<(i64, i64)>` uses NOAA's sunrise equation with a zenith of 90.833°. It returns `None` for polar day or night. `date` is the machine's local calendar date **[call]**.
  - `sky::moon(at_s: i64) -> Moon { phase: f64, illumination: f64, name: &'static str }`:
    - `phase` is `((at_s − 947_182_440) / 86_400 / 29.530_588_853) mod 1`, from the reference new moon of 2000-01-06 18:14 UTC;
    - `illumination = (1 − cos 2πp) / 2`;
    - `name` is one of `NEW`, `WAXING CRESCENT`, `FIRST QUARTER`, `WAXING GIBBOUS`, `FULL`, `WANING GIBBOUS`, `LAST QUARTER`, `WANING CRESCENT`. The four principal names apply within ±1 day of their exact phase.
  - **Feed:** recomputes every 60 s (spec §12) while `sky` is wanted. `available = false` without lat/lon. The fake publishes fixed values.
  - **Cost:** microseconds per minute.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn london_midsummer() { let (rise, set) = sun_times(51.4769, -0.0005, (2026, 6, 21)).unwrap(); assert_close(rise, utc(2026, 6, 21, 3, 43), 120); assert_close(set, utc(2026, 6, 21, 20, 21), 120); }
#[test] fn svalbard_midnight_sun_has_no_sunrise() { assert_eq!(sun_times(78.22, 15.65, (2026, 6, 21)), None); }
#[test] fn eclipse_new_moon() { let m = moon(1_712_600_460); assert!(m.illumination < 0.02); assert_eq!(m.name, "NEW"); }
#[test] fn april_2024_full_moon() { let m = moon(1_713_916_140); assert!(m.illumination > 0.98); assert_eq!(m.name, "FULL"); }
```

The London times are the published 04:43 / 21:21 BST. The moon instants are the 2024-04-08 eclipse new moon (18:21 UTC) and the 2024-04-23 full moon (23:49 UTC).

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --lib -- sky`
Expected: compile errors.

- [ ] **Step 3: Implement `sky.rs` and wire the feed**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --lib -- sky`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src crates/ghostd/src
git commit -m "feat(ghostd): compute sunrise, sunset and moon phase locally"
```

---

### Task 11: INMET alert provider, started from `location.alerts`

**Files:**
- Create: `crates/ghost-alerts-inmet/{Cargo.toml,src/lib.rs,src/main.rs}`, `crates/ghost-alerts-inmet/tests/parse.rs`, `crates/ghost-alerts-inmet/tests/fixtures/sample.json`
- Modify: `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/{providers.rs,main.rs,fake.rs}`, `crates/ghostd/src/server/feed.rs` (reconcile against the effective providers), `crates/ghostd/src/config/load.rs`, `Cargo.toml` (`jiff`), `scripts/install.sh` (install the binary to `~/.local/bin`)
- Test: `crates/ghostd/tests/config_load.rs` (3 tests); a unit test in `providers.rs`

**Interfaces:**
- Consumes: `ClientMsg::{Provide, Publish}` and `topics::WEATHER_ALERTS` (ghost-proto); `ProviderSpec`, `supervise` and `reconcile` (Plan 1).
- Produces:
  - **Types:** `WeatherAlerts { source: String, updated_s: u64, alerts: Vec<WeatherAlert> }`; `WeatherAlert { id: String, title: String, severity: String, level: AlertLevel, start_s: u64, end_s: u64, risks: Vec<String> }`; `AlertLevel { Warn, Crit }` (snake_case).
  - **`parse_alerts(json: &str, area: &str) -> Result<Vec<WeatherAlert>, ParseError>`** reads the entries of `hoje`, then `futuro`, whose `geocodes` (comma-separated, trimmed) contain `area`. This is the user's `storm_alert.py` logic:
    - a duplicate `id` keeps its first occurrence;
    - `title` is `descricao` uppercased and `severity` is `severidade` uppercased; the wording is INMET's own, untranslated;
    - `level`: `Perigo Potencial` → Warn, `Perigo` and `Grande Perigo` → Crit (spec §5.2); anything else → Warn, logged **[call]**;
    - `start_s` and `end_s` come from `data_inicio` + `hora_inicio` and `data_fim` + `hora_fim`, read in `America/Sao_Paulo` with jiff (tzdb, in case daylight saving returns);
    - an entry whose times don't parse is skipped and logged;
    - the result is sorted by start.
  - `provide_line() -> String` returns exactly `{"t":"provide","proto":1,"name":"inmet","topics":["weather.alerts"]}` and a newline.
  - **Binary `ghost-alerts-inmet --area <code>`:**
    - blocking std I/O, no tokio;
    - connects to `$GHOST_SOCKET` and sends the provide line;
    - every 10 min (spec §12), GETs `https://apiprevmet3.inmet.gov.br/avisos/ativos` (15 s timeout) and publishes `weather.alerts` as `WeatherAlerts { source: "INMET", .. }`;
    - a failed fetch publishes nothing, so ghostd keeps the last value, and retries after 60 s, doubling to 10 min;
    - a socket write error exits 1, and the supervisor restarts it.
  - **`providers::effective_providers(config: &ResolvedConfig) -> Vec<ProviderSpec>`** returns `config.providers` and, when `location.alerts.provider` is non-empty, `ProviderSpec { name: "alerts-<provider>", exec: ["ghost-alerts-<provider>", "--area", <area>], restart: true }`. `supervise` and `reconcile` both use it, so other countries plug in with a binary of that name.
  - **Validation:**
    - `location.alerts.provider: "In Met" must match ^[a-z][a-z0-9-]*$`;
    - `location.alerts.area: must not be empty when a provider is set`;
    - `providers[0].name: "alerts-inmet" is reserved for location.alerts`.
  - **Fake mode** publishes `weather.alerts` with one alert: `TEMPESTADE`, `PERIGO POTENCIAL` (warn), from now + 1 h to now + 12 h. The mockup's box is amber.
  - **Cost:** one HTTPS GET per 10 min. Record the provider process's RSS.

- [ ] **Step 1: Check INMET's time format against one real response**

This is one GET, the same request the user's timer makes every 10 min.

Run: `curl -s -m 15 https://apiprevmet3.inmet.gov.br/avisos/ativos -o /tmp/inmet.json && uv run python -I -c "import json; d=json.load(open('/tmp/inmet.json')); a=(d['hoje']+d['futuro'])[0]; print({k: a[k] for k in ('data_inicio','hora_inicio','data_fim','hora_fim','severidade')}, repr(a['geocodes'][:40]))"`

Expected: dates `YYYY-MM-DD`, times `HH:MM` (or `HH:MM:SS`), and comma-separated geocodes. **If the shape differs, stop and report**, because the parse contract above assumes it. The recorded file is not committed.

- [ ] **Step 2: Write `sample.json` and the failing tests**

`sample.json` holds the recorded shape, without polygons:
- `hoje`: id 1 for `3550308` with `Perigo`, 2026-10-21 18:00 → 2026-10-22 06:00; id 2 for `3304557` only; id 3 with `Perigo Potencial` for `"3509502, 3550308"`; id 4 with `hora_inicio` `"18h"`;
- `futuro`: id 1 again.

```rust
#[test] fn keeps_only_alerts_for_the_area() { /* ids == ["1", "3"] */ }
#[test] fn geocode_lists_tolerate_spaces() { }
#[test] fn maps_inmet_severities() { /* "1" Crit, "3" Warn; "Grande Perigo" → Crit */ }
#[test] fn reads_brasilia_time() { /* id 1 start_s == 1_792_616_400 (2026-10-21T21:00Z) */ }
#[test] fn duplicate_ids_across_today_and_future_appear_once() { }
#[test] fn unparsable_time_skips_the_entry() { /* id 4 absent, others present */ }
#[test] fn provide_line_is_exact() { assert_eq!(provide_line(), "{\"t\":\"provide\",\"proto\":1,\"name\":\"inmet\",\"topics\":[\"weather.alerts\"]}\n"); }
// ghostd
#[test] fn alerts_provider_is_derived_from_location() { /* provider "inmet", area "3550308" → last spec { name "alerts-inmet", exec ["ghost-alerts-inmet","--area","3550308"], restart true } */ }
#[test] fn bad_alert_provider_names_the_key() { }
#[test] fn alert_area_required_with_provider() { }
#[test] fn reserved_provider_name_rejected() { }
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p ghost-alerts-inmet; cargo test -p ghostd --test config_load`
Expected: compile errors.

- [ ] **Step 4: Implement the crate, `effective_providers`, the validation and the install step**

- [ ] **Step 5: Run them and watch them pass**

Run: `cargo test -p ghost-alerts-inmet && cargo test -p ghostd --test config_load && cargo test -p ghostd --lib -- providers`
Expected: PASS (parse 7, config_load + 3, providers + 1).

- [ ] **Step 6: Live check**

Run a dev ghostd with `location.alerts = { provider = "inmet", area = "3550308" }` and `PATH=target/debug:$PATH`.

Expected:
- the log shows `provider connected` for `inmet`;
- a `weather.alerts` subscriber gets `{"source":"INMET",…}` (`alerts` may be empty);
- killing the provider brings it back after 1 s.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghost-alerts-inmet crates/ghost-proto/src crates/ghostd scripts/install.sh
git commit -m "feat(alerts): add the inmet weather alert provider"
```

---

### Task 12: Weather widget

**Files:**
- Create: `widgets/weather/{manifest.toml,Widget.qml}`, `shell/kit/GhostKit/lib/weather.mjs`, `shell/tests/weather.test.mjs`
- Modify: `shell/kit/GhostKit/Fmt.qml` (re-export `weather.mjs`, which the clock and Plan 3's bar weather item share)

**Interfaces:**
- Consumes:
  - topics `weather`, `sky`, `weather.alerts` (Tasks 9–11);
  - `AlertBox`, `Chip`, `Panel { header: false }`, `FrameClock.epochSecond` (Task 6);
  - `countdown`, `levelAbove` (Task 1);
  - `Theme.aqiWarn` and `aqiCrit`.
- Produces:
  - **`weather.mjs`:**
    - `activeAlerts(alerts, nowS)` drops alerts with `end_s <= nowS`, so an alert expires even after its provider died. Crit sorts before warn, then the earliest `start_s`.
    - `alertLine(alert, source)` returns `"<source> · <title> · <severity>"`.
    - `hourLabel(atS, offsetMin)` returns e.g. `21H`.
    - `hiLo(w)` returns `H 22° · L 14°`.
    - `detailLine(w, offsetMin)` returns `HUM 88% · WIND 31 KM/H · RAIN 95% AT 22H`; the rain part is omitted without a peak.
    - `skyLine(sky, offsetMin)` returns `SUN 05:17 / 18:52 · MOON 64% WAXING GIBBOUS`; the sun part is omitted when `sunrise_s` is null.
  - **Manifest:** `id = "weather"`, `name = "Weather"`, `kinds = ["panel"]`, `requires = ["weather", "sky", "weather.alerts"]`, `animates = true`, no settings.
  - **Widget** (the MONITOR band's `.band-wx` look: a headerless panel with columns `auto | 1fr | 140`):
    - left: the temperature at 34 px with a 13 px dim `°C`, the condition in `hi` (tracking .12 em) and `hiLo`;
    - middle:
      - an `AlertBox` for `activeAlerts[0]` (level from `level`, chip `alert`, `trailing` = its `countdown`, holding `acquireSeconds()` while shown);
      - `detailLine` in `dim`;
      - `skyLine` in `dim`, followed by `AQI <n>` coloured by `levelAbove(aqi, aqiWarn, aqiCrit)`;
      - only the most urgent alert is shown **[call]**;
    - right: six hour cells (a segmented rain column 14 high with 2 px cells and 1 px gaps, the temperature, the hour label).
    - It renders **nothing** (no panel) while `weather.available` is false and no alert is active (spec §13: weather stays hidden until a location is set; ruling R51 keeps alerts visible without a forecast). Values are `dim` while `stale`.

- [ ] **Step 1: Write the failing tests**

```js
test('expired alerts drop and crit sorts first', () => assert.deepEqual(activeAlerts([{ id: 'a', level: 'warn', start_s: 1, end_s: 99 }, { id: 'b', level: 'crit', start_s: 5, end_s: 99 }, { id: 'c', level: 'crit', start_s: 0, end_s: 10 }], 10).map(a => a.id), ['b', 'a']));
test('alert line', () => assert.equal(alertLine({ title: 'TEMPESTADE', severity: 'PERIGO' }, 'INMET'), 'INMET · TEMPESTADE · PERIGO'));
test('detail line', () => assert.equal(detailLine({ humidity_pct: 88, wind_kmh: 31.2, rain_peak: { pct: 95, at_s: 1792630800 } }, -180), 'HUM 88% · WIND 31 KM/H · RAIN 95% AT 22H'));
test('sky line', () => assert.equal(skyLine({ sunrise_s: 1792570620, sunset_s: 1792619520, moon_illumination: 0.64, moon_name: 'WAXING GIBBOUS' }, -180), 'SUN 05:17 / 18:52 · MOON 64% WAXING GIBBOUS'));
test('hour labels use local time', () => assert.equal(hourLabel(1792616400, -180), '18H'));
test('hi and lo', () => assert.equal(hiLo({ high_c: 22.4, low_c: 13.6 }), 'H 22° · L 14°'));
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Implement `weather.mjs`, the manifest and the widget**

- [ ] **Step 4: Run the tests and a smoke check**

`/tmp/ghost-weather-test.toml` gives `v1` a single `weather` slot sized 104, and sets a location.

Run: `node --test shell/tests/*.test.mjs && GHOST_CONFIG=/tmp/ghost-weather-test.toml scripts/qml-smoke.sh`
Expected: PASS and `smoke: ok`.

Then check a `GHOST_SNAPSHOT` (fake data):
- the amber `TEMPESTADE` alert box, with its countdown ticking in held steps;
- the six hour cells;
- the sun, moon and `AQI 42` line in `dim`.

- [ ] **Step 5: Commit**

```bash
git add widgets/weather shell/kit/GhostKit shell/tests/weather.test.mjs
git commit -m "feat(widgets): add the weather widget with alerts, forecast, sun, moon and aqi"
```

---

### Task 13: Clock widget

**Files:**
- Create: `widgets/clock/{manifest.toml,Widget.qml}`

**Interfaces:**
- Consumes:
  - `host` (Task 2);
  - `FrameClock.acquireSeconds` / `epochSecond`, `Panel { header; framed }`, `Digits` (Plan 1, Task 6);
  - `Fmt.{uptime, utcOffset, dow, ddmmyy, countdown}`, `Fmt.{activeAlerts, alertLine, hiLo, detailLine}` (Tasks 1 and 12);
  - `Data.want` (Task 5).
- Produces:
  - **Manifest:** `id = "clock"`, `name = "Clock"`, `kinds = ["panel"]`, `requires = ["host"]`, `animates = false`. Settings:
    - `style`: enum `hero | band | compact`, default `"hero"`;
    - `weather`: bool, default `true`.
  - **`hero`** (V1, frameless: `framed: false`; mockup `.timehero`), a centred column with 14 px gaps:
    1. `HH:MM` at 118 px bold in `hi` (tracking −.02 em), with the seconds at 34 px in `signal`, top-aligned 10 px to the right;
    2. the date line: 11 px, tracking .26 em, `dim`, 18 px gaps: `TUE  DATE 21.10.26  UPTIME 02:14:07  SÃO PAULO UTC−3`. Labels are `dim` and values are `text`. The city comes from `location.city` uppercased, and its segment is omitted when the city is empty.
    3. if `weather` is set: the weather line while `weather.available` (26 px temperature, the condition, then `hiLo` and `HUM` in `dim`), and an `AlertBox` for `activeAlerts[0]` with its countdown whenever alerts are configured and active, forecast or not (ruling R51).
  - **`band`** (V2, headerless; mockup `.timeband`): 74 px / 24 px digits on the left. On the right: the date line without the city, the weather line with `detailLine`, and the alert box.
  - **`compact`** (the MONITOR band, headerless; mockup `.band-clock`): 44 px / 16 px digits over a `Kv` row of DATE, DAY and UPTIME. It never shows weather.
  - Everything updates on `FrameClock.epochSecond`. Uptime is `epochSecond − host.boot_time_s`, shown as `--` until `host` arrives.
  - With `weather` set, the widget wants `weather` and `weather.alerts` while it exists (`Data.want`), and releases them on destruction.
  - The digits never scramble.

- [ ] **Step 1: Write a three-style test layout and watch the smoke test fail**

`/tmp/ghost-clock-test.toml` gives `v1` a column of `clock` slots sized 262 `{style="hero"}`, 150 `{style="band"}` and 104 `{style="compact"}`, and sets a location and city.

Run: `GHOST_CONFIG=/tmp/ghost-clock-test.toml scripts/qml-smoke.sh`
Expected: FAIL with `no widget named "clock" is installed`.

- [ ] **Step 2: Implement the widget**

- [ ] **Step 3: Run the smoke check and check it visually at two scales**

Run: `GHOST_CONFIG=/tmp/ghost-clock-test.toml scripts/qml-smoke.sh`
Expected: `smoke: ok`.

Take `GHOST_SNAPSHOT`s at the native `u` and at `GHOST_FORCE_U=1.0`. Check:
- the geometry matches the mockup's hero, band and band-clock;
- the digits don't move from second to second;
- nothing overflows;
- with `location` removed, the weather lines disappear.

- [ ] **Step 4: Commit**

```bash
git add widgets/clock
git commit -m "feat(widgets): add the clock widget in hero, band and compact styles"
```

---

### Task 14: Spectrum feed (PipeWire capture and FFT)

**Files:**
- Create: `crates/ghostd/src/feeds/spectrum/{mod.rs,analyzer.rs,capture.rs}` (analyzer unit tests inside)
- Modify: `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/{main.rs,fake.rs}`, `Cargo.toml`, `crates/ghostd/Cargo.toml` (`pipewire`, `realfft`)

**Interfaces:**
- Consumes: `Feed`, `wanted` and `until_wanted` (Task 2).
- Produces:
  - `SpectrumSnapshot { bands: Vec<u8> }`: 32 values from 0 to 255.
  - **Analyzer:** `Analyzer::new(rate: u32)` and `Analyzer::push(&mut self, interleaved: &[f32], channels: usize, out: &mut Vec<[u8; 32]>)`:
    - it emits one frame per `rate / 30` input frames;
    - each frame is an `FFT_SIZE = 2048` Hann-windowed real FFT (realfft) of the mono mix of the latest 2048 samples;
    - the spectrum is split into `BANDS = 32` log-spaced bands (the mockup's 32 columns) from `LOW_HZ = 50` to `HIGH_HZ = 16_000`;
    - a band's value is its largest bin magnitude in dBFS, mapped from `FLOOR_DB = −60` → 0 up to 0 dB → 255;
    - a band rises at once and falls to no less than `FALL = 0.8` × its previous value per frame.
  - `band_of(hz: f32) -> usize` gives a frequency's band.
  - **Capture:** a dedicated thread runs a PipeWire main loop with one capture stream on the default sink's monitor:
    - properties `stream.capture.sink = "true"`, `media.type = "Audio"`, `media.category = "Capture"`, `node.name = "ghostd-spectrum"`, `application.name = "ghostd"`;
    - format F32LE, stereo, 48 kHz;
    - it starts when `audio.spectrum` becomes wanted, and stops when it isn't (stream disconnected, loop quit, thread joined);
    - frames reach the feed through a bounded channel of capacity 2 (newest wins), and the feed publishes them;
    - without PipeWire, it logs once per demand and publishes nothing.
  - **Fake mode** publishes deterministic sine-shaped bands at 30 Hz while wanted.
  - **Cost:** expected ≈ 0.1–0.3 % of a core while capturing **[estimate, to measure]**; 0 otherwise.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn silence_is_dark() { /* 48 000 zero frames → 30 outputs, all 0 */ }
#[test] fn thirty_frames_per_second() { /* 48 000 frames in 1024-frame chunks → exactly 30 outputs */ }
#[test] fn a_1khz_tone_peaks_in_its_band() { /* full-scale 1 kHz sine → argmax == band_of(1000.0), value ≥ 200 */ }
#[test] fn stereo_mixes_to_mono() { /* tone only on L at 2× == mono at 1×, ±1 */ }
#[test] fn bands_fall_gradually() { /* tone then silence → the next frame's band ≥ ⌊0.8 × previous⌋ */ }
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --lib -- spectrum`
Expected: compile errors.

- [ ] **Step 3: Implement the analyzer, the capture thread and the feed**

- [ ] **Step 4: Run them and watch them pass**

Run: `cargo test -p ghostd --lib -- spectrum`
Expected: PASS (5 tests).

- [ ] **Step 5: Live check and measurement (music playing)**

Subscribe to `audio.spectrum` for 10 s. Expected: about 300 lines, with non-zero bands.

Unsubscribe, then run `pw-cli ls Node | rg ghostd-spectrum`. Expected: no output within 2 s.

Measure the daemon over 60 s while subscribed, then unsubscribed.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghost-proto/src crates/ghostd
git commit -m "feat(ghostd): add the audio spectrum feed"
```

---

### Task 15: Media widget (MPRIS, idle states, spectrum)

**Files:**
- Create: `widgets/media/{manifest.toml,Widget.qml,media.mjs}`, `shell/kit/GhostKit/SegBars.qml`, `shell/kit/GhostKit/shaders/{segbars.frag,duotone.frag}`, `shell/tests/media.test.mjs`
- Modify: `shell/kit/GhostKit/qmldir`

**Interfaces:**
- Consumes:
  - Quickshell `Mpris`, `DesktopEntries` and `Io.FileView`;
  - `Data.topic("audio.spectrum")` and `Data.want` (Tasks 5 and 14);
  - `FrameClock.acquireFast`, `acquireSeconds` and `epochSecond`, `ScrambleText`, `Chip`, `Glyph`, `Panel` (Task 6);
  - `SegMeter` (Plan 1);
  - `Fmt.ago` (Task 1);
  - `Data.config.bar.icons`.
- Produces:
  - **`media.mjs`:**
    - `pickPlayer(players, lastActiveId) -> number` returns the index of a playing player (preferring `lastActiveId`), else of `lastActiveId`, else 0, else −1. `players` is `[{ id, playing }]`.
    - `mediaView(player, remembered, nowMs) -> { mode, title, artist, artUrl, identity, positionS, lengthS, status }`.
      - `player` is a plain object using `MprisPlayer`'s property names, or null: `{ playbackState: 'playing'|'paused'|'stopped', trackTitle, trackArtist, trackArtUrl, identity, desktopEntry, position, length }`. The widget maps the enum to those strings.
      - `mode` is `playing | paused | stopped`, and `status` is one of:
      - `NOW PLAYING`;
      - `PAUSED`;
      - `STOPPED · LAST PLAYED <ago> AGO` (spec §6.3), with the remembered track shown dimmed;
      - `STOPPED` when nothing is remembered.
    - `rememberTrack(player, nowMs)` returns `{ title, artist, artUrl, identity, desktopEntry, atMs }`, or null without a title.
    - `progress(positionS, lengthS)` returns a fraction, or null when the length is ≤ 0 or unsupported.
    - `mmss(s)` returns e.g. `1:44` (mockup).
  - **Manifest:** `id = "media"`, `name = "Media"`, `kinds = ["panel"]`, `requires = []`, `animates = true`. Settings:
    - `style`: enum `stack | strip | compact`, default `"stack"`;
    - `audio`: bool, default `true` (Task 16).
  - **Widget:**
    - **Layout per style:**
      - `stack` (V1, mockup `.media.stack`): a header with tag `media` and the identity uppercased (e.g. `SPOTIFY`) as meta;
      - `strip` (V2, headerless, 120 high; mockup `mediaStrip`);
      - `compact` (the MONITOR band, headerless; mockup `bandMedia`).
    - **Status row:** a `Chip` (`play`, `pause` or `stop`) and the status text (8.5 px, tracking .14 em, `dim`).
    - **Title and artist:** the title at 18 px bold in `hi` (15 px in compact) and the artist at 10 px in `text`. Both are `ScrambleText` keyed `media:<title>|<artist>`, so a new track scrambles once (spec §5.3). Both elide.
    - **Art:** the `trackArtUrl` image at 124 px (stack, centred), 84 px (strip) or 44 px (compact), with a 1 px `hair` border, `sourceSize` = displayed size × `u`, loaded asynchronously and cached.
      - With `bar.icons == "mono"` it is tinted by `duotone.frag` (luminance → signal, alpha kept); it is shown as-is in `colour`.
      - Without art, it shows the mockup's ring pattern.
    - **Spectrum** (stack and strip only): `SegBars` with 32 columns, 3 px cells and 2 px gaps (mockup), sampled from `audio.spectrum` on FrameClock ticks.
      - While playing **and** visible, the widget holds `Data.want("audio.spectrum")` and `FrameClock.acquireFast()` (spec: 30 fps only while playing and visible).
      - Paused or stopped, it releases both, and every cell is unlit with no motion.
    - **Progress:** `mmss(pos)`, then a thin 4 px `SegMeter`, then `mmss(len)`. It is hidden when `progress` is null. While playing, `player.positionChanged()` is called on each `epochSecond`.
    - **Controls:** `prev`, `play` or `pause`, and `next` as 13 px Glyphs in `hi`, 20 px apart. Prev and next are `dim` and inert when unavailable. Pressing play:
      - with a player present and stopped, calls `player.play()`;
      - with no player, calls `DesktopEntries.byId(remembered.desktopEntry)?.execute()` (spec: "play resumes it or opens the player");
      - with an unknown entry, does nothing.
    - **Persistence:** the remembered track lives in `$XDG_STATE_HOME/ghost-shell/media.json` (default `~/.local/state/ghost-shell/media.json`), through `FileView`. It is written on each track change and when the player disappears.
  - **`SegBars { levels: list<real>; rows; cellH; gap }`** (GhostKit, public) is a single `ShaderEffect`. Up to 32 levels are packed into 8 `vector4d` uniforms, coloured per `cellColor` (unlit signal 6 %, the top lit cell `hi`, the rest signal 35–85 %). It replaces ~300 `Rectangle`s for the one widget that redraws at 30 fps.

- [ ] **Step 1: Write the failing tests**

```js
test('prefers a playing player, then the last active one', () => { assert.equal(pickPlayer([{ id: 'a', playing: false }, { id: 'b', playing: true }], 'a'), 1); assert.equal(pickPlayer([{ id: 'a', playing: false }, { id: 'b', playing: false }], 'b'), 1); assert.equal(pickPlayer([], 'b'), -1); });
test('paused keeps the track and says PAUSED', () => assert.equal(mediaView({ playbackState: 'paused', trackTitle: 'Derezzed', trackArtist: 'Daft Punk', position: 38, length: 104 }, null, 0).status, 'PAUSED'));
test('a vanished player shows the remembered track as stopped with its age', () => { const v = mediaView(null, { title: 'Derezzed', artist: 'Daft Punk', atMs: 0 }, 12 * 60000); assert.equal(v.status, 'STOPPED · LAST PLAYED 12M AGO'); assert.equal(v.title, 'Derezzed'); });
test('nothing remembered is plain STOPPED', () => assert.equal(mediaView(null, null, 0).status, 'STOPPED'));
test('no length hides progress', () => { assert.equal(progress(30, 0), null); assert.equal(progress(52, 104), 0.5); });
test('mmss', () => { assert.equal(mmss(104), '1:44'); assert.equal(mmss(38.9), '0:38'); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Implement `media.mjs`, `SegBars`, the shaders, the manifest and the widget**

- [ ] **Step 4: Run the tests and a private-bus smoke check**

On a private bus there is no MPRIS player. `/tmp/ghost-media-test.toml` gives `v1` the three styles.

Run: `node --test shell/tests/*.test.mjs && make shaders && GHOST_CONFIG=/tmp/ghost-media-test.toml GHOST_SMOKE_BUS=private scripts/qml-smoke.sh`
Expected: PASS and `smoke: ok`, with the stopped state shown.

- [ ] **Step 5: Live check and measurement on the user's session (music playing)**

Snapshot and check:
- the title scrambles once;
- the spectrum moves;
- the progress fills.

Then:
- **Pause:** the bars fall to dark in about 1.4 s (0.7 s at `spectrum_fps = 30`), then the frame rate drops to 15 frames/s or 0, whichever the rest of the view needs (`QSG_RENDER_TIMING`).
- **Quit the player:** `STOPPED · LAST PLAYED NOW`. Play reopens the player.
- Measure shell + daemon over 60 s while playing. Expected: ≤ 2.00 % combined.

- [ ] **Step 6: Commit**

```bash
git add widgets/media shell/kit/GhostKit shell/tests/media.test.mjs
git commit -m "feat(widgets): add the media widget with idle states and spectrum"
```

---

### Task 16: Audio rows and MIC LIVE

**Files:**
- Create: `widgets/media/audio.mjs`, `shell/tests/audio.test.mjs`
- Modify: `widgets/media/Widget.qml`

**Interfaces:**
- Consumes: Quickshell `Pipewire` (`defaultAudioSink`, `defaultAudioSource`, `nodes`), `PwObjectTracker`, `PwNodePeakMonitor`, `Quickshell.processId`; `SegMeter`, `AlertBox` with chip `live`, and FrameClock ticks.
- Produces:
  - **`audio.mjs`:**
    - `recordingApps(nodes, ownPids) -> string[]`. `nodes` are `[{ isStream, isSink, isAudio, props }]`. A node is an app recording when all of these hold:
      - it is an audio stream that is not a sink (`isStream && !isSink && isAudio`);
      - it is not capturing a sink monitor (`props["stream.capture.sink"] !== "true"`, which excludes ghostd's spectrum);
      - its `props["application.process.id"]` is not in `ownPids` (which excludes the shell's own peak meter).

      The name is `application.name` (else `node.name`), uppercased, unique, in order. Mute is ignored: a muted mic with a recording app is still live.
    - `micLevelEnabled(apps, visible, muted) -> bool` is true only while another app records (`apps` non-empty), the widget is visible and the source isn't muted (decision 2).
    - `micAlertText(apps)` returns `MIC LIVE · DISCORD IS RECORDING` (spec) for one app, and `MIC LIVE · DISCORD, OBS ARE RECORDING` for several.
    - `volumeText(volume, muted)` returns `64%` or `MUTED`.
  - **Rows** (styles `stack` and `strip` while `audio` is set; `compact` shows only the MIC LIVE alert, since the 104 px band has no room for two rows):
    - `OUT  <default sink description>  [SegMeter volume]  64%|MUTED`.
    - **While no app records:** `IN  <default source description>  MUTED|--`. There is no level meter, and the shell opens no capture stream on the microphone.
    - **While any app records:**
      - the row becomes `IN  <default source description>  [SegMeter level]  LIVE`, with `LIVE` in `warn`;
      - the level comes from a `PwNodePeakMonitor` on the default source whose `enabled` is `micLevelEnabled(apps, visible, muted)`, copied on FrameClock ticks (spec: ~15 Hz, only while visible);
      - a warn `AlertBox` with chip `live` shows `micAlertText(apps)`.

      The peak monitor's own stream is excluded by `ownPids`, so it never counts as an app recording and never keeps itself running: it ends when the last other app stops.
    - The control row shows `VOL <out %>` (mockup).
    - Clicking a row's value toggles that device's mute **[call]**.
  - `ownPids = [Quickshell.processId]`. Nodes are tracked with `PwObjectTracker` if `properties` stays empty until bound.
  - The spectrum (Task 14) captures the sink monitor inside ghostd, not the microphone, so its capture path is unaffected.

- [ ] **Step 1: Write the failing tests**

```js
const node = (props, extra = {}) => ({ isStream: true, isSink: false, isAudio: true, props, ...extra });
test('a capture stream is recording', () => assert.deepEqual(recordingApps([node({ 'application.name': 'Discord', 'application.process.id': '900' })], [42]), ['DISCORD']));
test('sink monitors and our own streams are not', () => assert.deepEqual(recordingApps([node({ 'application.name': 'ghostd', 'stream.capture.sink': 'true' }), node({ 'application.name': 'quickshell', 'application.process.id': '42' })], [42]), []));
test('playback streams and devices are not', () => assert.deepEqual(recordingApps([node({ 'application.name': 'Spotify' }, { isSink: true }), node({ 'node.name': 'alsa_input' }, { isStream: false })], []), []));
test('the level meter runs only while another app records', () => { assert.equal(micLevelEnabled(['DISCORD'], true, false), true); assert.equal(micLevelEnabled([], true, false), false); assert.equal(micLevelEnabled(['DISCORD'], false, false), false); assert.equal(micLevelEnabled(['DISCORD'], true, true), false); });
test('our own meter never keeps itself running', () => assert.equal(micLevelEnabled(recordingApps([node({ 'application.name': 'quickshell', 'application.process.id': '42' })], [42]), true, false), false));
test('alert text names one or several apps', () => { assert.equal(micAlertText(['DISCORD']), 'MIC LIVE · DISCORD IS RECORDING'); assert.equal(micAlertText(['DISCORD', 'OBS']), 'MIC LIVE · DISCORD, OBS ARE RECORDING'); });
test('volume text', () => { assert.equal(volumeText(0.64, false), '64%'); assert.equal(volumeText(0.64, true), 'MUTED'); });
```

- [ ] **Step 2: Run them and watch them fail**

Run: `node --test shell/tests/*.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Implement `audio.mjs` and the rows**

- [ ] **Step 4: Run the tests and the smoke check**

Run: `node --test shell/tests/*.test.mjs && GHOST_CONFIG=/tmp/ghost-media-test.toml GHOST_SMOKE_BUS=private scripts/qml-smoke.sh`
Expected: PASS and `smoke: ok`.

- [ ] **Step 5: Live check on the user's session**

1. With the dashboard visible and no recorder running, check that:
   - no MIC LIVE appears;
   - the IN row shows no level;
   - `pw-cli ls Node` lists no stream whose `application.process.id` is the shell's pid.
2. Run `pw-record /tmp/gs-mic.wav` for 5 s. Expected: `IN` reads `LIVE` in warn, its level moves, and `MIC LIVE · PW-RECORD IS RECORDING` blinks.
3. Mute the mic while it records. Expected: still LIVE.
4. Stop the recorder. Expected: within 2 s the level and the alert disappear, and `pw-cli ls Node` again lists no stream from the shell.

Delete `/tmp/gs-mic.wav` afterwards.

- [ ] **Step 6: Commit**

```bash
git add widgets/media shell/tests/audio.test.mjs
git commit -m "feat(widgets): add audio output and input rows with the mic live alert"
```

---

### Task 17: Default V2 and MONITOR views

**Files:**
- Modify: `crates/ghost-proto/src/config.rs`, `crates/ghostd/src/config/default.toml`, `crates/ghostd/tests/config_load.rs`, `shell/tests/layout.test.mjs` (default-view fixtures)

**Interfaces:**
- Consumes: the `clock`, `weather` and `media` widgets (Tasks 12, 13, 15); `placeholder` (Plan 1).
- Produces:
  - `Dashboard::default()`: `main_views = ["v1", "v2"]`, `monitor_view = "monitor"`, `default_view = "v1"` (spec §13).
  - **`v1`** is Plan 1's tree with two changes:
    - `placeholder{Clock}` 262 → `clock` 262 `{style="hero"}`;
    - `placeholder{Media}` 290 → `media` 290 `{style="stack"}`.

    `placeholder{Notifications}` (Plan 3) and `placeholder{Uplink}` 300 (Plan 2b) stay.
  - **`v2`** (spec §6.2), sizes in design px, no size meaning flex:
    - `row`:
      - `column` 300: `processor` 250, `graphics` 204, `memory`;
      - `column`:
        - `clock` 150 `{style="band"}`;
        - tabs `[placeholder{label="Uplink"} title "UPLINK", placeholder{label="Silicon"} title "SILICON", placeholder{label="Process tree"} title "PROCESS TREE"]`, default 0;
        - `media` 120 `{style="strip"}`;
      - `column` 300: `placeholder{Notifications}` 290, `placeholder{Network}` 164, `storage`.
  - **`monitor`** (spec §6.2):
    - `column`:
      - `row` 104: `clock` 250 `{style="compact"}`, `weather`, `media` 340 `{style="compact"}`;
      - `row`:
        - `placeholder{label="Telemetry"}`;
        - `column` 340: `placeholder{label="Process tree"}`, `sensors` 184.
  - `default.toml` spells out all three views. Its comments keep Plan 1's style.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn default_views_and_dashboard() { let c = load_str_with_user("", "u").unwrap(); assert_eq!(c.dashboard.main_views, ["v1", "v2"]); assert_eq!(c.dashboard.monitor_view, "monitor"); assert!(c.layout.views.contains_key("v2") && c.layout.views.contains_key("monitor")); }
// default_file_equals_builtin_defaults (Plan 1) must keep passing with the new trees.
```

```js
test('default v2 geometry', () => { const r = computeRects(DEFAULT_V2_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12); assert.deepEqual(pick(r, 'clock'), { x: 324, y: 42, w: 632, h: 150 }); assert.equal(r.find((x) => x.tabs).h, 372); });
test('default monitor geometry', () => { const r = computeRects(DEFAULT_MONITOR_FOR_TEST, { x: 12, y: 42, w: 1256, h: 666 }, 12); assert.deepEqual(pick(r, 'weather'), { x: 274, y: 42, w: 642, h: 104 }); assert.deepEqual(pick(r, 'media'), { x: 928, y: 42, w: 340, h: 104 }); assert.deepEqual(pick(r, 'sensors'), { x: 928, y: 524, w: 340, h: 184 }); });
```

Also update Plan 1's `DEFAULT_V1_FOR_TEST` copy to the new V1.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p ghostd --test config_load; node --test shell/tests/*.test.mjs`
Expected: FAIL.

- [ ] **Step 3: Write the trees in `config.rs` and `default.toml`**

- [ ] **Step 4: Run the tests and the smoke check with no allow-list**

Run: `cargo test -p ghostd --test config_load && node --test shell/tests/*.test.mjs && scripts/qml-smoke.sh && GHOST_SMOKE_BUS=private GHOST_SMOKE_ACTIONS="monitor swap" scripts/qml-smoke.sh`
Expected: PASS, then `smoke: ok` twice with zero slot errors. The second run logs `view v1 -> monitor (640 ms)` and `view monitor -> v2 (640 ms)`.

- [ ] **Step 5: Check it visually**

Snapshot each default view (fake data) at the native `u`. Compare V1, V2 and MONITOR with the mockup's layers `L-v1`, `L-v2` and `L-mon`. Nothing overflows.

- [ ] **Step 6: Commit**

```bash
git add crates/ghost-proto/src/config.rs crates/ghostd/src/config/default.toml crates/ghostd/tests/config_load.rs shell/tests/layout.test.mjs
git commit -m "feat(config): default to the v1, v2 and monitor views"
```

---

### Task 18: Install, measure the three budget states, and accept Part 2a

**Files:**
- Create: `docs/superpowers/notes/2026-10-21-plan2a-acceptance.md`
- Modify: `README.md` (Performance: every new feed, the provider, and the three states)

**Interfaces:**
- Consumes: everything above; Plan 1's `scripts/install.sh` and user units.

- [ ] **Step 1: Install, with the user's consent at each item**

1. Run `scripts/install.sh`.
2. **Ask the user**, separately:
   1. to enable the KWin script (`scripts/kwin-script.sh enable`);
   2. to let ghostd register Meta+M and Meta+Shift+M;
   3. to add their machine's settings to `~/.config/ghost-shell/config.toml`: `display.model = "M27Q"` and `[location]` `city = "São Paulo"`, `lat = -23.55`, `lon = -46.63`, `alerts = { provider = "inmet", area = "3550308" }`.
3. Start the units.

- [ ] **Step 2: Measure each state for 60 s (shell + daemon combined; provider RSS listed separately)**

| State | How | Expected |
|---|---|---|
| Animating | V1 (the dead-drive alert animates) | ≤ 2.00 %, RSS ≤ 307200 KiB |
| Playing | V1 with music (spectrum at the default 15 fps; 30 fps, the user's own setting, is recorded but not a pass mark) | ≤ 2.00 % |
| Still | MONITOR with no active alert, nothing playing, no recorder | ≤ 1.00 % (if an INMET alert is active, note it and re-measure later) |
| Covered | `scripts/measure-covered.sh 60`, then the user maximises a real window | 0 frames, ≤ 0.50 % |

- [ ] **Step 3: Behaviour checks with the user**

- On the empty desktop, Meta+M and Meta+Shift+M switch views with the print. With a focused window they do nothing. After Meta+D they work.
- Covering and uncovering the desktop logs `dashboard covered` and `dashboard visible`, and the dashboard returns with its tab selections.
- Real weather, sun, moon and AQI appear. An active INMET alert, if any, counts down.
- Media: pausing lets the spectrum fall, then it stays dark. Quitting the player gives `STOPPED · LAST PLAYED`. Play reopens it.
- MIC LIVE appears with a recorder, and never from the dashboard itself. The shell opens no microphone stream while nothing records.

- [ ] **Step 4: Write the note and update README**

Record:
- every measurement;
- the deviations.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/notes/2026-10-21-plan2a-acceptance.md README.md
git commit -m "docs: record plan 2a acceptance"
```

---

## Plan 2b outline: machine depth (to be written after 2a ships)

1. **`net` feed:**
   - rates from `/proc/net/dev` at 1 Hz, for the default-route interface plus totals;
   - links from NetworkManager over D-Bus, event-driven (LAN, OpenVPN `tun0`);
   - Tailscale's local API over `/run/tailscale/tailscaled.sock` (mode 0666 here) every 10 s **[call]**;
   - ICMP echo to 1.1.1.1 every 5 s (spec), on an unprivileged ping socket (`net.ipv4.ping_group_range = 0 2147483647` here);
   - history keys `net_up` and `net_down`.
2. **`network` widget:** V2's 164 px panel. Also the V1 Uplink scope's traffic graph, up/down/ping chips and VPN line, which makes V1's right column a `tabs` + network composite.
3. **Telemetry lanes:**
   - history keys `core0`–`core15` (SMT pairs averaged), `mem_used`, `disk_read`, `disk_write`, `temp_cpu`, `temp_gpu`, `temp_nvme`;
   - one `telemetry-lane` widget with a `metric` setting;
   - an optional per-split `gutter` in the layout schema for the mockup's 6 px lane gaps **[call]**;
   - MONITOR's `Telemetry` placeholder becomes six lanes.
4. **`procs` built-in topic:** the full table (pid, ppid, name, cpu %, rss) every 10 s (spec: 5–20 s), from the same `stat` scan. It also **splits `top` out of `mem`**, so Plan 3's bar memory item doesn't drag in the process scan; the memory widget migrates.
5. **`process-tree` widget:**
   - a radial pscircle layout with kernel threads collapsed into one node;
   - dots sized by RSS and coloured by CPU (mockup cut-offs 1, 5 and 20 %);
   - nodes collapse and expand on click (spec);
   - edges are one `Shape`, rebuilt only on new data;
   - no pulses: the mockup has none.
6. **Uplink data:**
   - a netlink `sock_diag` dump of established TCP and UDP every 5 s (spec);
   - an offline GeoIP lookup with the `maxminddb` crate against DB-IP IP-to-City Lite (CC BY 4.0, no account) **[call]**;
   - the top 8 endpoints **[call]**.
7. **Globe:**
   - Natural Earth 1:50m land polygons (public domain), rasterised into an equirectangular mask texture;
   - the dot sphere drawn by a fragment shader, rotating on `FrameClock.elapsedMs`;
   - arcs drawn as `Shape` polylines;
   - the São Paulo marker checked on Brazil's south-east coast.
8. **`uplink` widget,** and V1's Uplink scope with tabs `UPLINK | SILICON | TREE`.
9. **RAPL package power:**
   - an **ask-the-user** step to install a tmpfiles rule with sudo: `z /sys/class/powercap/intel-rapl:0/energy_uj 0440 root <user's group>`;
   - `cpu.package_w: Option<f32>`, hidden when unreadable (spec).
10. **`silicon` widget** per §6.3, with the header `PKG <W> · SYS ≈ <W>`.
11. **Drive wear and power-on hours** from UDisks2 NVMe and ATA properties every 10 min (spec), reading cached properties only and never calling `SmartUpdate`, so sleeping disks stay asleep **[call]**. **Device batteries** from UPower (DualSense) and KDE Connect's battery over D-Bus every 60 s go into the sensors widget, with the battery thresholds.
12. **Acceptance:** the SILICON tab's still state ≤ 1 %.

Open questions for 2b:
- **Endpoint labels:** reverse DNS, an offline ASN/organisation name, or the owning process?
- **The land mask:** commit the generated texture, or generate it at install time?
- **The RAPL rule:** group-readable as above, or world-readable? Either way it re-opens the PLATYPUS power side channel (CVE-2020-8694) to whoever can read it.
- **Tailscale peers on the globe:** `100.x` addresses have no GeoIP entry. Omit them?

## Plan 2c outline: the greeting (spec §7)

1. **`session` feed in ghostd:**
   - logind `PrepareForSleep`, the session's `Lock`/`Unlock`, and `org.freedesktop.ScreenSaver.ActiveChanged`;
   - publishes `{ locked, sleeping, event: login|unlock|wake, event_seq }`;
   - a wake and the unlock that follows count as one event (spec).
2. **The greeting overlay:**
   - an Overlay-layer surface, mapped as soon as the session locks or prepares for sleep, behind the locker, so windows never flash in first;
   - the §7 sequence: a 120/300/900 ms power-on, a 24-step dot print of ~0.9 s, typing at 62 ms ±40 % with +60 % on spaces, a 0.55 s burn-in, the subtitle, a ~1.5 s hold with the refresh band, then a 30-step scan pass of ~0.8 s that reveals the windows or the dashboard;
   - step timers run only during the sequence;
   - no glow: the mockup's shadows are dropped;
   - `FirstWrite.reset()` runs before the reveal.
3. **Skip:** any key or click jumps to the end. The overlay takes the keyboard exclusively while shown, so the skipping key is consumed (spec).
4. **Login:** the first greeting frame appears within 500 ms of shell start (§15), without waiting for ghostd.
5. **Subtitle:** `SESSION nnnn` is the logind session id zero-padded to 4 digits **[call]**. `N NEW SIGNALS` needs the notification server, so it is omitted until Plan 3 **[call]**.
6. **Acceptance with the user:** lock and unlock, suspend and resume. Write a handover note for project 2 (the lock screen).

## Hand-offs to Plan 3

- The dashboard's Notifications widget, the INMET and dead-drive notifications (§11), and `N NEW SIGNALS` in the greeting.
- The bar's `barRequires` input to `subscriptionFor` (Task 5).
- The view chip calls `Views.toggleMonitor()` and `Views.swap()`. The chip reads `MAIN`/`MONITOR` from `Views.isMonitor`.
- Bar items read `weather` through `Fmt.activeAlerts`, never animated (spec §5.4).

## Decisions (2026-10-21)

The user's answers to the draft's questions:

1. **Split:** Parts 2a, 2b and 2c as drafted. The dashboard Notifications list moves to Plan 3.
2. **Mic level:** the IN level shows **only while another app is recording**. Otherwise the IN row shows the device and mute state, with no level and no capture stream. The shell's own capture stream never counts as an app recording (Task 16).
3. **Translucent windows:** no allowlist. A translucent window counts as covering (Task 4). An app allowlist is a possible later addition if the user reports translucent windows over the desktop.
4. **Shortcuts:** config.toml is authoritative. ghostd registers them with KGlobalAccel, and rebinding them in System Settings doesn't persist.
5. **Album art:** tinted to the signal colour in `mono` icon style, original in `colour`.
6. **Compact MONITOR player:** no OUT/IN rows, only the MIC LIVE alert.

The proposed values are all approved:
1. retry after a failed fetch (weather and INMET): 60 s, doubling up to the normal cadence;
2. weather goes stale after 30 min without a good forecast;
3. the rain-peak window is the next 12 h;
4. spectrum DSP: FFT 2048 with a Hann window, 50 Hz to 16 kHz, a −60 dB floor, falling to ×0.8 per frame;
5. principal moon phases are named within ±1 day;
6. chips `pause` 一時停止 / ポーズ and `stop` 停止 / ストップ;
7. the remembered track lives in `$XDG_STATE_HOME/ghost-shell/media.json`;
8. D-Bus names `org.ghostshell.Ghostd`, `/org/ghostshell/Desktop`, `org.ghostshell.Desktop1`, with KGlobalAccel component `ghost-shell` and actions `monitor` and `swap`;
9. alert providers are binaries named `ghost-alerts-<provider>`, supervised as `alerts-<provider>`;
10. before the first KWin report, the dashboard renders and shortcuts are off;
11. `ago()` buckets are `NOW`, then `M`, `H`, `D`;
12. INMET wording is passed through untranslated.

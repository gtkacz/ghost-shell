# ghost-shell agents (the AI chip) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One bar widget that shows, for Claude Code and Codex CLI, the weekly usage meters, the live session count and how many sessions wait on the user, with a popup for both windows and every session, fed entirely from files the CLIs already write.

**Architecture:** Two new built-in topics (`agents.usage`, `agents.sessions`) with snapshot types in `ghost_proto`. Two in-daemon feeds under `crates/ghostd/src/feeds/agents/` read Claude's session registry and a captured status line, and Codex's rollouts, session index and a per-session state file. Two `ghostd` subcommands are the taps that write the capture and the state files: `claude-statusline-tap` wraps the user's status line, `codex-hook` is a Codex user hook. A separate installer wires both into the user's Claude and Codex settings with backups. The widget `widgets/agents` is direction A of the mockup and reads only the two topics.

**Tech Stack:** Rust (serde, serde_json, notify, tokio, nix — all already dependencies of ghostd), Quickshell QML with GhostKit, Node tests for the `.mjs` rules, Python (`uv run`, stdlib `json`) for the settings edits, bash for the installer.

**Spec:** `docs/superpowers/specs/2026-10-23-ghost-shell-agents-design.md`; mockup `docs/superpowers/specs/mockups/2026-10-23-ghost-shell-agents-bar.html`, direction A (lines 62–96).

## Global Constraints

- The daemon never calls Anthropic or OpenAI. Every number is read from disk.
- The status-line capture is written by temp file plus rename, never in place (eighteen sessions render concurrently).
- Both taps always exit 0 with empty stdout. `codex-hook` finishes well inside its 2 s timeout and takes no decision. The status-line tap forwards the inner command's stdin and stdout byte for byte.
- Hook and status-line commands use the absolute path `$HOME/.local/bin/ghostd`, where `scripts/install.sh` puts the binary.
- Usage is read from the first configured Claude config dir only; sessions from all of them.
- The widget is in no bar preset. It shows only where a user's `[layout.bar]` names it (user, 2026-10-24).
- Agents stay headless: `scripts/qml-smoke.sh` and `scripts/headless-kwin.sh` only; nothing opens on the user's display.
- After acceptance, the change is installed on the live desktop with `scripts/install.sh`, and every measurement goes in README.md's "Performance" section.
- Config keys and their defaults: `[agents] providers = ["claude", "codex"]`, `claude_config_dirs = ["~/.claude"]`, `codex_home = "~/.codex"`; `[thresholds] usage_warn = 60`, `usage_crit = 85`.
- Commit style: `<type>(<scope>): <imperative subject>` as in `git log`, no attribution trailers.

## Review Focus

Inputs the spec implies but names no test for; each has its test added to the owning task.

1. A Codex hook `session_id` with path separators or dots (`../x`) must not escape the sessions dir: the hook writes nothing and still exits 0 (Task 6).
2. A Claude session file whose `/proc/<pid>/stat` cannot be read (gone or permission) counts as dead and is dropped (Task 4).
3. A status-line JSON without `rate_limits` (free plan, or before the first API response) publishes Claude as `available = false` with empty windows, not a crash and not zeros (Task 2).
4. A rollout tail cut mid-line at the 256 KiB boundary still yields the last complete `token_count` line (Task 3).
5. A `session_index.jsonl` with one malformed line among good ones still names every other session (Task 3).

---

### Task 1: Protocol and config

**Files:**
- Modify: `crates/ghost-proto/src/lib.rs` (topics), `crates/ghost-proto/src/snapshot.rs`, `crates/ghost-proto/src/config.rs`
- Modify: `crates/ghostd/src/config/default.toml`, `crates/ghostd/src/config/load.rs:252-281`
- Test: `crates/ghostd/tests/config_load.rs`, `crates/ghostd/tests/config_bar.rs:9-18`, `crates/ghostd/tests/feeds.rs:18-35`

**Interfaces:**
- Produces (in `ghost_proto`, all `Debug, Clone, PartialEq, Serialize, Deserialize`, the snapshots also `Default`):
  ```rust
  pub mod topics { pub const AGENTS_USAGE: &str = "agents.usage"; pub const AGENTS_SESSIONS: &str = "agents.sessions"; /* BUILTIN grows to 25 */ }
  pub struct AgentsUsage { pub providers: Vec<ProviderUsage> }
  pub struct ProviderUsage { pub id: String, pub label: String, pub plan: Option<String>, pub available: bool, pub reason: Option<String>, pub read_at_s: u64, pub windows: Vec<UsageWindow> }
  pub struct UsageWindow { pub length_s: u64, pub used_pct: f64, pub resets_at_s: Option<u64> }
  pub struct AgentsSessions { pub sessions: Vec<AgentSession> }
  pub struct AgentSession { pub provider: String, pub id: String, pub name: String, pub cwd: String, pub state: AgentState, pub waiting_for: Option<String>, pub since_s: u64, pub pid: Option<u32>, pub tmux: Option<String> }
  #[serde(rename_all = "snake_case")] pub enum AgentState { Busy, Idle, Waiting }
  #[serde(default, deny_unknown_fields)] pub struct Agents { pub providers: Vec<AgentProvider>, pub claude_config_dirs: Vec<String>, pub codex_home: String }
  #[serde(try_from = "String", rename_all = "lowercase")] pub enum AgentProvider { Claude, Codex }
  // ResolvedConfig gains `pub agents: Agents`; Thresholds gains `pub usage_warn: f32` (60.0) and `pub usage_crit: f32` (85.0).
  ```
  `AgentProvider::try_from` fails with `agents.providers: {name:?} is not a provider; "claude" and "codex" are` (the `LauncherStyle` pattern at `config.rs:394-411`).

- [ ] **Step 1: Write the failing tests**

In `crates/ghostd/tests/config_load.rs`:

```rust
#[test]
fn agents_defaults_name_both_providers() {
    let c = load_str_with_user("", "u").unwrap();
    assert_eq!(c.agents.providers, [AgentProvider::Claude, AgentProvider::Codex]);
    assert_eq!(c.agents.claude_config_dirs, ["~/.claude"]);
    assert_eq!(c.agents.codex_home, "~/.codex");
    assert_eq!((c.thresholds.usage_warn, c.thresholds.usage_crit), (60.0, 85.0));
}

#[test]
fn an_unknown_agent_provider_is_rejected() {
    assert_eq!(
        rejected("[agents]\nproviders = [\"gemini\"]\n"),
        "agents.providers: \"gemini\" is not a provider; \"claude\" and \"codex\" are"
    );
}

#[test]
fn a_duplicate_agent_provider_is_rejected() {
    assert_eq!(
        rejected("[agents]\nproviders = [\"claude\", \"claude\"]\n"),
        "agents.providers[1]: duplicate provider \"claude\""
    );
}

#[test]
fn usage_thresholds_must_be_ordered() {
    assert_eq!(
        rejected("[thresholds]\nusage_warn = 90\nusage_crit = 85\n"),
        "thresholds: usage_warn (90) must be below usage_crit (85)"
    );
}
```

In `crates/ghostd/tests/config_bar.rs`: add `"agents"` to `BAR_WIDGETS` (now 9) and

```rust
#[test]
fn agents_is_in_no_preset_but_a_user_bar_can_place_it() {
    for layout in [BarLayout::A, BarLayout::B, BarLayout::C] {
        assert!(!widgets(&resolve_config("", layout).bar.right).contains(&"agents"));
    }
    let resolved = resolve_config(
        "[layout.bar]\nright = [{ widget = \"status\" }, { widget = \"agents\" }, { widget = \"tray\" }]\n",
        BarLayout::A,
    );
    assert_eq!(widgets(&resolved.bar.right), ["status", "agents", "tray"]);
    assert!(errors_at(&resolved, "layout.bar").is_empty());
}
```

In `crates/ghostd/tests/feeds.rs`: add `topics::AGENTS_USAGE` and `topics::AGENTS_SESSIONS` to `NEW_BUILTINS` (now 19).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ghostd --test config_load --test config_bar --test feeds`
Expected: compile errors on `AgentProvider`, `agents`, `usage_warn`.

- [ ] **Step 3: Add the types, topics, defaults and validation**

`default.toml`: the `[agents]` block from the spec's Configuration section, placed after `[uplink.labels]`; `usage_warn = 60` and `usage_crit = 85` after `aqi_crit` with the comment extended: "and LLM usage once it reaches the limit". `validate_thresholds` gains the `usage_warn`/`usage_crit` `check_below`. New `validate_agents(agents: &Agents)` rejects duplicates at `agents.providers[{index}]`, called from `validate`. The commented `[layout.bar]` example in `default.toml` gains `{ widget = "agents" }` between the status and tray lines, with a comment line "`agents` is the LLM usage and sessions chip; it is in no preset". The presets in `bar.rs` are not touched. `topics::BUILTIN` lists both new topics; the `default_file_equals_builtin_defaults` test must still pass.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ghostd --test config_load --test config_bar --test feeds && cargo test -p ghost-proto`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto crates/ghostd/src/config crates/ghostd/tests/config_load.rs crates/ghostd/tests/config_bar.rs crates/ghostd/tests/feeds.rs
git commit -m "feat(proto): add the agents topics and config"
```

---

### Task 2: Claude parsers

**Files:**
- Create: `crates/ghostd/src/feeds/agents/mod.rs` (module declarations and the shared `window_as_of` only, for now), `crates/ghostd/src/feeds/agents/claude.rs`
- Create fixtures: `crates/ghostd/tests/fixtures/agents/claude-session-waiting.json`, `claude-session-daemon.json`, `claude-statusline.json`, `claude-statusline-no-limits.json`, `claude-settings-tapped.json`, `claude-settings-hud.json`
- Modify: `crates/ghostd/src/feeds/mod.rs` (`pub mod agents;`)
- Test: `crates/ghostd/tests/agents.rs` (new)

**Interfaces:**
- Produces:
  ```rust
  // feeds/agents/mod.rs
  pub const FIVE_HOURS_S: u64 = 5 * 3600;
  pub const SEVEN_DAYS_S: u64 = 7 * 24 * 3600;
  /// A window as the widget should see it at `now_s`: a reset at or before now zeroes it and drops the reset.
  pub fn window_as_of(length_s: u64, used_pct: f64, resets_at_s: Option<u64>, now_s: u64) -> UsageWindow;
  pub fn unavailable(id: &str, label: &str, reason: &str) -> ProviderUsage;

  // feeds/agents/claude.rs
  pub const ID: &str = "claude";
  pub const LABEL: &str = "Claude Code";
  pub const TAP_COMMAND_SUFFIX: &str = "ghostd claude-statusline-tap";
  pub struct ClaudeSession { pub session: AgentSession, pub proc_start: u64 }
  /// `None` for an unparseable file and for the `daemon` and `daemon-worker` kinds.
  pub fn parse_session(json: &str) -> Option<ClaudeSession>;
  pub fn parse_statusline(json: &str, now_s: u64, read_at_s: u64) -> ProviderUsage;
  /// Whether `statusLine.command` of a settings.json ends with the tap.
  pub fn tap_installed(settings_json: &str) -> bool;
  ```
  Mapping: `status` `busy → Busy`, `idle | shell → Idle`, `waiting → Waiting` with `waiting_for` copied; `since_s = statusUpdatedAt / 1000`; `name` falls back to `<cwd basename>-<first 8 of sessionId>` when empty; `pid` is `Some(pid)`; `tmux` copied when present. A status-line without `rate_limits` is `unavailable(ID, LABEL, "no reading yet")`; a key missing inside `rate_limits` is `window_as_of(len, 0.0, None, now_s)`.

- [ ] **Step 1: Write the fixtures**

Author them from the field lists in the spec (names and paths anonymised: user `u`, project `proj`). `claude-session-waiting.json` has `pid: 4242, procStart: 123456, sessionId: "0123456789abcdef-…", cwd: "/home/u/Codes/proj", name: "proj-68", kind: "interactive", status: "waiting", waitingFor: "permission prompt", statusUpdatedAt: 1792797000000, tmux: "unsnooze-6:@12.%12"`. `claude-session-daemon.json` is the same with `kind: "daemon"`. `claude-statusline.json` has `rate_limits: { five_hour: { used_percentage: 42, resets_at: 1792805040 }, seven_day: { used_percentage: 61, resets_at: 1793091600 } }` plus a `model` and `cwd` key. `claude-statusline-no-limits.json` has the same without `rate_limits`. `claude-settings-tapped.json` has `statusLine: { type: "command", command: "/home/u/.local/bin/ghostd claude-statusline-tap" }`; `claude-settings-hud.json` a bun one-liner.

- [ ] **Step 2: Write the failing tests**

In `crates/ghostd/tests/agents.rs`:

```rust
const NOW_S: u64 = 1_792_797_010;

#[test]
fn a_waiting_claude_session_maps_to_waiting_with_its_reason() {
    let parsed = claude::parse_session(include_str!("fixtures/agents/claude-session-waiting.json")).unwrap();
    assert_eq!(parsed.proc_start, 123_456);
    let s = parsed.session;
    assert_eq!((s.provider.as_str(), s.pid, s.state), ("claude", Some(4242), AgentState::Waiting));
    assert_eq!(s.waiting_for.as_deref(), Some("permission prompt"));
    assert_eq!((s.name.as_str(), s.cwd.as_str(), s.since_s), ("proj-68", "/home/u/Codes/proj", 1_792_797_000));
    assert_eq!(s.tmux.as_deref(), Some("unsnooze-6:@12.%12"));
}

#[test]
fn daemon_kinds_and_garbage_are_not_sessions() {
    assert!(claude::parse_session(include_str!("fixtures/agents/claude-session-daemon.json")).is_none());
    assert!(claude::parse_session("{not json").is_none());
}

#[test]
fn shell_status_reads_as_idle_and_an_empty_name_is_built() {
    let json = include_str!("fixtures/agents/claude-session-waiting.json")
        .replace("\"waiting\"", "\"shell\"").replace("\"proj-68\"", "\"\"");
    let s = claude::parse_session(&json).unwrap().session;
    assert_eq!(s.state, AgentState::Idle);
    assert_eq!(s.name, "proj-01234567");
}

#[test]
fn the_statusline_gives_both_windows() {
    let usage = claude::parse_statusline(include_str!("fixtures/agents/claude-statusline.json"), NOW_S, 7);
    assert!(usage.available && usage.reason.is_none() && usage.plan.is_none());
    assert_eq!(usage.read_at_s, 7);
    assert_eq!(usage.windows, [
        UsageWindow { length_s: FIVE_HOURS_S, used_pct: 42.0, resets_at_s: Some(1_792_805_040) },
        UsageWindow { length_s: SEVEN_DAYS_S, used_pct: 61.0, resets_at_s: Some(1_793_091_600) },
    ]);
}

#[test]
fn a_passed_or_dropped_window_reads_zero_with_no_reset() {
    // five_hour's reset is in the past; seven_day is dropped from the JSON.
    let json = r#"{"rate_limits":{"five_hour":{"used_percentage":42,"resets_at":1792797010}}}"#;
    let usage = claude::parse_statusline(json, NOW_S, 0);
    assert_eq!(usage.windows, [
        UsageWindow { length_s: FIVE_HOURS_S, used_pct: 0.0, resets_at_s: None },
        UsageWindow { length_s: SEVEN_DAYS_S, used_pct: 0.0, resets_at_s: None },
    ]);
}

#[test]
fn a_statusline_without_limits_is_unavailable_not_zero() {
    let usage = claude::parse_statusline(include_str!("fixtures/agents/claude-statusline-no-limits.json"), NOW_S, 0);
    assert!(!usage.available && usage.windows.is_empty());
    assert_eq!(usage.reason.as_deref(), Some("no reading yet"));
    assert!(!claude::parse_statusline("nope", NOW_S, 0).available);
}

#[test]
fn the_tap_is_recognised_by_its_command_suffix() {
    assert!(claude::tap_installed(include_str!("fixtures/agents/claude-settings-tapped.json")));
    assert!(!claude::tap_installed(include_str!("fixtures/agents/claude-settings-hud.json")));
    assert!(!claude::tap_installed("{}"));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ghostd --test agents`
Expected: FAIL, `ghostd::feeds::agents` not found.

- [ ] **Step 4: Implement `claude.rs` and the shared helpers in `mod.rs`**

Deserialize with private `#[derive(Deserialize)]` structs and `#[serde(rename_all = "camelCase")]`, as `tailscale.rs` does. `window_as_of` treats `resets_at_s <= now_s` as passed.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ghostd --test agents`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ghostd/src/feeds crates/ghostd/tests/agents.rs crates/ghostd/tests/fixtures/agents
git commit -m "feat(agents): parse Claude's session files, status line and tap setting"
```

---

### Task 3: Codex parsers

**Files:**
- Create: `crates/ghostd/src/feeds/agents/codex.rs`
- Create fixtures: `crates/ghostd/tests/fixtures/agents/codex-rollout-tail.jsonl` (a dozen `event_msg` lines, two of them `token_count` with `rate_limits`, the last one being the newest), `codex-session-index.jsonl` (three ids, one id repeated with a newer `thread_name`, one malformed line in the middle)
- Test: `crates/ghostd/tests/agents.rs`

**Interfaces:**
- Produces:
  ```rust
  pub const ID: &str = "codex";
  pub const LABEL: &str = "Codex CLI";
  pub const TAIL_BYTES: u64 = 256 * 1024;
  pub const SESSION_INDEX_FILE: &str = "session_index.jsonl";
  pub const ROLLOUTS_SUBDIR: &str = "sessions";
  /// What `ghostd codex-hook` writes and the sessions feed reads, one file per session id.
  #[derive(Serialize, Deserialize)] pub struct SessionFile { pub state: AgentState, pub waiting_for: Option<String>, pub cwd: String, pub since_s: u64, pub pid: Option<u32>, pub proc_start: Option<u64> }
  pub enum HookEffect { Set { state: AgentState, waiting_for: Option<&'static str> }, Remove }
  /// `None` for an event the hook does not track.
  pub fn hook_effect(event_name: &str) -> Option<HookEffect>;
  /// The last complete `token_count` line of `tail`; a partial first line is skipped.
  pub fn parse_rollout_tail(tail: &[u8], now_s: u64, read_at_s: u64) -> ProviderUsage;
  /// id → thread_name, the last line per id winning; malformed lines skipped.
  pub fn thread_names(index: &str) -> HashMap<String, String>;
  pub fn session(id: &str, file: &SessionFile, names: &HashMap<String, String>) -> AgentSession;
  /// The newest `rollout-*.jsonl` under `<codex_home>/sessions`, by mtime, looking in the last two day dirs by name.
  pub fn newest_rollout(codex_home: &Path) -> Option<PathBuf>;
  /// `$XDG_RUNTIME_DIR/ghost-shell/codex-sessions`, or `/run/user/<uid>/…` when the variable is unset.
  pub fn sessions_dir() -> PathBuf;
  ```
  Window mapping: `primary → length_s = window_minutes * 60` (300 → 18000), `secondary` likewise (10080 → 604800), `used_pct = used_percent`, `resets_at_s = Some(resets_at)`, through `window_as_of`; `plan = plan_type`. Event map: `SessionStart | UserPromptSubmit | PreToolUse | PostToolUse → Busy`, `PermissionRequest → Waiting("permission prompt")`, `Stop | Interrupt → Idle`, `SessionEnd → Remove`. Name: `names[id]` else `<cwd basename>-<first 8 of id>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn the_newest_token_count_line_gives_both_codex_windows() {
    let usage = codex::parse_rollout_tail(include_bytes!("fixtures/agents/codex-rollout-tail.jsonl"), NOW_S, 9);
    assert!(usage.available);
    assert_eq!(usage.plan.as_deref(), Some("plus"));
    assert_eq!(usage.windows[0], UsageWindow { length_s: FIVE_HOURS_S, used_pct: 0.0, resets_at_s: Some(1_792_814_950) });
    assert_eq!(usage.windows[1], UsageWindow { length_s: SEVEN_DAYS_S, used_pct: 14.0, resets_at_s: Some(1_793_018_780) });
}

#[test]
fn a_tail_cut_mid_line_still_finds_the_last_complete_reading() {
    let whole = include_bytes!("fixtures/agents/codex-rollout-tail.jsonl");
    let cut = &whole[whole.len() / 3..];
    assert_eq!(codex::parse_rollout_tail(cut, NOW_S, 0).windows, codex::parse_rollout_tail(whole, NOW_S, 0).windows);
    assert!(!codex::parse_rollout_tail(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\"}}\n", NOW_S, 0).available);
}

#[test]
fn thread_names_take_the_last_line_per_id_and_skip_garbage() {
    let names = codex::thread_names(include_str!("fixtures/agents/codex-session-index.jsonl"));
    assert_eq!(names.len(), 3);
    assert_eq!(names["aaaaaaaa-0000"], "AGENTS.md draft");
}

#[test]
fn a_codex_session_is_named_from_the_index_or_its_cwd() {
    let names = codex::thread_names(include_str!("fixtures/agents/codex-session-index.jsonl"));
    let file = codex::SessionFile { state: AgentState::Busy, waiting_for: None, cwd: "/home/u/Codes/content-platform".into(), since_s: 5, pid: Some(7), proc_start: Some(8) };
    assert_eq!(codex::session("aaaaaaaa-0000", &file, &names).name, "AGENTS.md draft");
    let unnamed = codex::session("zzzzzzzz-9999-1234", &file, &names);
    assert_eq!((unnamed.name.as_str(), unnamed.provider.as_str(), unnamed.pid), ("content-platform-zzzzzzzz", "codex", Some(7)));
}

#[test]
fn hook_events_map_to_states() {
    use codex::HookEffect::*;
    assert!(matches!(codex::hook_effect("PermissionRequest"), Some(Set { state: AgentState::Waiting, waiting_for: Some("permission prompt") })));
    assert!(matches!(codex::hook_effect("PostToolUse"), Some(Set { state: AgentState::Busy, waiting_for: None })));
    assert!(matches!(codex::hook_effect("Interrupt"), Some(Set { state: AgentState::Idle, .. })));
    assert!(matches!(codex::hook_effect("SessionEnd"), Some(Remove)));
    assert!(codex::hook_effect("PreCompact").is_none());
}

#[test]
fn the_newest_rollout_is_found_across_day_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("sessions/2026/10/22/rollout-a.jsonl");
    let new = dir.path().join("sessions/2026/10/23/rollout-b.jsonl");
    for (path, mtime) in [(&old, 1_700_000_000), (&new, 1_700_000_100)] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
        File::open(path).unwrap().set_modified(UNIX_EPOCH + Duration::from_secs(mtime)).unwrap();
    }
    assert_eq!(codex::newest_rollout(dir.path()), Some(new));
    assert_eq!(codex::newest_rollout(&dir.path().join("nowhere")), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ghostd --test agents`
Expected: FAIL, `codex` module not found.

- [ ] **Step 3: Implement `codex.rs`**

`parse_rollout_tail` splits on `\n`, drops the first chunk when `tail` does not start at a line (callers always pass a tail; to keep it simple, drop the first chunk whenever the slice does not begin with `{`), and walks the lines from the end for one that parses as `event_msg` with `payload.type == "token_count"` and a `rate_limits` object. `sessions_dir` uses `nix::unistd::getuid()` for the fallback.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ghostd --test agents`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/agents crates/ghostd/tests/agents.rs crates/ghostd/tests/fixtures/agents
git commit -m "feat(agents): parse Codex rollouts, the session index and hook state files"
```

---

### Task 4: Process identity and the sessions feed

**Files:**
- Modify: `crates/ghostd/src/collect/procs.rs` (expose one identity read over the existing `parse_stat`), `crates/ghostd/src/collect/mod.rs` (re-export)
- Modify: `crates/ghostd/src/feeds/agents/mod.rs`
- Test: `crates/ghostd/tests/agents.rs`

**Interfaces:**
- Consumes: `parse_stat` (`procs.rs:343`), `SysRoot`, `config::DEBOUNCE`, `until_wanted`/`until_unwanted`, the Task 2 and 3 parsers.
- Produces:
  ```rust
  // collect/procs.rs
  pub struct ProcIdentity { pub name: String, pub ppid: u32, pub start_time: u64 }
  /// `None` when the process is gone or its stat cannot be read.
  pub fn identity(root: &SysRoot, pid: u32) -> Option<ProcIdentity>;

  // feeds/agents/mod.rs
  pub const SWEEP_EVERY: Duration = Duration::from_secs(30);
  pub struct SessionPaths { pub claude_sessions_dirs: Vec<PathBuf>, pub codex_sessions_dir: PathBuf, pub codex_home: PathBuf }
  /// Reads `[agents]` from the live config once at start; `~` expands to `$HOME`.
  pub fn sessions_feed(root: SysRoot) -> Feed;
  pub fn sessions_feed_at(root: SysRoot, providers: Vec<AgentProvider>, paths: SessionPaths, clock: WallClock) -> Feed;
  /// One pass: every live session, sorted waiting → busy → idle, newest `since_s` first within a group.
  pub fn read_sessions(root: &SysRoot, providers: &[AgentProvider], paths: &SessionPaths) -> AgentsSessions;
  ```
  Liveness: `identity(root, pid)` is `Some` and its `start_time == proc_start`; a Codex file with `pid: None` is live until removed by the hook, or by the feed once `since_s` is 24 h behind the clock. The feed publishes an empty snapshot at start, then only while wanted: one `notify` watcher on every dir in `paths` (missing dirs are created for the Codex one and skipped with a `warn!` for Claude), `DEBOUNCE` after a burst, a `SWEEP_EVERY` tick, and a publish only when the snapshot changed.

- [ ] **Step 1: Write the failing tests**

A `Sandbox` in `tests/agents.rs` like `tests/recent.rs`'s: a temp dir with `root/proc/<pid>/stat` lines (field 22 = start time, as in `main.rs`'s `write_machine`), `claude/sessions/`, `run/codex-sessions/`, `codex/session_index.jsonl`; a `start(providers)` that builds `FeedContext` the way `tests/session.rs` does, with demand containing `agents.sessions`, and a `next()` with a 2 s timeout.

```rust
#[test]
fn identity_reads_name_parent_and_start_time() {
    let dir = tempfile::tempdir().unwrap();
    write_proc(dir.path(), "proc/42/stat", "42 (codex) S 7 0 0 0 0 0 0 0 0 0 1 2 0 0 0 0 0 0 555 0 8 0\n");
    let id = identity(&SysRoot::new(dir.path()), 42).unwrap();
    assert_eq!((id.name.as_str(), id.ppid, id.start_time), ("codex", 7, 555));
    assert!(identity(&SysRoot::new(dir.path()), 43).is_none());
}

#[tokio::test]
async fn live_sessions_are_sorted_and_dead_ones_dropped() {
    let sb = Sandbox::new();
    sb.claude_session(1, "idle", 100, 10);            // pid 1, proc start 100, since 10
    sb.claude_session(2, "busy", 200, 20);
    sb.claude_session(3, "waiting", 300, 5);
    sb.claude_session(4, "busy", 400, 30);
    sb.proc(1, "claude", 100); sb.proc(2, "claude", 200); sb.proc(3, "claude", 300);
    sb.proc(4, "claude", 999);                          // pid reused: start time differs
    sb.codex_session("abc", AgentState::Busy, 25, None); // no pid: trusted
    let mut feed = sb.start(&[AgentProvider::Claude, AgentProvider::Codex]).await;
    assert!(feed.next().await.sessions.is_empty(), "an empty snapshot comes first");
    let snapshot = feed.next().await;
    let order: Vec<(String, AgentState)> = snapshot.sessions.iter().map(|s| (s.id.clone(), s.state)).collect();
    assert_eq!(order, [("3".into(), Waiting), ("abc".into(), Busy), ("2".into(), Busy), ("1".into(), Idle)]);
}

#[tokio::test]
async fn a_rewritten_session_file_publishes_and_an_unreadable_stat_is_dead() {
    let sb = Sandbox::new();
    sb.claude_session(1, "busy", 100, 10); sb.proc(1, "claude", 100);
    let mut feed = sb.start(&[AgentProvider::Claude]).await;
    feed.next().await; feed.next().await;
    sb.claude_session(1, "waiting", 100, 11);
    assert_eq!(feed.next().await.sessions[0].state, AgentState::Waiting);
    fs::remove_file(sb.root().join("proc/1/stat")).unwrap();
    sb.claude_session(1, "busy", 100, 12);
    assert!(feed.next().await.sessions.is_empty());
}

#[tokio::test]
async fn only_enabled_providers_are_read() {
    let sb = Sandbox::new();
    sb.claude_session(1, "busy", 100, 10); sb.proc(1, "claude", 100);
    sb.codex_session("abc", AgentState::Busy, 25, None);
    let mut feed = sb.start(&[AgentProvider::Codex]).await;
    feed.next().await;
    assert_eq!(feed.next().await.sessions.iter().map(|s| s.provider.as_str()).collect::<Vec<_>>(), ["codex"]);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ghostd --test agents`
Expected: FAIL to compile on `identity`, `sessions_feed_at`.

- [ ] **Step 3: Implement `identity` and the sessions feed**

`identity` reuses `parse_stat`; the `ProcIdentity` name comes from the comm bytes, lossily. The feed loop is `recent.rs`'s `run`/`watch` shape with a `tokio::select!` over the debounced events and an `interval(SWEEP_EVERY)`. The watcher's callback ignores `EventKind::Access`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ghostd --test agents`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/collect crates/ghostd/src/feeds/agents crates/ghostd/tests/agents.rs
git commit -m "feat(agents): publish the live Claude and Codex sessions"
```

---

### Task 5: The usage feed

**Files:**
- Modify: `crates/ghostd/src/feeds/agents/mod.rs`
- Test: `crates/ghostd/tests/agents.rs`

**Interfaces:**
- Consumes: Task 2 `parse_statusline`, `tap_installed`; Task 3 `parse_rollout_tail`, `newest_rollout`, `TAIL_BYTES`.
- Produces:
  ```rust
  pub const CAPTURE_FILE: &str = "claude-statusline.json";
  /// `$XDG_STATE_HOME/ghost-shell/claude-statusline.json`, or `~/.local/state/…` when unset.
  pub fn capture_path() -> PathBuf;
  pub struct UsagePaths { pub claude_capture: PathBuf, pub claude_settings: PathBuf, pub codex_home: PathBuf }
  pub fn usage_feed() -> Feed;
  pub fn usage_feed_at(providers: Vec<AgentProvider>, paths: UsagePaths, clock: WallClock) -> Feed;
  pub fn read_usage(providers: &[AgentProvider], paths: &UsagePaths, now_s: u64) -> AgentsUsage;
  ```
  `read_usage`: Claude is `unavailable(…, "tap")` while the settings file exists and `tap_installed` is false; otherwise the capture is parsed with `read_at_s` = its mtime, a missing capture being `unavailable(…, "no reading yet")`. Codex reads the last `TAIL_BYTES` of `newest_rollout`, mtime as `read_at_s`; none is `unavailable(…, "no reading yet")`. Watches: the capture's directory and the settings file's directory (non-recursive), and `<codex_home>/sessions` recursively; a Codex event names a `rollout-*.jsonl`, which is then the file read. Publish on start (empty providers, so the widget renders), then on `DEBOUNCE`d change when the snapshot differs, while wanted.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn a_new_capture_publishes_claude_usage() {
    let sb = Sandbox::new();
    sb.write_settings(include_str!("fixtures/agents/claude-settings-tapped.json"));
    let mut feed = sb.start_usage(&[AgentProvider::Claude]).await;
    feed.next().await;
    assert_eq!(feed.next().await.providers[0].reason.as_deref(), Some("no reading yet"));
    sb.replace_capture(include_str!("fixtures/agents/claude-statusline.json"));
    let usage = feed.next().await.providers.remove(0);
    assert!(usage.available);
    assert_eq!(usage.windows[1].used_pct, 61.0);
}

#[tokio::test]
async fn a_clobbered_status_line_marks_claude_tap_missing() {
    let sb = Sandbox::new();
    sb.write_settings(include_str!("fixtures/agents/claude-settings-tapped.json"));
    sb.replace_capture(include_str!("fixtures/agents/claude-statusline.json"));
    let mut feed = sb.start_usage(&[AgentProvider::Claude]).await;
    feed.next().await; feed.next().await;
    sb.write_settings(include_str!("fixtures/agents/claude-settings-hud.json"));
    let usage = feed.next().await.providers.remove(0);
    assert!(!usage.available);
    assert_eq!(usage.reason.as_deref(), Some("tap"));
}

#[tokio::test]
async fn an_expired_window_reads_zero_without_a_reset_and_garbage_does_not_end_the_feed() {
    let sb = Sandbox::new();
    sb.append_rollout("2026/10/23/rollout-a.jsonl", include_str!("fixtures/agents/codex-rollout-tail.jsonl"));
    let clock = sb.clock_at(1_793_018_780); // exactly the fixture's secondary reset
    let mut feed = sb.start_usage_with(&[AgentProvider::Codex], clock).await;
    feed.next().await;
    let usage = feed.next().await.providers.remove(0);
    assert_eq!(usage.windows[1], UsageWindow { length_s: SEVEN_DAYS_S, used_pct: 0.0, resets_at_s: None });
    sb.append_rollout("2026/10/23/rollout-b.jsonl", "{broken\n");
    let usage = feed.next().await.providers.remove(0);
    assert!(!usage.available);
    sb.append_rollout("2026/10/23/rollout-c.jsonl", include_str!("fixtures/agents/codex-rollout-tail.jsonl"));
    assert!(feed.next().await.providers[0].available);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ghostd --test agents`
Expected: FAIL to compile on `usage_feed_at`.

- [ ] **Step 3: Implement the usage feed**

Same loop shape as Task 4. Reading a tail: `File::open`, `seek(SeekFrom::End(-min(len, TAIL_BYTES)))`, `read_to_end`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ghostd --test agents`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/agents crates/ghostd/tests/agents.rs
git commit -m "feat(agents): publish Claude and Codex usage from the captured readings"
```

---

### Task 6: The two taps as ghostd subcommands

**Files:**
- Create: `crates/ghostd/src/taps/mod.rs`, `crates/ghostd/src/taps/claude.rs`, `crates/ghostd/src/taps/codex.rs`
- Modify: `crates/ghostd/src/lib.rs` (`pub mod taps;`), `crates/ghostd/src/main.rs:31-36,48-54` (USAGE and dispatch)
- Test: `crates/ghostd/tests/agents.rs` (runs `env!("CARGO_BIN_EXE_ghostd")` with a fake `HOME`, `XDG_STATE_HOME`, `XDG_CONFIG_HOME`, `XDG_RUNTIME_DIR`)

**Interfaces:**
- Consumes: `capture_path` (Task 5), `codex::sessions_dir`, `codex::hook_effect`, `codex::SessionFile` (Task 3), `identity` (Task 4), `config::config_dir()` (`load.rs:114`), `unix_now_s`.
- Produces:
  ```rust
  // taps/claude.rs
  pub const INNER_FILE: &str = "claude-statusline-inner";   // under config_dir()
  /// Writes stdin to the capture atomically, then runs the saved inner command with the same stdin, stdout passed through.
  pub fn claude_statusline_tap() -> ExitCode;

  // taps/codex.rs
  pub const SESSION_ID_CHARS: &str = "ASCII letters, digits, '-' and '_'";
  /// The nearest ancestor of `pid` whose comm is `codex`, as (pid, start_time).
  pub fn codex_ancestor(root: &SysRoot, pid: u32) -> Option<(u32, u64)>;
  pub fn codex_hook() -> ExitCode;
  ```
  Both return `ExitCode::SUCCESS` whatever happens, logging failures to stderr. `main.rs` dispatch: `["claude-statusline-tap"] => taps::claude_statusline_tap()`, `["codex-hook"] => taps::codex_hook()`, both added to `USAGE`. The hook: parse `session_id`, `hook_event_name`, `cwd`; a `session_id` with any character outside `SESSION_ID_CHARS` writes nothing; `Set` writes `SessionFile` with `since_s` kept from the existing file when its `state` is unchanged, else `unix_now_s()`, `pid`/`proc_start` from `codex_ancestor(root, getppid())`; `Remove` deletes the file; an untracked event writes nothing.

- [ ] **Step 1: Write the failing tests**

```rust
/// Runs a subcommand with `stdin`, every XDG variable pointed into the sandbox, and returns (status, stdout).
fn run_tap(sb: &Sandbox, args: &[&str], stdin: &str) -> (ExitStatus, String);

#[test]
fn the_statusline_tap_captures_atomically_and_forwards_the_inner_command() {
    let sb = Sandbox::new();
    sb.write_inner("cat; echo -n tail");
    let (status, out) = run_tap(&sb, &["claude-statusline-tap"], "{\"a\":1}");
    assert!(status.success());
    assert_eq!(out, "{\"a\":1}tail");
    assert_eq!(fs::read_to_string(sb.capture_path()).unwrap(), "{\"a\":1}");
    assert!(fs::read_dir(sb.capture_path().parent().unwrap()).unwrap().count() == 1, "no temp file left");
}

#[test]
fn the_statusline_tap_prints_nothing_without_an_inner_command() {
    let sb = Sandbox::new();
    let (status, out) = run_tap(&sb, &["claude-statusline-tap"], "x");
    assert!(status.success() && out.is_empty());
}

#[test]
fn the_codex_hook_writes_the_state_per_event_and_keeps_since() {
    let sb = Sandbox::new();
    let event = |name: &str| format!(r#"{{"session_id":"abc-123","hook_event_name":"{name}","cwd":"/home/u/p"}}"#);
    let (status, out) = run_tap(&sb, &["codex-hook"], &event("UserPromptSubmit"));
    assert!(status.success() && out.is_empty());
    let first = sb.codex_file("abc-123");
    assert_eq!((first.state, first.cwd.as_str(), first.pid), (AgentState::Busy, "/home/u/p", None));
    thread::sleep(Duration::from_millis(1100));
    run_tap(&sb, &["codex-hook"], &event("PreToolUse"));
    assert_eq!(sb.codex_file("abc-123").since_s, first.since_s, "same state keeps since_s");
    run_tap(&sb, &["codex-hook"], &event("PermissionRequest"));
    let waiting = sb.codex_file("abc-123");
    assert_eq!((waiting.state, waiting.waiting_for.as_deref()), (AgentState::Waiting, Some("permission prompt")));
    assert!(waiting.since_s > first.since_s);
    run_tap(&sb, &["codex-hook"], &event("SessionEnd"));
    assert!(!sb.codex_sessions_dir().join("abc-123.json").exists());
}

#[test]
fn the_codex_hook_refuses_a_traversing_id_and_swallows_garbage() {
    let sb = Sandbox::new();
    let (status, out) = run_tap(&sb, &["codex-hook"], r#"{"session_id":"../../escape","hook_event_name":"SessionStart","cwd":"/"}"#);
    assert!(status.success() && out.is_empty());
    assert_eq!(fs::read_dir(sb.codex_sessions_dir()).unwrap().count(), 0);
    assert!(!sb.root().join("escape.json").exists());
    let (status, out) = run_tap(&sb, &["codex-hook"], "not json");
    assert!(status.success() && out.is_empty());
}

#[test]
fn the_codex_ancestor_is_the_nearest_process_named_codex() {
    let dir = tempfile::tempdir().unwrap();
    write_proc(dir.path(), "proc/30/stat", "30 (sh) S 20 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 30 0 1 0\n");
    write_proc(dir.path(), "proc/20/stat", "20 (codex) S 10 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 20 0 1 0\n");
    write_proc(dir.path(), "proc/10/stat", "10 (codex) S 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 10 0 1 0\n");
    write_proc(dir.path(), "proc/1/stat", "1 (systemd) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0 1 0\n");
    let root = SysRoot::new(dir.path());
    assert_eq!(codex_ancestor(&root, 30), Some((20, 20)));
    assert_eq!(codex_ancestor(&root, 20), Some((20, 20)), "the start pid itself counts");
    assert_eq!(codex_ancestor(&root, 1), None);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ghostd --test agents`
Expected: FAIL: the binary prints usage and exits 2; `codex_ancestor` missing.

- [ ] **Step 3: Implement the taps and the dispatch**

The inner command runs through `std::process::Command::new("bash").args(["-c", inner])` with piped stdin and inherited stdout; stop the walk in `codex_ancestor` at pid 0 or 1 and after 64 steps.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ghostd --test agents`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/taps crates/ghostd/src/lib.rs crates/ghostd/src/main.rs crates/ghostd/tests/agents.rs
git commit -m "feat(ghostd): add the Claude status-line tap and the Codex hook subcommands"
```

---

### Task 7: Wire the feeds into the daemon and fake mode

**Files:**
- Modify: `crates/ghostd/src/main.rs:265-282` (`host_feeds`), `crates/ghostd/src/fake/feeds.rs`
- Test: `crates/ghostd/src/fake/feeds.rs` (unit), smoke run

**Interfaces:**
- Consumes: `feeds::agents::{sessions_feed, usage_feed}`.
- Produces in `fake/feeds.rs`: `fn agents_usage() -> Feed`, `fn agents_sessions() -> Feed`, both in `feeds()`. Numbers: Claude 5H 42 % resetting in 2 h 14 min, 7D 61 % resetting in 3 days, plan `None`; Codex 0 % / 14 %, plan `"plus"`. Sessions: `proj-68` waiting on `permission prompt` for 11 min, three busy (two Claude, one Codex named `AGENTS.md draft` in `/home/u/Codes/content-platform`), fourteen idle Claude sessions aged 15 h to 2 days — 18 in total, published once, sorted as the real feed sorts.

- [ ] **Step 1: Write the failing unit test**

In `fake/feeds.rs`'s test module:

```rust
#[test]
fn the_fake_fleet_matches_the_mockup() {
    let sessions = mockup_sessions(NOW);
    assert_eq!(sessions.sessions.len(), 18);
    assert_eq!(sessions.sessions.iter().filter(|s| s.state == AgentState::Waiting).count(), 1);
    assert_eq!(sessions.sessions[0].name, "proj-68");
    let usage = mockup_usage(NOW);
    assert_eq!(usage.providers[0].windows[1].used_pct, 61.0);
    assert_eq!(usage.providers[1].plan.as_deref(), Some("plus"));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p ghostd --lib fake`
Expected: FAIL, `mockup_sessions` not found.

- [ ] **Step 3: Add the fake feeds and register the real ones**

`host_feeds` gains `feeds::agents::sessions_feed(SysRoot::host())` and `feeds::agents::usage_feed()`.

- [ ] **Step 4: Run the tests and the smoke**

Run: `cargo test -p ghostd && scripts/qml-smoke.sh`
Expected: tests PASS; the smoke prints `smoke: ok` (no preset names `agents`, so the fake feeds publish once and then idle).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/main.rs crates/ghostd/src/fake/feeds.rs
git commit -m "feat(ghostd): run the agents feeds and fake the mockup's fleet"
```

---

### Task 8: The widget's rules, as a tested library

**Files:**
- Create: `widgets/agents/agents.mjs` (widget-local, like `widgets/network/network.mjs`; it is installed with the widget, so it imports nothing from `shell/kit`), `shell/tests/agents.test.mjs`
- Modify: `shell/kit/GhostKit/Theme.qml:51-60,116-123` (`usage_warn: 60, usage_crit: 85` in `defaultThresholds`; `usageWarn`/`usageCrit` as fractions like `loadWarn`)

**Interfaces:**
- Produces (ES module; `'--'` and the `SUN…SAT` table are its own constants):
  ```js
  export const PROVIDER_TAGS = Object.freeze({ claude: 'CC', codex: 'CX' });
  export const IDLE_SHOWN = 5;
  export const DAY_S = 86400;
  export function providerTag(id);              // PROVIDER_TAGS[id] ?? first two letters upper-cased
  export function windowLabel(lengthS);          // 18000 → '5H', 604800 → '7D', else Math.round(hours) + 'H'
  export function weeklyOf(provider);            // the window with length_s 604800, or null
  export function sortSessions(sessions);        // waiting, busy, idle; newer since_s first within a group; new array
  export function waitingCount(sessions);
  export function stateLabel(session);           // waiting → waiting_for upper-cased ('PERMISSION PROMPT'), else 'BUSY' / 'IDLE'
  export function projectOf(cwd);                // last path segment, '' for ''
  export function resetText(resetsAtS, nowS);    // null → MISSING; under a day → '2H 14M' ('14M' under an hour, 'NOW' at 0); else dow + ' ' + 'HH:MM' local
  export function collapseIdle(sorted, shown = IDLE_SHOWN); // { rows, hiddenIdle }: idle rows past `shown` are cut and counted
  ```

- [ ] **Step 1: Write the failing tests**

```js
import assert from 'node:assert/strict';
import test from 'node:test';
import { collapseIdle, providerTag, resetText, sortSessions, stateLabel, waitingCount, windowLabel, projectOf } from '../../widgets/agents/agents.mjs';

const s = (id, state, since_s, extra = {}) => ({ provider: 'claude', id, name: id, cwd: '/home/u/p', state, waiting_for: null, since_s, pid: null, tmux: null, ...extra });

test('tags are CC and CX, unknown providers abbreviate', () => {
  assert.equal(providerTag('claude'), 'CC');
  assert.equal(providerTag('codex'), 'CX');
  assert.equal(providerTag('gemini'), 'GE');
});

test('windows are labelled by length', () => {
  assert.equal(windowLabel(18000), '5H');
  assert.equal(windowLabel(604800), '7D');
});

test('waiting first, then busy, then idle, newest first within a group', () => {
  const sorted = sortSessions([s('a', 'idle', 10), s('b', 'busy', 20), s('c', 'waiting', 5), s('d', 'busy', 30)]);
  assert.deepEqual(sorted.map(x => x.id), ['c', 'd', 'b', 'a']);
});

test('state labels name the wait', () => {
  assert.equal(stateLabel(s('a', 'waiting', 0, { waiting_for: 'permission prompt' })), 'PERMISSION PROMPT');
  assert.equal(stateLabel(s('a', 'busy', 0)), 'BUSY');
  assert.equal(waitingCount([s('a', 'waiting', 0), s('b', 'idle', 0)]), 1);
});

test('reset reads as a countdown under a day and a weekday beyond', () => {
  const now = 1792797010;
  assert.equal(resetText(null, now), '--');
  assert.equal(resetText(now + 2 * 3600 + 14 * 60, now), '2H 14M');
  assert.equal(resetText(now + 59, now), 'NOW');
  const later = now + 3 * 86400;
  const d = new Date(later * 1000);
  const hhmm = String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
  assert.match(resetText(later, now), /^(SUN|MON|TUE|WED|THU|FRI|SAT) \d\d:\d\d$/);
  assert.ok(resetText(later, now).endsWith(hhmm));
});

test('idle rows collapse past five', () => {
  const sorted = sortSessions([s('w', 'waiting', 1), ...Array.from({ length: 8 }, (_, i) => s('i' + i, 'idle', i))]);
  const { rows, hiddenIdle } = collapseIdle(sorted);
  assert.equal(rows.length, 6);
  assert.equal(hiddenIdle, 3);
  assert.equal(projectOf('/home/u/Codes/ghost-shell'), 'ghost-shell');
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `node --test shell/tests/agents.test.mjs`
Expected: FAIL, module not found.

- [ ] **Step 3: Implement `agents.mjs` and the Theme thresholds**

- [ ] **Step 4: Run the tests to verify they pass**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: PASS, including the existing theme tests.

- [ ] **Step 5: Commit**

```bash
git add widgets/agents/agents.mjs shell/tests/agents.test.mjs shell/kit/GhostKit/Theme.qml
git commit -m "feat(shell): add the agents chip's rules and usage thresholds"
```

---

### Task 9: The agents widget

**Files:**
- Create: `widgets/agents/manifest.toml`, `widgets/agents/Widget.qml`, `widgets/agents/AgentsPopup.qml`
- Test: `scripts/qml-smoke.sh` for layouts a, b and c; `cargo test -p ghostd --test widgets --test config_bar`

**Interfaces:**
- Consumes: `Data.topic("agents.usage").value`, `Data.topic("agents.sessions").value` (wanted by the bar through `requires`, so no `syncTopics`), `SegMeter` (`value` 0..1, `warnAt: Theme.usageWarn`, `critAt: Theme.usageCrit`), `BarPopup` (the `IconTasks.qml:302-314` shape with `catchOutside: true`), `FrameClock.epochSecond`, `Fmt.ago`, `Fmt.pct`, the Task 8 library imported as `import "agents.mjs" as Agents` (the `widgets/network/Widget.qml:3` form).
- Produces: a widget with id `agents`, `kinds = ["bar"]`, `requires = ["agents.usage", "agents.sessions"]`, one enum setting `style` with `values = ["chip"]`, default `"chip"` (so `"parts"` and `"dots"` are rejected by `check_setting` until built).

- [ ] **Step 1: Write the manifest and a failing check**

The smoke needs a bar that names the widget, since no preset does. Write a helper for this task's runs:

```bash
agents_smoke() {  # $1: a, b or c
    local cfg; cfg=$(mktemp --suffix=.toml)
    target/debug/ghostd default-config | sed "s/^layout = \"a\"/layout = \"$1\"/" > "$cfg"
    printf '\n[layout.bar]\nleft = [{ widget = "brand" }, { widget = "tasks", settings = { style = "icons" } }]\nright = [{ widget = "status", settings = { parts = ["cpu", "net", "vpn", "weather"] } }, { widget = "agents" }, { widget = "tray" }, { widget = "view-chip" }, { widget = "bar-clock" }]\n' >> "$cfg"
    GHOST_CONFIG=$cfg scripts/qml-smoke.sh
}
```

Run `agents_smoke a` now and confirm it fails only with `no widget named "agents" is installed`.

- [ ] **Step 2: Build the chip**

`Widget.qml`, left to right in one `Row` with the status widget's `partGapPx`/`labelGapPx`/`textPx` values: `AI` in `Theme.hi`; per provider in config order a `Label` with `Agents.providerTag(p.id)`, a `SegMeter` sized to ten cells (`width: 10 * cellPx + 9 * gapPx` design pixels) with `value: weekly ? weekly.used_pct / 100 : NaN` and `sourceLive: p.available`, and `Fmt.pct(weekly.used_pct)` or `--`; then `·`, the session count in `Theme.hi`, and `N WAIT` in `Theme.warn` only while `waiting > 0`. A 1 px border rectangle around the chip in `Theme.warn` while waiting, else transparent. A `ClickArea` toggles `popup.open`; `BarPopup { catchOutside: true; onDismissed: open = false; content: Component { AgentsPopup { … } } }` among the chip's siblings.

- [ ] **Step 3: Build the popup**

`AgentsPopup.qml`: a `Panel`-styled column at the mockup's proportions: header `AI · USAGE AND SESSIONS` with `N SESSIONS · M WAITING`; two provider columns each with label, plan, and two `usage` rows (`5H`/`7D`, a track `Rectangle` filled by `used_pct`, in `Theme.signal`/`warn`/`crit` by `Theme.usageWarn`/`usageCrit`, the percent, `↻ ` + `Agents.resetText`); when a provider is unavailable with `reason === "tap"`, the row reads `CC -- · run scripts/install-agents.sh`; then the session rows from `Agents.collapseIdle(Agents.sortSessions(sessions))`: tag, name with `Agents.projectOf(cwd)` small and dim, state dot plus `Agents.stateLabel`, `Fmt.ago(nowS - since_s)`; a final `N more idle` row when `hiddenIdle > 0`. The waiting dot blinks by `FrameClock.epochSecond % 2`, nothing else animates.

- [ ] **Step 4: Run the smokes and the tests**

Run: `agents_smoke a && agents_smoke b && agents_smoke c && scripts/qml-smoke.sh && cargo test -p ghostd --test widgets --test config_bar`
Expected: every smoke prints `smoke: ok` with 0 errors (the plain run proves the default bar is unchanged); tests PASS. **Call:** the popup is not opened in the smoke. `GHOST_SMOKE_ACTIONS` only drives view actions, and an `IpcHandler` inside a widget crashes Quickshell 0.2.1 when its scope is destroyed (`Launcher.qml:333`), so the popup is covered by the Task 8 tests and the live acceptance in Task 11.

- [ ] **Step 5: Commit**

```bash
git add widgets/agents
git commit -m "feat(widgets): add the AI chip with usage meters, session count and popup"
```

---

### Task 10: The installer

**Files:**
- Create: `scripts/agents-hooks.py`, `scripts/test_agents_hooks.py`, `scripts/install-agents.sh`
- Modify: `Makefile` (`install-agents` target, in `.PHONY`), `README.md` (a `### LLM agents (optional)` section after the terminal theme one)

**Interfaces:**
- Produces (`agents-hooks.py`, stdlib only, run as `uv run --no-project python -I scripts/agents-hooks.py <command> …`):
  ```python
  TAP_SUFFIX = "ghostd claude-statusline-tap"
  HOOK_EVENTS = ("SessionStart", "UserPromptSubmit", "PreToolUse", "PermissionRequest", "PostToolUse", "Stop", "Interrupt", "SessionEnd")
  HOOK_TIMEOUT_S = 2
  def wrap_claude(settings: dict, tap_command: str) -> tuple[dict, str | None]   # new settings, and the inner command to save (None when already wrapped)
  def unwrap_claude(settings: dict, inner_command: str | None) -> dict           # statusLine back to inner, or removed when None
  def merge_codex(hooks: dict, command: str) -> dict                             # one unmatched group per event holding {type: "command", command, timeout}; a group already holding `command` is left alone
  def remove_codex(hooks: dict, command: str) -> dict
  def main(argv: list[str]) -> int
  # CLI: claude-wrap SETTINGS INNER_FILE TAP ; claude-unwrap SETTINGS INNER_FILE ; codex-merge HOOKS CMD ; codex-remove HOOKS CMD
  # Every command reads the JSON file (a missing file is {}), applies the pure function, writes back with indent=2 and a trailing newline, and prints the event names it wrote for codex-merge.
  ```
  `install-agents.sh [--dry-run|--uninstall]`: dies unless `$HOME/.local/bin/ghostd` exists; backs up `~/.claude/settings.json` and `~/.codex/hooks.json` (when present) to `<file>.ghost-shell.bak.<timestamp>` and verifies each with `cmp` before any edit; runs the two commands; prints the backup paths and `restore: cp <backup> <file>`. `--uninstall` runs the unwrap and remove commands with the same backups.

- [ ] **Step 1: Write the failing Python tests**

```python
def test_wrap_saves_the_inner_command_and_points_at_the_tap():
    settings = {"statusLine": {"type": "command", "command": "bun hud.ts"}, "model": "x"}
    wrapped, inner = ah.wrap_claude(settings, "/h/.local/bin/ghostd claude-statusline-tap")
    assert inner == "bun hud.ts"
    assert wrapped["statusLine"] == {"type": "command", "command": "/h/.local/bin/ghostd claude-statusline-tap"}
    assert wrapped["model"] == "x" and settings["statusLine"]["command"] == "bun hud.ts"

def test_rewrap_keeps_the_inner_command():
    wrapped, _ = ah.wrap_claude({"statusLine": {"type": "command", "command": "bun hud.ts"}}, "/h/ghostd claude-statusline-tap")
    again, inner = ah.wrap_claude(wrapped, "/h/ghostd claude-statusline-tap")
    assert inner is None and again == wrapped

def test_unwrap_restores_or_removes():
    tapped = {"statusLine": {"type": "command", "command": "/h/ghostd claude-statusline-tap"}}
    assert ah.unwrap_claude(tapped, "bun hud.ts")["statusLine"]["command"] == "bun hud.ts"
    assert "statusLine" not in ah.unwrap_claude(tapped, None)

def test_merge_adds_one_group_per_event_once():
    existing = {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "other"}]}]}}
    merged = ah.merge_codex(existing, "/h/ghostd codex-hook")
    assert set(merged["hooks"]) == set(ah.HOOK_EVENTS)
    assert len(merged["hooks"]["Stop"]) == 2
    assert merged["hooks"]["PermissionRequest"] == [{"hooks": [{"type": "command", "command": "/h/ghostd codex-hook", "timeout": 2}]}]
    assert ah.merge_codex(merged, "/h/ghostd codex-hook") == merged

def test_remove_leaves_other_hooks():
    merged = ah.merge_codex({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "other"}]}]}}, "/h/ghostd codex-hook")
    removed = ah.remove_codex(merged, "/h/ghostd codex-hook")
    assert removed == {"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "other"}]}]}}

def test_cli_round_trips_files(tmp_path):
    settings = tmp_path / "settings.json"; inner = tmp_path / "inner"
    settings.write_text('{"statusLine": {"type": "command", "command": "bun hud.ts"}}\n')
    ah.main(["claude-wrap", str(settings), str(inner), "/h/ghostd claude-statusline-tap"])
    assert inner.read_text() == "bun hud.ts"
    assert json.loads(settings.read_text())["statusLine"]["command"].endswith(ah.TAP_SUFFIX)
    ah.main(["claude-unwrap", str(settings), str(inner)])
    assert json.loads(settings.read_text())["statusLine"]["command"] == "bun hud.ts"
```

- [ ] **Step 2: Run them to verify they fail**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_agents_hooks.py`
Expected: FAIL, import error.

- [ ] **Step 3: Implement the script, the installer and the Makefile target**

`install-agents.sh --dry-run` prints what it would write. `install-agents.sh` is never called by `install.sh`.

- [ ] **Step 4: Run the tests and the dry run**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_agents_hooks.py && scripts/install-agents.sh --dry-run && shellcheck scripts/install-agents.sh`
Expected: PASS; the dry run lists both files and the eight events; shellcheck clean.

- [ ] **Step 5: Write the README section**

What it edits, the backups, `--uninstall`, that other tools (claude-hud updates, unsnooze setup) rewrite `statusLine` and the chip then shows `CC --` until the installer is rerun, that a Codex session already running stays invisible until its next prompt, and the test command.

- [ ] **Step 6: Commit**

```bash
git add scripts/agents-hooks.py scripts/test_agents_hooks.py scripts/install-agents.sh Makefile README.md
git commit -m "feat(install): wire the Claude status-line tap and the Codex hooks with backups"
```

---

### Task 11: Live acceptance, performance and install

Done with the user present, in this order, nothing before their yes at each marked point.

**Files:**
- Modify: `README.md` ("Performance" gains `### Agents feeds`)

- [ ] **Step 1: Baseline the live daemon**

Run: `scripts/measure-cpu.sh "$(systemctl --user show -p MainPID --value ghostd)" 60`
Record `cpu_pct` and `rss_kib` with the current fleet size (`ls ~/.claude/sessions | wc -l`).

- [ ] **Step 2: Install (user's yes)**

Run: `scripts/install.sh && systemctl --user restart ghostd ghost-shell`, then `scripts/install-agents.sh`.
Then, with the user's yes, append to `~/.config/ghost-shell/config.toml` (which has `bar.layout = "a"` and no `[layout.bar]`) preset a's bar with the chip added, so nothing else moves:

```toml
[layout.bar]
left = [{ widget = "brand" }, { widget = "tasks", settings = { style = "icons" } }]
right = [
    { widget = "status", settings = { parts = ["cpu", "net", "vpn", "weather"] } },
    { widget = "agents" },
    { widget = "tray" },
    { widget = "view-chip" },
    { widget = "bar-clock" },
]
```

The config watcher applies it live. Expected: both installers print their backups; the chip shows `CC -- · CX nn%` until the next Claude status render, then `CC nn%`.

- [ ] **Step 3: Accept**

With the user: the real fleet shows with its names; a `claude` session put on a permission prompt flips the chip to `1 WAIT` in warn within a second and back on approval; a Codex `PermissionRequest` does the same and its approval prompt still appears; the popup opens on click and closes on a click outside; resets read as a countdown or a weekday.

- [ ] **Step 4: Measure after**

Run the Step 1 command again, and `journalctl --user -u ghostd --since -5min | grep -c agents` for warnings.

- [ ] **Step 5: Log and commit**

README "Performance" `### Agents feeds`: date, fleet size, before and after `cpu_pct`/`rss_kib`, one line on the cost per Codex hook event (time `echo '{}' | ~/.local/bin/ghostd codex-hook`).

```bash
git add README.md
git commit -m "docs(perf): log the agents feeds' cost on the live daemon"
```

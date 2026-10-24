# ghost-shell agents: design

## Intent

The user runs many LLM coding agents at once from terminals: at the time of writing, eighteen Claude Code processes across
three config dirs and two Codex CLI processes. Two things get lost in that fleet. One session sits on a permission prompt
for ten minutes while the user works elsewhere. And the user cannot tell, before starting one more session, how much of the
five-hour and weekly subscription windows is left and when they reset.

This is the "Agent usage component" of `docs/IDEAS.md`: one bar widget, the **AI chip**, that shows both. It is direction A of
`mockups/2026-10-23-ghost-shell-agents-bar.html` (user's decision, 2026-10-23). Directions B (two plain parts) and C (a dot
strip) are deferred as later values of the widget's `style` setting.

## Goals

- In the bar: each enabled provider's weekly used-percent with a segment meter, the live session count, and how many
  sessions wait on the user. The chip turns warn while any session waits.
- In the popup: both windows (five-hour and weekly) per provider with used-percent and a reset time; every live session with
  provider, name, project, state and how long it has been in that state.
- Providers default to Anthropic (Claude Code) and OpenAI (Codex CLI). Each provider is its own implementation behind one
  shared shape, so a third can be added without touching the widget.
- V1 covers the CLIs only. Usage is read for the default Claude profile (`~/.claude`) only; sessions are read from every
  configured Claude config dir (user's decisions, 2026-10-23).
- The shell never calls Anthropic or OpenAI for its own sake (user's decision, 2026-10-23). Every number comes from what the
  CLIs already write to disk while the user uses them.

## Non-goals

- Cost in dollars, token counts, or burn-rate estimates (ccusage's territory).
- GUI agents: the Claude VS Code extension, the Codex desktop app, Claude desktop. Whatever the Claude session registry lists
  is shown, but nothing is done to find sessions it does not list.
- Per-account usage. Sessions from the work config dirs show in the list; their accounts' limits do not.
- Jumping to a session's terminal from the popup. The data for it (tmux pane, pid) is kept in the snapshot so a later
  change can add it.
- Resuming limit-stopped sessions (unsnooze does that).

## Anti-goals

- Polling a vendor API from the daemon, even an undocumented one with the user's own OAuth token. Rejected because it is
  rate-limited per account, undocumented, and puts the user's credentials in ghostd's hands for a status readout.
- A hook or wrapper that slows the agents down. The taps below do one small write and get out of the way; a permission
  hook that took a decision, or blocked while waiting on ghostd, would be a failure even if the chip were right.
- Session state carried by colour alone (the user is colour-blind; parent design). Waiting is colour **and** the blinking dot
  **and** the WAIT count in text.

## Constraints

- Both data sources are version-coupled to the CLIs. The Claude status-line JSON is documented (`rate_limits` since Claude
  Code 2.1.80); the Claude session registry and the Codex rollout format are not. Each parser has fixture tests, and a file
  that fails to parse makes its provider read as unavailable rather than crashing the feed.
- Eighteen Claude sessions render their status lines concurrently. The usage capture must be an atomic rename, never an
  in-place write.
- Agents working on this repo stay headless (`scripts/qml-smoke.sh`, `scripts/headless-kwin.sh`); nothing opens on the
  user's display. Every accepted change is installed on the live desktop with `scripts/install.sh` right away, and every
  measurement goes in README.md's "Performance" section.

## What the sources are (verified on this machine, 2026-10-23)

### Claude Code 2.1.309

- **Sessions:** `<config dir>/sessions/<pid>.json`, one per process, rewritten on every state change. Fields used: `pid`,
  `procStart` (the `/proc/<pid>/stat` start time, in clock ticks), `sessionId`, `cwd`, `name`, `kind`
  (`interactive|bg|daemon|daemon-worker`), `status`, `waitingFor`, `statusUpdatedAt` (ms), `tmux`
  (`session:@window.%pane`). Seen in the files here: `status` of `busy`, `idle` and `waiting`. From the docs of
  `claude agents` (the same fields): a `shell` status, and `waitingFor` of `permission prompt`, `input needed`,
  `sandbox request`, `worker request` or `dialog open` while waiting. Nothing says the file is removed when the process
  dies, so liveness is `pid` alive **and** its start time equal to `procStart`. The documented route, `claude agents
  --json`, lists the same interactive sessions (checked) but costs a Node start per poll, so it is the fallback named in
  the docs, not the reader.
- **Usage:** Claude Code pipes JSON to the configured `statusLine` command on every render. `rate_limits.five_hour` and
  `rate_limits.seven_day` each carry `used_percentage` (0–100) and `resets_at` (epoch seconds). Pro/Max only; absent until
  the session's first API response; a window is dropped from the JSON once its `resets_at` passes. The user's status line
  is claude-hud, launched from `~/.claude/settings.json`.

### Codex CLI 0.164.0

- **Usage:** every rollout `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` carries `event_msg` lines with
  `payload.type = "token_count"` and `payload.rate_limits = {primary, secondary, plan_type, …}`, each window
  `{used_percent, window_minutes, resets_at}`. `primary` is the 300-minute window, `secondary` the 10080-minute one. The
  newest such line across all rollouts is the current reading. (`codex app-server` answers `account/rateLimits/read` with
  the same numbers in camelCase, verified live, but that is a network call to OpenAI and so is not used.)
- **Sessions:** Codex writes no status registry, and its shared app-server daemon did not answer a `thread/loaded/list`
  through `codex app-server proxy` (CodexBar's docs say the TUI's threads are not reachable that way either). Codex
  does run user-level hooks from `~/.codex/hooks.json` with `session_id`, `cwd`, `hook_event_name` and `turn_id` on stdin
  for `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `Stop`, `Interrupt` and
  `SessionEnd`. `PermissionRequest` fires only when approval is actually needed, and a hook that exits without a decision
  leaves the normal prompt in place. Thread titles are in `~/.codex/session_index.jsonl` (`{id, thread_name, updated_at}`,
  append-only, last line per id wins).

## Design

### Topics

Two built-in topics, since they change at different rates and a later widget (direction C) wants only one:

- `agents.usage` — `AgentsUsage { providers: Vec<ProviderUsage> }`, one entry per enabled provider in config order.
  `ProviderUsage { id, label, plan: Option<String>, available: bool, reason: Option<String>, read_at_s: u64,
  windows: Vec<UsageWindow> }`,
  `UsageWindow { length_s: u64, used_pct: f64, resets_at_s: Option<u64> }`, the reset `None` once it has passed. The
  widget labels a window from `length_s` (5H, 7D) and tags a provider from its `id` (`claude → CC`, `codex → CX`, a
  widget-side table). `available = false` with empty windows means no reading yet or an unparseable one; the chip shows
  `--` for it. `read_at_s` is the source file's mtime, for the popup.
- `agents.sessions` — `AgentsSessions { sessions: Vec<AgentSession> }`.
  `AgentSession { provider, id, name, cwd, state: busy|idle|waiting, waiting_for: Option<String>, since_s: u64,
  pid: Option<u32>, tmux: Option<String> }`, `since_s` being the epoch second the state began. Sorted waiting first, then
  busy, then idle, newest `since_s` first within a group. Claude's `status` maps `busy → busy`, `shell → idle` (the user
  is running a `!` command at the prompt), `idle → idle`, `waiting → waiting` whatever `waitingFor` says: every documented
  value (`permission prompt`, `input needed`, `sandbox request`, `worker request`, `dialog open`) is the session blocked on
  the user, which is what the chip's WAIT count is for **[call]**. `kind` `interactive` and `bg` are shown.

Both live in `ghost_proto` next to the other snapshots, with topic constants `AGENTS_USAGE` and `AGENTS_SESSIONS`.

### Feeds (ghostd, in-process)

`crates/ghostd/src/feeds/agents/` with `mod.rs` (the two feeds and the merge), `claude.rs` and `codex.rs` (the parsers and
readers, one per provider, each behind the same small trait). **[call]** In-daemon Rust rather than external providers under
`[[providers]]`: both readers are file parsers with no dependencies, the snapshot types belong in `ghost_proto` anyway, and
an external provider would add a crate, a supervisor entry and an install step for the same ~300 lines.

- **Sessions feed.** Watches (inotify, the pattern of `config::spawn_watcher`) every Claude sessions dir and the Codex state
  dir below; on any change, or every 30 s as a liveness sweep, re-reads the files, drops the dead and publishes when the
  snapshot changed. Dead means the recorded pid is gone or its `/proc/<pid>/stat` start time differs; `SysRoot` provides
  `/proc` so tests use a fake one. Claude's `daemon` and `daemon-worker` kinds are hidden. A Codex session's `name` is its
  `thread_name` from `session_index.jsonl` (re-read when that file changes), or the cwd's basename plus the id's first
  eight characters until it has one.
- **Usage feed.** Watches the Claude capture file (below) and, for Codex, the newest rollout: on change it reads the file
  tail (last 256 KiB is enough; a `token_count` line is a few hundred bytes) for the last `rate_limits`. A window whose
  `resets_at_s` has passed is published as `used_pct = 0` with no reset, since the source only updates when the agent next
  runs; a window Claude has dropped from its JSON for the same reason is published the same way, its `length_s` being the
  known one for its key. A reading has no age limit: a window only moves when the user uses the agent, and that use rewrites the source, so
  the last reading plus its reset time stays right through any idle stretch, a logout or a reboot **[call]**. What can go
  wrong instead is the tap being removed from the status line by another tool, so the feed also watches `settings.json`
  in the first configured Claude config dir and publishes Claude as unavailable with `reason = "tap"` while
  `statusLine.command` is not the tap; the chip shows `CC --` and the popup says the installer needs rerunning.

Both feeds publish only while wanted, like `weather`, and publish an empty snapshot at start so the widget renders at once.

### Taps (two ghostd subcommands)

Both are subcommands of the installed `ghostd` binary, so there is no script to ship and no interpreter start:

- `ghostd claude-statusline-tap` — reads stdin to the end, writes it to
  `$XDG_STATE_HOME/ghost-shell/claude-statusline.json` (default `~/.local/state`) by temp file and rename, then runs
  the inner status line command saved by the installer (`~/.config/ghost-shell/claude-statusline-inner`) with `bash -c`,
  feeding it the same bytes and passing its stdout through. With no inner command it prints nothing. Any failure of the
  write is logged to stderr and never stops the inner command.
- `ghostd codex-hook` — reads the hook JSON from stdin and writes
  `$XDG_RUNTIME_DIR/ghost-shell/codex-sessions/<session_id>.json` with `{state, waiting_for, cwd, since_s, pid, proc_start}`
  where `pid` is the nearest ancestor process named `codex` (hooks are spawned through a shell, so the direct parent is
  not it) and `proc_start` its `/proc` start time; without such an ancestor both are null and the file is only ever removed
  by `SessionEnd` or by the sweep once it is 24 h old. State by event: `SessionStart|UserPromptSubmit|PreToolUse|PostToolUse
  → busy`, `PermissionRequest → waiting ("permission prompt")`, `Stop|Interrupt → idle`, `SessionEnd → remove the file`.
  It always exits 0 with empty stdout: Codex ignores plain stdout and takes a decision only from a `hookSpecificOutput`
  JSON, so this is the documented "let the normal approval prompt continue". `since_s` is kept when the state is unchanged.

The usage capture lives in the state dir because it stays valid across logins. The Codex session files live in
`$XDG_RUNTIME_DIR/ghost-shell/` (tmpfs, mode 0700) because no Codex process survives a logout. A Codex session that was
already running when the installer ran stays invisible until its next hook event, which is its next prompt or tool call
**[call]**.

### Installer

`scripts/install-agents.sh`, the shape of `install-term.sh`: a verified backup first, idempotent, and `--uninstall` restores.
The user chose the script over hand-wiring (2026-10-23).

- Claude: in the default profile's `~/.claude/settings.json` only, saves the current `statusLine.command` to
  `~/.config/ghost-shell/claude-statusline-inner` (unless it is already the tap) and sets `statusLine` to
  `{type: "command", command: "$HOME/.local/bin/ghostd claude-statusline-tap"}`, the absolute path `install.sh` puts
  the binary at, since Claude's shell need not have it on PATH; the hook command below is written the same way. Backup
  `settings.json.ghost-shell.bak.<timestamp>`. Other
  tools rewrite this key (claude-hud on update, unsnooze setup); rerunning the installer re-wraps, and the feed's tap check
  above is how the user notices it was clobbered.
- Codex: merges into `~/.codex/hooks.json` (an object keyed by event name, each holding matcher groups with a `hooks`
  array of `{type: "command", command, timeout}`) one unmatched group per event above with command `ghostd codex-hook` and a
  2 s timeout (`Interrupt` and `SessionEnd` cap at 3 s), recognised by its command so a rerun does not duplicate; same
  backup scheme. Hooks are on by default in Codex 0.164.0 and this machine's `config.toml` already records project hooks
  running, so no feature flag is touched; the installer prints the event list it wrote so a Codex release that drops an
  event is noticed at install time.
- The JSON edits are done by a small `scripts/agents-hooks.py` (run with `uv run`, `-I`), with `scripts/test_agents_hooks.py`
  covering wrap, re-wrap, unwrap and the merge, like `term-theme.py`.
- `scripts/install.sh` does **not** call it; the taps touch files outside the repo's install tree and the user runs the
  agents installer once.

### Widget

`widgets/agents/` with `kinds = ["bar"]`, `requires = ["agents.usage", "agents.sessions"]`. Settings:

- `style = "chip"` — direction A. `"parts"` (B) and `"dots"` (C) are reserved and rejected by validation until built.
- `providers` is not a widget setting; it follows `[agents] providers` below.

The chip, left to right: `AI`, then per provider its two-letter tag (`CC`, `CX`), a ten-segment `SegMeter` of the weekly
window and the percent, then `·`, the live session count in `hi` and, only while some session waits, `N WAIT` in warn. The
chip's border goes warn while any session waits. Clicking the chip toggles a `BarPopup` (the kit piece `tasks` and `tray`
use) with `catchOutside`; the popup is the mockup's: a two-column usage block (per provider: plan, 5H and 7D tracks, percent,
reset), then the session rows (provider tag, name with the cwd's basename, state with its dot, time in state). Idle rows
collapse to `N more idle` past five. Reset reads as a countdown under 24 h and weekday plus time beyond.

Meter and track colours follow `[thresholds]`: `usage_warn = 60`, `usage_crit = 85` **[call]**, added beside `load_warn` and
company with the same ordering rule. The dot blinks only in the waiting state, on the once-a-second clock, so a waiting
session costs no frame-clock wakeups.

The widget is installed with the others but placed in **no bar preset** (user's decision, 2026-10-24: most installs run no
LLM agents). It shows only where a `[layout.bar]` names it; the commented example in `default.toml` shows it between
`status` and `tray`, and the user's own config gets that block at acceptance. With no slot wanting the topics, the feeds
watch nothing.

### Configuration

```toml
[agents]
# LLM providers whose usage and sessions the AI chip shows, in order: "claude", "codex".
providers = ["claude", "codex"]
# Claude Code config dirs whose sessions are listed. Usage is read from the first one only.
claude_config_dirs = ["~/.claude"]
# Where Codex keeps its rollouts and session index.
codex_home = "~/.codex"
```

The user adds the two work config dirs to `claude_config_dirs`. Unknown provider names are rejected at load like any other
config error.

### Fake mode and tests

- `--fake` publishes both topics from `fake/feeds.rs`: two providers with the mockup's numbers and a session list with one
  waiting, three busy and fourteen idle, so the headless smoke renders the warn state.
- Rust unit tests per parser (Claude session file, status-line JSON, rollout tail, session index) from fixtures under
  `crates/ghostd/tests/fixtures/agents/` captured from this machine with names and paths anonymised; integration tests in
  `crates/ghostd/tests/agents.rs` drive the feeds with a fake `SysRoot` and a temp dir: a file rewrite publishes, a dead pid
  drops, an expired window zeroes, a malformed file marks the provider unavailable without ending the feed.
- Both subcommands are tested end to end in `tests/agents.rs` by running the built binary with a fake `XDG_STATE_HOME`
  and `XDG_RUNTIME_DIR`: the tap forwards stdin and stdout exactly and creates its directory, the hook writes the right
  state per event, keeps `since_s` across an unchanged state, and exits 0 on garbage input.
- `scripts/qml-smoke.sh` snapshots layout A with the chip in its warn state and the popup open (`GHOST_SMOKE_ACTIONS`).
- Acceptance on the live desktop: the user's real fleet shows; a `claude` session put on a permission prompt flips to WAIT
  within a second and back on approval; a Codex `PermissionRequest` does the same and the approval prompt still appears.
- Performance: measure ghostd's CPU with the fleet running (`scripts/measure-cpu.sh`) before and after, and log it in
  README.md.

## Left to the builder

- Exact segment widths, gaps and the popup's column widths; follow the status widget's design pixels and the mockup.
- Whether the sessions watcher is one inotify instance across dirs or one per dir.
- How the usage feed finds the newest rollout cheaply (mtime walk of the last two day directories is enough).
- Where the hook falls back to when `XDG_RUNTIME_DIR` is unset (`/run/user/<uid>` is the convention).

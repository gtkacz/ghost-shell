# ghost-shell Plan 3 — Part 3b: Notifications and the Bell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ghostd becomes the desktop's `org.freedesktop.Notifications` server, inherited from plasmashell reversibly; the shell shows toasts under the bar, a bell with the unread count in the tray, a drop-down centre, and the dashboard's Notifications panel; the greeting and the gate get their counts.

**Architecture:**
- **Daemon (`ghostd`):**
  - **Feeds can watch topics.** `FeedContext` gains a broadcast of every published topic value, so one feed can react to another's data (`windows` for fullscreen, `session` for locked, `weather.alerts` and `disks` for the built-in sources) without new plumbing per pair.
  - **`feeds/notifications/`:** `history.rs` (the pure rules: replace, dedupe, cap, read state, counts, DND, held stamping, JSON), `images.rs` (`image-data` to PNG), `server.rs` (the zbus interface and the bus-name request), `mod.rs` (the feed: owns the history, serves the calls, publishes the `notifications` topic, persists the file, raises the built-in sources through `notify.rs`).
  - **The name** is requested only when the marker `$XDG_STATE_HOME/ghost-shell/notifications-server` exists, with `REPLACE_EXISTING` set and neither `ALLOW_REPLACEMENT` nor `DO_NOT_QUEUE`, so ghostd queues behind a running plasmashell and inherits the name when it restarts. `ghostd.service` becomes `Type=notify`; READY is sent once the request has been answered, owned or queued, or at once without the marker.
  - **The gate snapshot** gains `[session] signals=<n>` while locked.
- **Shell:**
  - **Rules in `lib/notifications.mjs`** (node-tested): what to toast, the bell's state, the filter, the row text, which window to raise, the counts line.
  - **`Notifications` singleton** in GhostKit: open/toggle/close of the centre, the `Meta+N` sequence from the `desktop` topic, a debug IPC for smokes.
  - **Surfaces:** `shell/host/Toasts.qml` (one Overlay surface for the stack, only while something shows) and `shell/host/NotificationCentre.qml` (a `BarPopup`-style panel surface from the bell, `OnDemand` focus, closes on focus loss). Rows are `GhostKit/NotificationRows.qml`, shared with `widgets/notifications`.
  - **The bell** is a `TrayItem` in `widgets/tray`, after the status items, before the chevron.
- **Plasma:** `scripts/plasma-panel.sh retire-notifications|restore-notifications` writes the marker, the systemd drop-in that starts plasmashell after ghostd, and Plasma's popup corner, restarts ghostd and plasmashell, and undoes all of it. It runs live only at acceptance, with the user's yes.

**Tech Stack:**
- Rust 1.98, zbus 6 (`service` feature, already on), tokio, serde. **New crate:** `png` (pure Rust, encode only).
- Quickshell 0.2.1 on Qt 6.11: `PanelWindow`, `WlrLayershell`, `LazyLoader`, `IpcHandler`.
- Node 24 for `.mjs` tests. KWin 6.7.5, plasma-workspace 6.7.5 for the spikes.

**Spec:** `docs/superpowers/specs/2026-10-24-ghost-shell-plan-3b-notifications-design.md`, on top of `docs/superpowers/specs/2026-10-20-ghost-shell-design.md` §5, §8, §11, §13, §14, §15 and the 3a design `docs/superpowers/specs/2026-10-22-ghost-shell-plan-3a-bar-design.md`. The visual truth: `docs/superpowers/specs/mockups/2026-10-24-ghost-shell-notifications.html` (density A for toasts; CSS `.toast`, `.alert`, `.life`, `.centre`, `.search`, `.tools`, `.list`, `.foot`, `.badge`) and the reference mockup's Notifications panel (`docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html`, CSS `.nsearch`, `.notifs`, `.nfoot`; script `NOTIFS`, `renderNotifs`).

Read with: the Plasma-hidden spike note `docs/superpowers/notes/2026-10-23-plasma-hidden-spike.md` (the sandbox and the retire mechanics) and 3a's process rules `.superpowers/sdd/2026-10-22-ghost-shell-plan-3a-bar/process-rules.md`.

**Prerequisite:** Plan 3a is complete and accepted on `main` (2026-10-24, HEAD `bdbda0f` when this plan was written). Another session's agents work also lives on `main`; every task re-reads the files it edits at its own base.

## Decisions (user, 2026-10-24)

1. **Take over `org.freedesktop.Notifications`** (spec §4). Restyling around Plasma's server has no workable form.
2. **The server lives in ghostd**, not in Quickshell's built-in one.
3. **Toast density A (cards).** B (compact rows) stays in the mockup only.
4. **KIO job progress stays with Plasma in 3b.** Plasma's job popup moves to the opposite corner; the JobViewServer takeover is backlog.

## Calls

Marked **[call]**; the user may override any of them. These add to the spec's own calls.

1. **[call] Feeds watch topics through `FeedContext.topics`**, a `tokio::sync::broadcast` of `TopicUpdate { topic: Arc<str>, value: Arc<Value> }` with capacity 256, sent by the hub for every published value. A feed that never reads it just lags, which is harmless. This replaces per-pair channels for fullscreen, locked, alerts and drives.
2. **[call] The shell raises the sender's window**, not ghostd (spec §5.4 said ghostd). The shell already holds the `windows` topic and the `windows.activate` call; `notifications.mjs`'s `windowToRaise(windows, record)` picks the window by `app_id === desktop_entry`, else `pid === record.pid`, and the shell sends `windows.activate` after `notifications.invoke`. ghostd records the sender's pid (`GetConnectionUnixProcessID` of the caller) in the record for that.
3. **[call] Built-in sources go through the bus**, with `notify::send` extended to carry the app name, urgency, `replaces_id` and hints, so they land in whoever serves (Plasma before the retire, ghostd after). INMET and the dead drive are raised by the notifications feed from the topic updates (call 1).
4. **[call] The marker is read at ghostd's start only.** The retire and restore steps restart ghostd, so no live re-read is needed.
5. **[call] `arrived_s` is in Unix seconds**, like `WeatherAlert.start_s`; `Fmt.ago()` takes seconds.
6. **[call] Images:** `image-data` is decoded only when `bits_per_sample == 8`, `channels` is 3 or 4, and `width × height ≤ 4 096 × 4 096`; anything else is logged and the record carries no image. The PNG goes to `$XDG_RUNTIME_DIR/ghost-shell/notify/<id>.png`, mode 0600; the directory is pruned to the ids in history on every save.
7. **[call] Markup:** the body keeps `<b>`, `<i>`, `<u>` and `<a href="…">`, drops every other tag, and `body_text` is the body with every tag removed and entities decoded. A body that is not valid markup is escaped whole.
8. **[call] The centre is right-aligned to the bell**: its right edge sits at the bell's right edge, clamped to the screen; `BarPopup` is extended with an `alignRight` property for it.
9. **[call] Toasts and the centre are `GHOST_BAR=off`-aware**: with the bar off (`scripts/measure-covered.sh`), neither loads, so the covered "0 frames" check stays exact.
10. **[call] Meta+N is the `notifications` id** in the `ghost-shell` KGlobalAccel component, relayed like the launcher: `DesktopSnapshot.notifications_seq` counts presses, whatever has focus.

## Global Constraints

Plan 3a's Global Constraints apply unchanged (process rules, never touch the user's Plasma or sessions, budgets and O1, Conventional Commits with no trailer, WHY-only comments, braces, files ≤ 800 lines, `cargo test --workspace` / `cargo fmt --check` / `cargo clippy --workspace --all-targets -- -D warnings` / `node --test 'shell/tests/*.test.mjs'` / the relevant `make smoke*` headless before every commit). These add to them.

- Smokes use `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-3b-t<N>.sock`; headless KWins are `3b-t<N>…`; every `qs` run sets `GHOST_GREETING=${GHOST_GREETING:-off}`.
- **Never request `org.freedesktop.Notifications` on the user's session bus** from a test, a spike or a dev daemon. Every feed test runs on `common::bus::PrivateBus`; dev daemons run with no marker. The only exception is Task 15, after the user says yes.
- **Record (`ghost_proto::Notification`), JSON keys exactly:** `id` u32, `app`, `desktop_entry`, `app_icon`, `image` (a path or `""`), `summary`, `body` (markup subset), `body_text`, `urgency` (`low|normal|critical`), `level` (`none|warn|crit`), `actions` `[{key, label}]`, `has_default` bool, `pid` u32 (0 unknown), `arrived_s` u64, `read`, `resident`, `held`.
- **Topic `notifications` (`NotificationsSnapshot`):** `server` (`none|plasma|ghost-shell`), `items` newest first, `unread`, `unread_crit`, `dnd`, `inhibited`, `suppressed` (`""|dnd|inhibited|fullscreen|locked`).
- **Calls** (added to `ghost_proto::calls::ALL`, 14 → 19): `notifications.invoke {id, key}`, `notifications.dismiss {id}`, `notifications.clear {}`, `notifications.read {ids:[…]}` or `{all:true}`, `notifications.dnd {on}`.
- **Config:** `[notifications] history_limit = 200`, `popup_timeout_s = 6`, `max_popups = 3`, `icons = true`; `[shortcuts] notifications = "Meta+N"`. `topics::BUILTIN` grows by `notifications` (26 → 27).
- **Server facts:** capabilities exactly `["body", "body-markup", "body-hyperlinks", "actions", "persistence", "icon-static", "inhibitions"]`; `GetServerInformation` → `("ghost-shell", "ghost-shell", <crate version>, "1.2")`; close reasons expired 1, dismissed 2, closed by call 3; dedupe window 1 000 ms with error name `org.freedesktop.Notifications.Error.ExcessNotificationGeneration`; the app's `expire_timeout` is ignored.
- **Files:** marker `$XDG_STATE_HOME/ghost-shell/notifications-server`; history `$XDG_STATE_HOME/ghost-shell/notifications.json`; images `$XDG_RUNTIME_DIR/ghost-shell/notify/<id>.png`; drop-in `$XDG_CONFIG_HOME/systemd/user/plasma-plasmashell.service.d/ghost-shell-notifications.conf`; Plasma's corner in `$XDG_CONFIG_HOME/plasmanotifyrc`, `[Notifications] PopupPosition` (`BottomRight` with a top bar, `TopRight` with a bottom bar).
- **Toast values** (design px, mockup CSS `.toasts`, `.toast`, `.alert`, `.life`):
  - the stack: 360 wide, 12 from the right edge, 8 past the bar, toasts 8 apart, newest nearest the bar; a bottom bar flips it;
  - a toast is a `Panel` with fill `ground` at 96 %, label the app name (UPPERCASE), meta the age (`NOW`, then `Fmt.ago`), header 22 high; body padding 4 12 12;
  - row: icon 20 (`AppIcon`, mono/colour per `bar.icons`, hidden when `notifications.icons = false`) or the sender's image 40 (chamfer 5), 10 apart; summary 10 px / 600 `hi`; body 10 px `text`, at most two lines (line height 1.45), elided; a drawn `cross` 10 px `dim` at the right;
  - action chips 8 px below: 1 px `hair` border, padding 2 × 8, 8.5 px tracking 0.14 em `text`, 6 apart; the first (or the `default`) chip has a `signal` border and `hi` text;
  - the life line: 24 cells 2 high, 2 apart, inside padding 0 12 4, lit `signal`, unlit `signal` at 8 %; it empties over `popup_timeout_s` in held steps on the seconds clock, holds while hovered, and is absent on critical toasts;
  - a `warn` or `crit` record is an `AlertBox` (`level`, `chipKey: "alert"`, `alertKey: "notif:" + id`, `trailing` the countdown when the body is `HH:MM:SS`) filling the body; its label and meta stay;
  - `+N WAITING` 8.5 px tracking 0.14 em `dim`, right-aligned, under the stack.
- **Centre values** (design px, mockup CSS `.centre`, `.search`, `.tools`, `.list`, `.foot`):
  - 392 × 560, fill `ground` (opaque, like the launcher), header label `NOTIFICATIONS`, tag `notif`, meta `<n> TOTAL · <c> CRIT · <w> WARN` (counts in `crit`/`warn`, omitted at 0);
  - search: 1 px `hair` border, padding 3 × 8, margin 2 0 8, the `chevron-right` glyph in `signal`, the query in `hi` with the block cursor (launcher's), placeholder `FILTER` in `dim`, an `ESC` chip;
  - tools: chips `MARK ALL READ`, `CLEAR`, then `DND` at the right; 8.5 px tracking 0.14 em `dim`, 1 px `hair` border, padding 2 × 8, 6 apart; `DND` in `warn` while on;
  - rows: columns 76 | stretch | 28, 8 apart, padding 4 6 4 0, 1 px `hair2` bottom rule; app 9 px / 600 tracking 0.1 em `signal`; message `summary — body_text` on one line, elided, 10 px `text` (`hi` when unread, the body part `dim`); age 9 px `dim` right-aligned; warn/crit rows: a 3 px left edge in the colour, 6 px extra left padding, app and message in the colour; selected: fill `hair` at 45 %, the age replaced by the `cross` glyph 7 px `dim`;
  - footer: `<shown> SHOWN · <unread> NEW` and `ENTER OPEN · DEL DISMISS`, 8.5 px tracking 0.14 em `dim`, padding-top 8; empty list: `NO SIGNALS` centred, tracking 0.2 em `dim`, and the footer's left reads `QUIET SINCE HH:MM` (the newest record's arrival, or the daemon's start);
  - with `server != "ghost-shell"` the list reads `PLASMA IS SERVING NOTIFICATIONS` (or `NO SERVER`) and the tools are hidden;
  - rows carry no icon, in the centre and in the widget (the mockups have none); `notifications.icons` governs toasts only.
- **Bell values** (mockup `.badge`): the `bell` glyph 13 px like the other tray icons, `dim` with no badge at 0 unread, `signal` with the badge otherwise, `crit` icon and badge while `unread_crit > 0`; the badge is 8 px / 700 `ground` text on a solid square, padding 0 3, line 10, offset 7 right and 6 up from the icon's corner, two digits at most (`99` cap); DND: the `bell-off` glyph, `dim`, badge in `dim`. Tooltip `Notifications · <n> unread`, or `Notifications · Plasma serving`.
- **Widget values:** the reference mockup's panel: header label `Notifications`, tag `notif`, meta `<unread> NEW`; the search row `.nsearch` (the `search` glyph, placeholder `FILTER NOTIFICATIONS`, a `/` chip); rows as the centre's; footer `<n> TOTAL · <c> CRIT · <w> WARN` and `SCROLL` with the `chevron-down` glyph.
- **Glyphs** (add to `Glyph.qml`, 12-unit box, stroked): `bell`: `M2.5 9V5.5a3.5 3.5 0 0 1 7 0V9zM1.5 9h9M5 10.5h2`; `bell-off`: the same plus `M1.5 1.5l9 9`; `cross`: `M3 3l6 6M9 3l-6 6`.
- **Read rules:** the centre marks everything read when it closes; the widget marks everything read after 2 000 ms continuously visible with the desktop uncovered, no greeting playing and the session unlocked; a record whose toast ends while the widget is visible is read then; invoke, dismiss and `MARK ALL READ` mark read.
- **Toast rules:** a record toasts when `held` is false, `arrived_s >= shellStartS`, it is not in the shell's shown set, and `suppressed == ""`; at most `max_popups` at once, the rest wait in arrival order; a critical toast is never pushed out by a normal one; `replaces_id` updates in place without re-scrambling, restarting the life line; a `crit` level that arrives by replacement re-arms the alert attention (a new `alertKey`).
- **Logging** (always on, one line per event): `notifications: owning org.freedesktop.Notifications`, `notifications: queued behind <owner pid>`, `notifications: plasma serving (no marker)`; `toast <id> <app>`, `centre opened` / `centre closed`; `call notifications.<x>` at debug.
- **Budgets:** the bell and the centre count inside the shell's §15 budgets; a normal toast ≤ 0.1 % of a core averaged over its 6 s; ghostd idle unchanged; every task that adds a surface or a feed reports its CPU and Rss deltas, and a check past O1 stops the task.

## Review Focus

1. **A client that floods.** A looping app sends the same `Notify` 50 times a second, or 10 000 distinct ones. The dedupe refuses the repeats, the cap keeps the file at 200 plus the criticals, the topic is published at most once per change, and the shell stays responsive. *Tests: Task 4 (`identical_notify_within_a_second_is_refused`, `cap_keeps_the_newest_and_every_unread_critical`), Task 6 (`a_burst_of_notifies_publishes_once_per_record`).*
2. **Hints and bodies that lie.** `image-data` with a rowstride shorter than its width, a 20 000 × 20 000 image, a body with `<script>` or an unterminated tag, a summary with invalid UTF-8, a `urgency` hint of the wrong type. Nothing panics, the record arrives without the bad part, and the body cannot inject markup into the toast. *Tests: Task 5 (`bad_image_rows_are_logged_not_written`, `oversized_image_is_dropped`, `wrong_typed_hints_are_ignored`), Task 4 (`markup_keeps_only_the_allowed_tags`, `invalid_markup_is_escaped_whole`).*
3. **The name dance in its odd states.** ghostd restarts while retired with plasmashell up (queued, `server = "plasma"`, READY still sent); the marker exists but plasmashell took the name first (queued, inherited on its next restart); restore with ghostd stopped; two ghostds (the second has no socket and must not request). *Tests: Task 5 (`queues_behind_a_holder_that_forbids_replacement_and_inherits`, `ready_is_sent_while_queued`), Task 8 (sandbox: `restore_with_ghostd_stopped`).*
4. **Read state against the greeting and the lock.** A notification arrives while locked, the unlock greeting must say `1 NEW SIGNALS` and the lock must have said `SIGNALS 1 NEW`; the dashboard behind the greeting must not clear it first; a shell restart must not replay toasts. *Tests: Task 9 (`toasts_only_records_after_shell_start`, `held_records_never_toast`), Task 13 (`widget_marks_read_only_after_dwell_uncovered_and_unlocked`, `greeting_reads_unread_before_the_widget`), Task 7 (`signals_counts_unread_arrivals_since_lock`).*
5. **Acting on what is gone.** Invoking a dismissed id, an action key the record never offered, a sender that has exited (no listener for `ActionInvoked`), a window to raise that closed. Each is a no-op with one log line, never an error to the shell. *Tests: Task 6 (`invoke_on_unknown_id_is_ignored`, `invoke_with_unoffered_key_is_ignored`), Task 9 (`window_to_raise_is_null_when_none_matches`).*

---

### Task 1: Spike A — inheriting the name from plasmashell, and Plasma's jobs

**Files:**
- Create: `spikes/notifications-takeover/` (`run.sh`, `claim.py` or a small Rust bin, `job.sh`), `docs/superpowers/notes/<YYYY-MM-DD>-notifications-takeover-spike.md`
- Reuse: `spikes/plasma-hidden/sandbox.sh` (its sandbox plasmashell on private buses; add `org.kde.plasma.Notifications` is already in its service dir)

**Interfaces:**
- Produces: a note answering, with evidence (bus owner pids, screenshots, log lines):
  1. **Inheritance:** a claimant requesting the name with `REPLACE_EXISTING`, no `ALLOW_REPLACEMENT`, no `DO_NOT_QUEUE`, while the sandbox plasmashell owns it, gets `IN_QUEUE`; after `sandbox.sh plasma-restart` the claimant owns it (`GetNameOwner` = its unique name); plasmashell's log shows `Failed to register Notification service on DBus`; the name stays with the claimant for 2 minutes, through opening the hidden tray's notification page (`sandbox.sh popup overflow`), and through a second plasmashell restart. `notify-send` in the sandbox reaches the claimant. When the claimant releases, plasmashell does not retake until restarted; after its restart it serves again.
  2. **Jobs:** with the claimant owning the name, a KIO copy in the sandbox (`kioclient copy` of a 2 GiB sparse file, or a `JobViewV2` session driven by `busctl`) shows Plasma's job popup; where it opens; and `kwriteconfig6 --file plasmanotifyrc --group Notifications --key PopupPosition BottomRight` plus a plasmashell restart moves it to the bottom-right. Record the exact key values.
  3. **Login order:** with a drop-in `After=ghostd.service` + `Wants=ghostd.service` on `plasma-plasmashell.service` and `Type=notify` on `ghostd.service`, `systemd-analyze --user verify` of copies of both units is clean and `systemctl --user show -p After` (against a copied unit dir via `SYSTEMD_UNIT_PATH`, never the user's) lists ghostd.
  4. **Raising by the bridge:** in the sandbox's KWin with ghostd's desktop script loaded, `windows.activate` on a known id brings a Qt test window to the front after another window was active (reuse `spikes/bar-bridge`'s harness).
  5. **Real payloads** (only if the user says yes, asked by the controller): a read-only `dbus-monitor --session "interface='org.freedesktop.Notifications',member='Notify'"` on the user's bus, stopped by pid, while the user triggers one KDE Connect, one Discord, one Firefox and one Spotify notification; the hints and action lists go in the note (bodies redacted).

- [ ] **Step 1: Build the claimant.** A 40-line Python (`uv run --no-project --with dbus-next python -I claim.py <flags>`) or a Rust example that requests the name with the given flags, serves a minimal `Notify` that prints its arguments, and holds until SIGTERM.
- [ ] **Step 2: Run questions 1–4** in the sandbox (`sandbox.sh up`, never the user's buses), capturing `GetNameOwner`, `ListQueuedOwners`, plasmashell's journal lines and `sandbox.sh shot` PNGs.
- [ ] **Step 3: Ask the user about question 5** and run it only on yes.
- [ ] **Step 4: Write the note** with a table of answers, the exact flags, keys and unit text that worked, and `sandbox.sh down`.
- [ ] **Step 5: Commit**

```bash
git add spikes/notifications-takeover docs/superpowers/notes/<YYYY-MM-DD>-notifications-takeover-spike.md
git commit -m "docs(spike): inherit org.freedesktop.Notifications from plasmashell and keep its job popups"
```

**Stop point:** a "no" on question 1 or 2 without a working variant stops the plan; the findings go to the user.

---

### Task 2: Spike B — keyboard focus on the desktop layer

**Files:**
- Create: `spikes/desktop-focus/focus.qml`, `spikes/desktop-focus/run.sh`; a section in Task 1's note.

**Interfaces:**
- Produces: whether a `PanelWindow` on `WlrLayer.Bottom`, namespace `desktop`, can switch `WlrLayershell.keyboardFocus` to `OnDemand` on click and receive key events on KWin 6.7.5, and whether switching it back to `None` on Esc returns focus to the window that had it.

- [ ] **Step 1: Write `focus.qml`:** the desktop surface with a `TextInput` that sets `keyboardFocus: OnDemand` on click and logs every key, plus a second ordinary window from `kdialog --msgbox` to take focus from.
- [ ] **Step 2: Run it** in `scripts/headless-kwin.sh 3b-t2`, drive clicks and keys with `scripts/fake-keys.py`, read the log.
- [ ] **Step 3: Record the answer** in the spike note. Task 13 reads it: yes → the widget's field takes focus; no → clicking the field opens the centre with its field focused.
- [ ] **Step 4: Commit**

```bash
git add spikes/desktop-focus docs/superpowers/notes/<YYYY-MM-DD>-notifications-takeover-spike.md
git commit -m "docs(spike): check on-demand keyboard focus on the desktop layer"
```

---

### Task 3: Feeds can watch topics

**Files:**
- Modify: `crates/ghostd/src/server/publish.rs`, `crates/ghostd/src/server/state.rs` (or wherever the hub publishes), `crates/ghostd/src/server/mod.rs`; every test that builds a `FeedContext` by hand (`crates/ghostd/tests/brightness.rs`, `batteries.rs`, `desktop.rs`, `links.rs`, `ticker.rs`, `recent.rs`, `actions.rs`, `agents.rs`, `windows.rs`, `common/bus.rs`, and any other `FeedContext {` site: `rg -l 'FeedContext \{' crates/ghostd/tests`)
- Test: `crates/ghostd/tests/server_sampling.rs` (or a new `crates/ghostd/tests/topic_updates.rs`)

**Interfaces:**
- Produces:
  - `pub struct TopicUpdate { pub topic: Arc<str>, pub value: Arc<Value> }` in `server/publish.rs`;
  - `FeedContext.topics: broadcast::Receiver<TopicUpdate>`;
  - `pub(super) const TOPIC_UPDATE_BACKLOG: usize = 256`;
  - the hub sends one `TopicUpdate` for every value it publishes (sampled, feed or provider), after it caches it;
  - `pub async fn next_topic(topics: &mut broadcast::Receiver<TopicUpdate>, name: &str) -> Arc<Value>` in `feeds/mod.rs`: the next value of `name`, skipping others and `Lagged` errors (logged at debug).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn every_published_value_reaches_feeds()        // a provider publishes x.y; a feed's `topics` yields ("x.y", value)
#[tokio::test] async fn sampled_topics_reach_feeds_too()              // cpu after one tick
#[tokio::test] async fn a_feed_that_never_reads_does_not_block_publishing()   // 1 000 publishes with an unread receiver complete at once
#[tokio::test] async fn next_topic_skips_other_topics_and_lag()
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test topic_updates`. Expected: FAIL (no field).
- [ ] **Step 3: Implement.** The sender lives beside the publisher in the hub's state; `FeedContext` is built with `sender.subscribe()`. Fix every test site that builds a `FeedContext`.
- [ ] **Step 4: Run the workspace tests and the checks.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/server crates/ghostd/src/feeds/mod.rs crates/ghostd/tests
git commit -m "feat(ghostd): let feeds watch every published topic"
```

---

### Task 4: The record, the topic, the calls, the config, and the history rules

**Files:**
- Create: `crates/ghostd/src/feeds/notifications/mod.rs` (module decl only for now), `crates/ghostd/src/feeds/notifications/history.rs`, `crates/ghostd/src/feeds/notifications/markup.rs`
- Modify: `crates/ghost-proto/src/snapshot.rs`, `crates/ghost-proto/src/lib.rs` (`topics`, `calls`), `crates/ghost-proto/src/config.rs` (`Notifications`, `Shortcuts`), `crates/ghostd/src/config/default.toml`, `crates/ghostd/src/feeds/mod.rs`
- Test: unit tests in `history.rs` and `markup.rs`; `crates/ghostd/tests/config_load.rs` (the new keys)

**Interfaces:**
- Produces (`ghost_proto`):
  - `Notification`, `NotificationAction { key, label }`, `Urgency { Low, Normal, Critical }`, `NotificationLevel { None, Warn, Crit }`, `NotificationsSnapshot` and `NotificationServer { None, Plasma, GhostShell }` as the Global Constraints give them (`serde(rename_all = "snake_case")`, `ghost-shell` via `rename`);
  - `topics::NOTIFICATIONS = "notifications"` in `BUILTIN`;
  - `calls::NOTIFICATIONS_INVOKE|DISMISS|CLEAR|READ|DND` in `ALL`;
  - `Notifications { history_limit: u32, popup_timeout_s: u32, max_popups: u32, icons: bool }` with defaults 200, 6, 3, true; `Shortcuts.notifications: String` default `"Meta+N"`; `default.toml` documents both.
- Produces (`history.rs`, pure, no I/O):
  - `pub struct Incoming { app, desktop_entry, app_icon, image, summary, body, body_text, urgency, level, actions, has_default, pid, resident, transient, replaces_id: u32 }`;
  - `pub struct History` with `pub fn new() -> Self`, `pub fn from_json(&str) -> Result<Self, serde_json::Error>`, `pub fn to_json(&self) -> String` (records, read flags, `dnd`, `next_id`);
  - `pub fn notify(&mut self, incoming: Incoming, now_s: u64, held: bool, limit: usize) -> Result<Outcome, Excess>` where `Outcome { id: u32, replaced: bool, kept: bool }` (`kept` false for `transient`) and `Excess` is the dedupe refusal: same `app`, `summary`, `body`, `actions` and `urgency` as the previous accepted call within 1 000 ms (`now_ms` is passed too, or `notify` takes `now_ms: u64` and derives seconds);
  - `pub fn close(&mut self, id: u32) -> bool`; `pub fn clear(&mut self)` keeps unread criticals; `pub fn mark_read(&mut self, ids: Option<&[u32]>)`; `pub fn set_dnd(&mut self, on: bool)`; `pub fn dnd(&self) -> bool`;
  - `pub fn items(&self) -> &[Notification]` newest first; `pub fn get(&self, id: u32) -> Option<&Notification>`; `pub fn counts(&self) -> Counts { unread, unread_crit }`; `pub fn unread_since(&self, since_s: u64) -> u32`;
  - the cap: after each accepted call, drop the oldest records beyond `limit`, never an unread critical (`urgency == Critical && !read`);
  - a replacement keeps `arrived_s`, sets `read = false`, replaces content; an unknown `replaces_id` is a new record.
- Produces (`markup.rs`): `pub fn sanitise(body: &str) -> (String, String)` → (markup subset, plain text) per call 7.

- [ ] **Step 1: Write the failing tests**

```rust
// history.rs
#[test] fn new_record_gets_the_next_id_and_is_unread()
#[test] fn replacement_keeps_arrival_and_resets_read()
#[test] fn unknown_replaces_id_is_a_new_record()
#[test] fn identical_notify_within_a_second_is_refused()           // 999 ms → Excess; 1 001 ms → ok
#[test] fn cap_keeps_the_newest_and_every_unread_critical()        // limit 3, 5 records, one old unread crit → 4 kept
#[test] fn transient_is_not_kept()                                  // Outcome.kept false, items unchanged
#[test] fn clear_keeps_unread_criticals()
#[test] fn mark_read_all_and_by_ids()
#[test] fn counts_follow_read_and_level()
#[test] fn unread_since_counts_arrivals_after_the_moment()
#[test] fn json_round_trips_records_dnd_and_next_id()
#[test] fn held_is_stamped_once_and_never_changes_on_replacement()
// markup.rs
#[test] fn markup_keeps_only_the_allowed_tags()                     // "<b>x</b><script>y</script><a href='u'>z</a>" → "<b>x</b>y<a href=\"u\">z</a>", "xyz"
#[test] fn invalid_markup_is_escaped_whole()                        // "a <b c" → "a &lt;b c", "a <b c"
#[test] fn entities_are_decoded_in_plain_text()                     // "&amp;&lt;" → "&<"
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd notifications`. Expected: FAIL.
- [ ] **Step 3: Implement** the proto types, the config keys, `history.rs` and `markup.rs`. `markup.rs` is a small hand-written scanner over `<`/`>` (no new crate).
- [ ] **Step 4: Run the workspace tests and the checks.** `config_load.rs` gains `notifications_and_shortcut_defaults_apply`. Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src crates/ghostd/src/feeds/notifications crates/ghostd/src/feeds/mod.rs crates/ghostd/src/config/default.toml crates/ghostd/tests/config_load.rs
git commit -m "feat(ghostd): add the notification record, topic, calls and history rules"
```

---

### Task 5: The D-Bus server, the name, images and readiness

**Files:**
- Create: `crates/ghostd/src/feeds/notifications/server.rs`, `crates/ghostd/src/feeds/notifications/images.rs`, `crates/ghostd/src/sdnotify.rs`
- Modify: `crates/ghostd/Cargo.toml` (`png`), `Cargo.toml` (workspace dep), `crates/ghostd/src/lib.rs`
- Test: `crates/ghostd/tests/notifications_server.rs`, stub client in `crates/ghostd/tests/common/notify_client.rs`; unit tests in `images.rs`

**Interfaces:**
- Produces (`server.rs`):
  - `pub struct Request` (an enum): `Notify { incoming: Incoming, reply: oneshot::Sender<Result<u32, Excess>> }`, `Close { id: u32 }`, `Inhibit { who: String, cookie: u32 }`, `UnInhibit { cookie: u32 }`, `InhibitorGone { unique_name: String }`;
  - `pub struct Server { inbox: mpsc::Sender<Request> }` with `#[interface(name = "org.freedesktop.Notifications")]`: `notify(app_name, replaces_id, app_icon, summary, body, actions: Vec<String>, hints: HashMap<String, OwnedValue>, expire_timeout) -> fdo::Result<u32>`, `close_notification(id)`, `get_capabilities() -> Vec<String>`, `get_server_information() -> (String, String, String, String)`, `inhibit(desktop_entry, reason, hints) -> fdo::Result<u32>`, `un_inhibit(cookie)`, property `inhibited: bool`, signals `notification_closed(id, reason)`, `action_invoked(id, key)`;
  - `fn parse_hints(hints, app_icon, desktop_entry) -> Incoming` reads `urgency` (byte, wrong type ignored), `desktop-entry`, `resident`, `transient`, `x-ghost-shell-level`, `image-data`/`image_data`/`icon_data` (decoded through `images.rs`), `image-path`/`image_path`; the caller's pid from `header.sender()` via `fdo::DBusProxy::get_connection_unix_process_id`;
  - `pub enum Claim { Owned, Queued { owner: String }, NotRequested }`; `pub async fn claim(bus: &Connection) -> zbus::Result<Claim>` requests with `RequestNameFlags::REPLACE_EXISTING` only, and `pub async fn release(bus: &Connection)`;
  - `pub async fn serve(bus: &Connection, inbox: mpsc::Sender<Request>) -> zbus::Result<()>` registers the object at `/org/freedesktop/Notifications` before any name request.
- Produces (`images.rs`): `pub struct ImageData { width: i32, height: i32, rowstride: i32, has_alpha: bool, bits_per_sample: i32, channels: i32, data: Vec<u8> }` with `pub fn from_variant(&Value) -> Option<Self>`; `pub fn write_png(dir: &Path, id: u32, image: &ImageData) -> Result<PathBuf, ImageError>` (call 6's limits); `pub fn prune(dir: &Path, keep: &[u32])`.
- Produces (`sdnotify.rs`): `pub fn ready()` writes `READY=1` to `$NOTIFY_SOCKET` with `nix::sys::socket` (datagram, abstract or path), a no-op without the variable; logs a warn on failure.

- [ ] **Step 1: Write the failing tests.** The feed is not wired yet, so the tests drive `serve` + `claim` directly with a `PrivateBus`, a receiving task on `inbox`, and a `zbus::Proxy` client:

```rust
#[tokio::test] async fn notify_reaches_the_inbox_with_parsed_hints()        // urgency 2 → Critical, desktop-entry, resident
#[tokio::test] async fn capabilities_and_server_information_are_exact()
#[tokio::test] async fn close_notification_queues_a_close()
#[tokio::test] async fn inhibit_sets_the_property_and_uninhibit_clears_it()
#[tokio::test] async fn an_inhibitor_that_disconnects_is_released()         // InhibitorGone on NameOwnerChanged
#[tokio::test] async fn claim_takes_the_name_from_a_holder_that_allows_replacement()
#[tokio::test] async fn queues_behind_a_holder_that_forbids_replacement_and_inherits()   // holder with DO_NOT_QUEUE|no ALLOW_REPLACEMENT; drop it → Owned
#[tokio::test] async fn ready_is_sent_while_queued()                        // NOTIFY_SOCKET = a temp datagram socket; receives "READY=1"
#[tokio::test] async fn wrong_typed_hints_are_ignored()                     // urgency as a string → Normal
// images.rs
#[test] fn rgba_rows_become_a_png()
#[test] fn bad_image_rows_are_logged_not_written()                          // rowstride < width*channels
#[test] fn oversized_image_is_dropped()
#[test] fn prune_removes_files_not_in_keep()
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test notifications_server`. Expected: FAIL.
- [ ] **Step 3: Implement** `server.rs`, `images.rs`, `sdnotify.rs`; add `png` to the workspace.
- [ ] **Step 4: Run the tests and the checks.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghostd/Cargo.toml crates/ghostd/src/lib.rs crates/ghostd/src/sdnotify.rs crates/ghostd/src/feeds/notifications crates/ghostd/tests/notifications_server.rs crates/ghostd/tests/common
git commit -m "feat(ghostd): serve org.freedesktop.Notifications and inherit the name from plasmashell"
```

---

### Task 6: The notifications feed: history, calls, topic, persistence

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/mod.rs`, `crates/ghostd/src/main.rs` (`host_feeds` gains the feed with the marker and state paths; the fake daemon gets Task 9's fake instead), `crates/ghostd/src/notify.rs` (call 3: `pub struct Outgoing { app, summary, body, urgency, replaces_id, app_icon, hints }`, `pub async fn send(bus, &Outgoing) -> zbus::Result<u32>`; `send_warning` keeps its signature and uses it)
- Test: `crates/ghostd/tests/notifications.rs`

**Interfaces:**
- Consumes: Task 3's `topics`, Task 4's `History`, Task 5's `Server`, `claim`, `images`, `sdnotify::ready`.
- Produces:
  - `pub struct Paths { marker: PathBuf, history: PathBuf, images: PathBuf }` with `pub fn default_paths() -> Paths` (`XDG_STATE_HOME`/`HOME`, `XDG_RUNTIME_DIR`);
  - `pub fn feed(paths: Paths) -> Feed` (session bus) and `pub fn feed_on(bus: Option<Connection>, paths: Paths, now: impl Fn() -> u64) -> Feed` for tests;
  - **the task:** loads the history (a missing or unreadable file starts empty with a warn); if the marker exists, `serve` then `claim`, logging per Global Constraints, else `server = None` with `plasma` when `GetNameOwner` succeeds and `none` otherwise (re-read on every `NameOwnerChanged` of the name); calls `sdnotify::ready()` once the claim is answered or skipped; publishes the topic at start and on every change; saves the file at most once a second after a change and prunes images with it;
  - **requests:** `Notify` → `history.notify(incoming, now, held, limit)` with `held = suppressed != ""`; a `transient` record (`Outcome.kept == false`) is held by the feed in a `passing: Vec<(Notification, Instant)>` prepended to `items` and dropped `popup_timeout_s` after its arrival (one `tokio::time::sleep` per entry), so it toasts but never reaches the file; `Close` → `history.close` + `NotificationClosed(id, 3)`; inhibitions counted;
  - **calls:** `notifications.invoke {id, key}` → `ActionInvoked(id, key)` (only if `key` is `default` with `has_default`, or one of the record's actions) then, unless `resident`, `close` + `NotificationClosed(id, 2)`; `dismiss` → `close` + `NotificationClosed(id, 2)`; `clear`; `read`; `dnd`;
  - **suppression inputs** (Task 3's updates): `windows` → `fullscreen = any(active && fullscreen)`; `session` → `locked`; `suppressed` is the first of `dnd`, `inhibited`, `fullscreen`, `locked` that holds;
  - the `calls` receiver may lag: log and continue.

- [ ] **Step 1: Write the failing tests.** The real feed on a `PrivateBus` with Task 5's client, a temp dir for `Paths`, demand set to `notifications`, and the hub's `topics` sender in hand:

```rust
#[tokio::test] async fn notify_publishes_the_record_unread()
#[tokio::test] async fn a_burst_of_notifies_publishes_once_per_record()         // 100 distinct → 100 topic messages, no extra
#[tokio::test] async fn invoke_emits_action_invoked_and_closes_unless_resident()
#[tokio::test] async fn invoke_on_unknown_id_is_ignored()
#[tokio::test] async fn invoke_with_unoffered_key_is_ignored()
#[tokio::test] async fn dismiss_clear_read_and_dnd_change_the_topic()
#[tokio::test] async fn history_survives_a_restart()                              // feed stopped, started again on the same Paths → same items, dnd, next id
#[tokio::test] async fn transient_record_passes_through_items_and_the_file()      // in items for popup_timeout_s, never in notifications.json
#[tokio::test] async fn fullscreen_active_window_holds_new_records()              // windows update → held true, suppressed "fullscreen"
#[tokio::test] async fn locked_session_holds_new_records()
#[tokio::test] async fn without_the_marker_server_reads_plasma_or_none()
#[tokio::test] async fn with_the_marker_the_name_is_owned_and_ready_sent()
#[tokio::test] async fn images_are_pruned_with_the_history()
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test notifications`. Expected: FAIL.
- [ ] **Step 3: Implement** the feed and wire it in `main.rs`.
- [ ] **Step 4: Run the tests and the checks.** Expected: PASS.
- [ ] **Step 5: Measure** ghostd's idle CPU and Rss with the feed, marker absent and present (private bus). Report the deltas.
- [ ] **Step 6: Commit**

```bash
git add crates/ghostd/src/feeds/notifications crates/ghostd/src/main.rs crates/ghostd/src/notify.rs crates/ghostd/tests/notifications.rs crates/ghostd/tests/common
git commit -m "feat(ghostd): keep notification history, serve the shell's calls and publish the topic"
```

---

### Task 7: Built-in sources, the shortcut, and the gate's count

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/mod.rs` (sources), `crates/ghostd/src/feeds/shortcuts.rs` and `desktop.rs` (the `notifications` id, `DesktopSnapshot.notifications_seq`), `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/gate.rs`, `crates/ghostd/src/main.rs`
- Test: `crates/ghostd/tests/notifications_sources.rs`, `crates/ghostd/tests/shortcuts.rs`, `crates/ghostd/tests/desktop.rs`, unit tests in `gate.rs`

**Interfaces:**
- Produces:
  - **INMET:** on each `weather.alerts` update, every alert id not seen since start raises `notify::send` with app `WEATHER`, summary `<title> · <severity>`, body the risks joined by ` · `, urgency `Critical` for `AlertLevel::Crit`, else `Normal` with hint `x-ghost-shell-level = "warn"`; the same id updated reuses its notification id as `replaces_id`.
  - **Dead drive:** on each `disks` update, a drive with `state == Dead` whose model+serial has not been raised since start raises app `STORAGE`, summary `<MODEL> · CONTROLLER STATE DEAD`, urgency `Critical`.
  - **Shortcut:** `shortcuts.notifications` registered as id `notifications` ("Open the notification centre"); a press bumps `DesktopSnapshot.notifications_seq` whatever has focus, exactly as `launcher_seq`.
  - **Gate:** `gate::keep` takes a `watch::Receiver<Option<u32>>` named `signals`; `snapshot_ini` gains `[session]\nsignals=<n>\n` when `Some`; the notifications feed sets it to `Some(unread_since(lock_s))` while `locked`, `None` otherwise (through a `watch::Sender` passed from `main.rs`).

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn new_inmet_alert_is_notified_once_with_its_level()     // a fake Notifications server on the private bus records the call
#[tokio::test] async fn updated_alert_replaces_its_notification()
#[tokio::test] async fn dead_drive_is_notified_once_per_start()
#[tokio::test] async fn notifications_shortcut_bumps_the_seq_whatever_has_focus()
#[test]        fn signals_appears_in_the_gate_ini_only_while_some()
#[tokio::test] async fn signals_counts_unread_arrivals_since_lock()           // lock, 2 notifies, 1 read → Some(1); unlock → None
```

- [ ] **Step 2: Run them and watch them fail.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests and the checks.** Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src/snapshot.rs crates/ghostd/src/feeds crates/ghostd/src/gate.rs crates/ghostd/src/main.rs crates/ghostd/tests
git commit -m "feat(ghostd): raise INMET and dead-drive notifications, add Meta+N and the gate's signals count"
```

---

### Task 8: Handing the server over from Plasma, reversibly

**Files:**
- Modify: `scripts/plasma-panel.sh` (`retire-notifications`, `restore-notifications`, `status`), `packaging/systemd/ghostd.service` (`Type=notify`), `scripts/install.sh` (prints the step), `README.md` (install, the two retires), `spikes/plasma-hidden/test-plasma-panel.sh`
- Create: `packaging/systemd/plasma-plasmashell-after-ghostd.conf` (the drop-in's content)

**Interfaces:**
- Consumes: Task 1's note (flags, keys, unit text), Task 6's marker path.
- Produces:
  - **`retire-notifications`:** refuses if already retired; backs up `plasmanotifyrc` (or notes its absence) into the same `plasma-backup/<stamp>/` layout with its own manifest; writes the marker; installs the drop-in and runs `systemctl --user daemon-reload`; sets `PopupPosition` per `bar.position`; checks `shortcuts.notifications` is free in KGlobalAccel like the popup keys; restarts ghostd (`systemctl --user restart ghostd`, overridable by `GHOST_GHOSTD_RESTART` like the plasmashell commands), then plasmashell; verifies `GetNameOwner` of `org.freedesktop.Notifications` is ghostd's pid and prints it; a marker `notifications-retired` with the stamp.
  - **`restore-notifications`:** removes the marker and the drop-in, `daemon-reload`, puts `plasmanotifyrc` back, restarts ghostd then plasmashell, verifies plasmashell owns the name; works with ghostd stopped.
  - **`status`** prints both lines: `retired <stamp>` / `plasma panel in place` and `notifications retired <stamp>` / `plasma serving notifications`.
  - The same recovery trap, echoing and refusal rules as the panel commands.

- [ ] **Step 1: Extend the sandbox test** `spikes/plasma-hidden/test-plasma-panel.sh`: `retire-notifications` → the name's owner is the sandbox ghostd's pid, `status` says both, a `notify-send` lands in ghostd's topic (a UI client subscribed on its socket); `restore-notifications` → plasmashell owns it again, `plasmanotifyrc` byte-identical; `restore_with_ghostd_stopped` (stop the sandbox ghostd first) still restores and plasmashell serves. Expected before implementing: FAIL.
- [ ] **Step 2: Implement** the subcommands, the unit change, the drop-in file, `install.sh`'s hint and README.
- [ ] **Step 3: Run the sandbox test.** Expected: PASS, both restores byte-identical.
- [ ] **Step 4: Commit**

```bash
git add scripts/plasma-panel.sh scripts/install.sh packaging/systemd README.md spikes/plasma-hidden/test-plasma-panel.sh
git commit -m "feat(install): retire and restore Plasma's notification server beside the panel"
```

---

### Task 9: The shell's rules, the singleton, the glyphs and the fake feed

**Files:**
- Create: `shell/kit/GhostKit/lib/notifications.mjs`, `shell/kit/GhostKit/Notifications.qml` (singleton)
- Modify: `shell/kit/GhostKit/qmldir`, `shell/kit/GhostKit/Glyph.qml` (`bell`, `bell-off`, `cross`), `crates/ghostd/src/fake/feeds.rs` (a fake `notifications` feed), `shell/kit/GhostKit/lib/subscriptions.mjs` (nothing to change if the tray's manifest requires the topic; verify)
- Test: `shell/tests/notifications.test.mjs`

**Interfaces:**
- Produces (`notifications.mjs`, pure):
  - `bellState({server, unread, unreadCrit, dnd})` → `{glyph: 'bell'|'bell-off', colour: 'dim'|'signal'|'crit', badge: ''|'1'…'99', badgeColour, tip}`;
  - `toastQueue({items, shown, shellStartS, maxPopups, suppressed})` → `{show: [ids], waiting: n}` per the toast rules; `shown` is the set of ids already toasted or ended;
  - `filterRecords(items, query)` case-insensitive substring over `app`, `summary`, `body_text`;
  - `rowText(record)` → `{app: UPPERCASE app, summary, body: body_text, age}`; `countsLine(items)` → `18 TOTAL · 1 CRIT · 1 WARN` with zero parts omitted; `footerLeft(items, unread)` → `16 SHOWN · 3 NEW`; `quietSince(items, startS)`;
  - `windowToRaise(windows, record)` → window id or null (call 2);
  - `acceptNotificationsSeq(seen, desktop, connection)` like `acceptLauncherSeq`;
  - `moveSelection(index, key, length)` Up/Down clamped; `isCountdown(bodyText)` matches `^\d\d:\d\d:\d\d$`.
- Produces (`Notifications.qml`): `readonly property bool open`, `toggle()`, `open()`, `close()`, `property string query`, `property int index`, `property bool widgetVisible` (set by the dashboard widget while it is shown and the desktop is uncovered; read by the toasts), `property rect anchorRect` (set by the bell), `invoke(id, key)` (sends `notifications.invoke`, then `windows.activate` when `windowToRaise` finds one, then `close()`), `dismiss(id)`, `markAllRead()`, `clear()`, `toggleDnd()`; takes `desktop.notifications_seq`; logs `centre opened` / `centre closed`; `IpcHandler` target `notifications` (`toggle`, `type`, `key`, `status`) under `GHOST_DEBUG=1`.
- Produces (fake daemon): a `notifications` fake publishing the mockup's 18 records (`NOTIFS` in the reference mockup, with the dead drive critical and the INMET warn), `server: "ghost-shell"`, and answering the five calls by editing its copy, so smokes and snapshots have data; `GHOST_FAKE_NOTIFY_EVERY_S=<n>` adds a new record every n seconds for toast smokes.

- [ ] **Step 1: Write the failing tests**

```js
test('bell is dim with no badge at zero and crit while an unread critical remains', …)
test('bell shows plasma serving without the server', …)
test('toasts only records after shell start', …)                     // arrived_s < shellStartS → never shown
test('held records never toast', …)
test('at most max popups, criticals are never pushed out', …)        // 3 normals shown, a 4th normal waits; a crit replaces the oldest normal
test('suppressed holds everything and waiting counts them', …)
test('filter matches app summary and body case-insensitively', …)
test('counts line omits zero parts', …)
test('window to raise is null when none matches', …)
test('window to raise prefers app id over pid', …)
test('notifications seq is a toggle only after the first value on a connection', …)
```

- [ ] **Step 2: Run them and watch them fail.** Run: `node --test shell/tests/notifications.test.mjs`. Expected: FAIL.
- [ ] **Step 3: Implement** the library, the singleton, the glyphs and the fake feed.
- [ ] **Step 4: Run the node tests, the workspace tests and `make smoke`** (the fake daemon must still start). Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib/notifications.mjs shell/kit/GhostKit/Notifications.qml shell/kit/GhostKit/qmldir shell/kit/GhostKit/Glyph.qml shell/tests/notifications.test.mjs crates/ghostd/src/fake/feeds.rs
git commit -m "feat(shell): add the notification rules, singleton, glyphs and a fake feed"
```

---

### Task 10: The bell

**Files:**
- Modify: `widgets/tray/Widget.qml`, `widgets/tray/manifest.toml` (`requires` gains `notifications`), `widgets/tray/TrayItem.qml` (a `badge` property: text and colour, drawn per the bell values)
- Test: `shell/tests/tray.test.mjs` (the tray's order includes the bell), the smoke snapshot

**Interfaces:**
- Consumes: `Notifications` singleton, `bellState`, the `notifications` topic.
- Produces: the bell item after the SNI items, before the chevron; left click `Notifications.toggle()`, middle click `Notifications.toggleDnd()`; tip per the bell values; repaints only on count or DND changes (bind to the derived `bellState`, not to the whole topic value).

- [ ] **Step 1: Write the failing test** `test('the tray ends with the bell before the chevron', …)` against the tray's item order helper (add `trayOrder(builtins, sniCount)` to `tray.mjs` if there is none).
- [ ] **Step 2: Run it and watch it fail.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests and `make smoke`;** the snapshot shows the bell with the fake feed's badge `3` and, in a second run with `GHOST_FAKE_NOTIFY_DND=1`, the slashed bell.
- [ ] **Step 5: Commit**

```bash
git add widgets/tray shell/kit/GhostKit/lib/tray.mjs shell/tests/tray.test.mjs scripts/qml-smoke.sh
git commit -m "feat(widgets): add the notification bell to the tray"
```

---

### Task 11: Toasts

**Files:**
- Create: `shell/host/Toasts.qml`, `shell/host/Toast.qml`
- Modify: `shell/shell.qml`, `scripts/qml-smoke.sh` (`GHOST_SMOKE_TOASTS=1`)

**Interfaces:**
- Consumes: `toastQueue`, `AlertBox`, `AppIcon`, `Panel`, `FrameClock` (seconds for the life line), `Data.config.notifications`, `Data.config.bar.position`.
- Produces: a `LazyLoader`ed `PanelWindow` on the Overlay layer, namespace `utility`, `keyboardFocus: None`, anchored top-right (bottom-right with a bottom bar), sized to its column after `BarPopup`'s settle rule, active while `show.length > 0 || waiting > 0`; each `Toast` per the toast values; a toast ends on timeout (`shown` gains the id, `notifications.read {ids:[id]}` is sent when `Notifications.widgetVisible`), on the cross (`notifications.dismiss`), on a click (`Notifications.invoke(id, 'default')`), on a chip (`invoke(id, key)`), on middle click (dismiss); logs `toast <id> <app>`; not loaded with `GHOST_BAR=off`.

- [ ] **Step 1: Write the smoke.** `GHOST_SMOKE_TOASTS=1` runs the fake daemon with `GHOST_FAKE_NOTIFY_EVERY_S=2`, waits for three `toast` log lines, takes a snapshot (top and bottom bar), waits 7 s more and expects the first toast gone from a second snapshot (pixel diff of the stack region), and `smoke-holds` shows no clock hold once all have ended. Expected before implementing: FAIL (no `toast` lines).
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run the smokes.** Expected: PASS; by eye the snapshot matches the mockup's density A.
- [ ] **Step 4: Measure** the shell's CPU over one normal toast's 6 s and the Rss before and after the first toast. Report; past O1, stop.
- [ ] **Step 5: Commit**

```bash
git add shell/host/Toasts.qml shell/host/Toast.qml shell/shell.qml scripts/qml-smoke.sh
git commit -m "feat(shell): show notifications as toasts under the bar"
```

---

### Task 12: The centre

**Files:**
- Create: `shell/host/NotificationCentre.qml`, `shell/kit/GhostKit/NotificationRows.qml`
- Modify: `shell/kit/GhostKit/BarPopup.qml` (`alignRight`), `shell/shell.qml`, `shell/kit/GhostKit/qmldir`, `scripts/qml-smoke.sh` (`GHOST_SMOKE_CENTRE=1`)

**Interfaces:**
- Consumes: `Notifications` singleton, `filterRecords`, `rowText`, `moveSelection`, the launcher's key handling pattern (`LauncherPanel.qml`: `OnDemand`, `Window.onActiveChanged` close, `Keys.onPressed`).
- Produces:
  - `NotificationRows { property var items; property int selected; property bool showDismiss; signal activated(int id); signal dismissed(int id) }`, a `ListView` of rows per the centre values, scrollable by wheel, used by the centre and the widget;
  - the centre surface per the centre values, opened from the bell's anchor (the tray passes the bell's rect through the singleton: `Notifications.anchorRect`), closing on focus loss, Esc, a second bell click, and after `invoke`; typing filters; Up/Down/Enter/Delete; `MARK ALL READ`, `CLEAR`, `DND` chips; marks everything read on close (`notifications.read {all:true}`); header scrambles per opening (`FirstWrite.showView("centre")`).

- [ ] **Step 1: Write the smoke.** `GHOST_SMOKE_CENTRE=1`: IPC `toggle`, expect `centre opened`, snapshot (top and bottom bar), `type disc`, snapshot, `key Down`, `key Enter`, expect `call notifications.invoke` at the fake daemon and `centre closed`; `toggle` again with `GHOST_FAKE_NOTIFY_EMPTY=1` and snapshot the empty state. Expected before implementing: FAIL.
- [ ] **Step 2: Implement.**
- [ ] **Step 3: Run the smokes and `smoke-holds`** (open and close leaves no hold). Expected: PASS.
- [ ] **Step 4: Measure** Rss closed, after the first open, and after four open/close cycles (plateau, as the launcher's). Report.
- [ ] **Step 5: Commit**

```bash
git add shell/host/NotificationCentre.qml shell/kit/GhostKit/NotificationRows.qml shell/kit/GhostKit/BarPopup.qml shell/kit/GhostKit/qmldir shell/shell.qml scripts/qml-smoke.sh
git commit -m "feat(shell): add the notification centre under the bell"
```

---

### Task 13: The dashboard widget, the layouts and the greeting

**Files:**
- Create: `widgets/notifications/manifest.toml`, `widgets/notifications/Widget.qml`
- Modify: `crates/ghost-proto/src/config.rs` (`placeholder("Notifications", …)` → `slot("notifications", …)` in V1 and V2), `shell/tests/layout.test.mjs`, `shell/kit/GhostKit/lib/greeting.mjs` and `shell/host/Greeting.qml` (`subtitleText` gains `unread`), `shell/tests/greeting.test.mjs`, `shell/kit/GhostKit/lib/notifications.mjs` (`widgetReadDue`)
- Test: `shell/tests/notifications.test.mjs`, `shell/tests/greeting.test.mjs`, `shell/tests/layout.test.mjs`

**Interfaces:**
- Produces:
  - `widgets/notifications`: `kinds = ["panel"]`, `requires = ["notifications"]`, `animates = false`; the widget per the widget values, using `NotificationRows`; its search field takes focus per Task 2's answer (yes: `OnDemand` on click, `None` on Esc; no: click opens the centre with the field focused);
  - `widgetReadDue({visibleMs, covered, greetingPlaying, locked})` → true at `visibleMs >= 2000` with all three false; the widget calls `notifications.read {all:true}` once per becoming-due, and keeps `Notifications.widgetVisible` current;
  - `subtitleText({japanese, sessionId, unread})` appends `N NEW SIGNALS` when `unread > 0`; `Greeting.qml` passes `unread` only while the topic's `server === "ghost-shell"` (else 0), read **at the moment the subtitle is typed**, before any widget can mark read (the dashboard is hidden while the greeting plays, so the dwell cannot run).

- [ ] **Step 1: Write the failing tests**

```js
test('widget marks read only after dwell uncovered and unlocked', …)   // 1999 ms → false; 2000 ms covered → false; 2000 ms clear → true
test('greeting reads unread before the widget', …)                     // subtitleText with unread 3 → "… · 3 NEW SIGNALS"; 0 → nothing
test('greeting leaves signals out while plasma serves', …)              // server "plasma", unread 3 → nothing
test('default layouts place the notifications widget where the placeholder was', …)   // layout.test.mjs: slot widget 'notifications' at the same rect
```

- [ ] **Step 2: Run them and watch them fail.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests, `make smoke` and `make smoke-greeting`** (the greeting smoke's fake daemon publishes `unread: 2` and the log shows the subtitle with `2 NEW SIGNALS`). The V1 and V2 snapshots show the panel. Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add widgets/notifications crates/ghost-proto/src/config.rs shell/kit/GhostKit/lib shell/host/Greeting.qml shell/tests scripts/qml-smoke.sh scripts/smoke-greeting.sh
git commit -m "feat(widgets): add the notifications panel and the greeting's new-signals count"
```

---

### Task 14: Budgets, snapshots and the record

**Files:**
- Modify: `README.md` (Performance: the 3b figures; Configuration: the new keys)

- [ ] **Step 1: Snapshots** in headless KWins at `QT_SCALE_FACTOR=1.15`: the bell's four states; the toast stack (normal with icon and chips, warn boxed, crit boxed with image, `+2 WAITING`), top and bottom bar; the centre filled, filtered, empty with DND, and `server: "plasma"`; the widget in V1 and V2; `notifications.icons = false`; `japanese = "off"`. Check each by eye against the mockup; text never clips.
- [ ] **Step 2: Budget runs** (installed-style build, real daemon on a private bus, 60 s, three runs, median): idle with the bar (unchanged from 3a's figures within noise); one toast every 10 s over a covered desktop (≤ 0.5 % still); the centre open; Rss per state. **Past O1, stop and bring the numbers to the user.**
- [ ] **Step 3: Write README's Performance section** for 3b with the per-task deltas, and document the config keys and the two retire commands.
- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs(readme): record the notification server's and toasts' budget figures"
```

---

### Task 15: Install, the hand-over and acceptance with the user

A controller task: every live step asks the user first.

- [ ] **Step 1: Reinstall** from a clean detached worktree. ghostd restarts with `Type=notify` and no marker: the bell reads `Plasma serving`, Plasma's popups continue. Tell the user before restarting the services.
- [ ] **Step 2: Ask the user to retire notifications.** On yes, run `scripts/plasma-panel.sh retire-notifications`, show its echoed changes and the owner check, and tell the user that `restore-notifications` undoes it.
- [ ] **Step 3: The user's checklist:**
  - `notify-send` normal, critical, with an action (`-A open=Open`), with an image (`-i` a file path and a `--hint=image-path:`), with markup in the body;
  - a KDE Connect phone notification (with its picture) and a Discord message; clicking each raises the app;
  - a Dolphin copy of a large file shows Plasma's job popup in the other corner;
  - an INMET alert through the fake provider (`GHOST_FAKE_ALERT=1` on a dev provider against the live ghostd, read-only otherwise) toasts boxed in warn;
  - DND from the bell's middle click and from the centre; Meta+N; typing in the centre; Delete; Enter;
  - a fullscreen video: no toast, the bell counts;
  - lock with a notification pending: `SIGNALS` is not on the prompt lock style, so check `[session] signals=1` in `gate.ini`; unlock: the greeting reads `1 NEW SIGNALS`;
  - `systemctl --user restart ghostd`: history intact, name re-owned at once;
  - the dashboard list marks read after it has been visible for 2 s.
- [ ] **Step 3b: Ask the user** whether `~/scripts/storm_alert.py` and `storm-alert.timer` (inactive) may be removed now that INMET notifications come from ghostd (parent §18: ask before deleting). Act only on yes.
- [ ] **Step 4: Live budgets.** Read the live services' CPU over 60 s and their `smaps_rollup` Rss idle, with one toast, and with the centre open.
- [ ] **Step 5: Restore check** against copies of the configs (byte-identical), and a real `restore-notifications` + `retire-notifications` once if the user wants.
- [ ] **Step 6: Logout and login** (the user), then `busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus GetConnectionUnixProcessID s org.freedesktop.Notifications` must name ghostd's pid.
- [ ] **Step 7: Write `docs/superpowers/notes/<YYYY-MM-DD>-plan3b-acceptance.md`:** the checklist results, the user's decisions, the live figures, and what is deferred (inline reply, JobViewServer, per-app rules) for `docs/IDEAS.md`, which the controller adds there (unstaged, as always).
- [ ] **Step 8: Commit**

```bash
git add docs/superpowers/notes/<YYYY-MM-DD>-plan3b-acceptance.md
git commit -m "docs: record plan 3b acceptance"
```

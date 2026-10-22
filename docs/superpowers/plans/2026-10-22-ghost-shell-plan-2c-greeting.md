# ghost-shell Plan 2 — Part 2c: The Greeting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The §7 greeting plays on login, on unlock and on wake from sleep: full-screen over the open windows, typed like a terminal, ending in one scan pass that reveals the windows or a freshly scrambled dashboard, with no frame of the desktop or windows between the lock screen and the greeting.

**Architecture:**
- **Daemon (`ghostd`):** a new always-running feed publishes the `session` topic from KWin's `org.freedesktop.ScreenSaver` (session bus) and logind's `PrepareForSleep` (system bus). A pure state machine turns lock, unlock, sleep and resume into at most one greeting per return, counted by `greeting_seq`.
- **Shell:**
  - `lib/greeting.mjs` holds everything with a rule in it: the timed sequence as a pure `frameAt(plan, ms)`, the subtitle text, the easings, and the phase machine (`idle → armed → playing → idle`) driven by the `session` topic;
  - `shell/host/Greeting.qml` owns the phase, the login marker, the debug IPC and the reveal hook; it loads one `GreetingScreen` overlay per screen only while the phase is not `idle`, so an idle shell carries no greeting objects;
  - the overlay is a `WlrLayer.Overlay` surface. It is mapped as soon as the session locks or prepares for sleep, resting on ground + dots under the lock screen, so the first frame after an unlock is the greeting's;
  - while the greeting hides the dashboard, `Desktop` treats it as covered: the view is unloaded, the frame clock is suspended and the daemon collects only the always-subscribed topics. At the scan pass the shell resets `FirstWrite` and re-arms `AlertAttention`, then loads the view, so its texts scramble and its alerts blink as they are revealed.

**Tech Stack:**
- Rust 1.98, zbus 6 (already a dependency). No new crates.
- Quickshell 0.2.1 on Qt 6.11: `Quickshell.Wayland` (`WlrLayershell`, `WlrKeyboardFocus`), `Quickshell.Io` (`FileView`, `IpcHandler`).
- Node 24 for `.mjs` tests.
- KWin 6.7.5 and kscreenlocker 6.7.5 for the spike.

**Spec:** `docs/superpowers/specs/2026-10-20-ghost-shell-design.md` §7 (greeting), §5 (visual language, text scramble at line 143, alert blink §5.4), §14 (config), §15 (first frame within 500 ms, budgets), §17 (no flash between lock screen and greeting). The reference mockup `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html` is the visual and timing truth: CSS lines 56–63 (`.crton`, `.roll`, `.greet .ch` burn), 248 (`.cur`), 329–332 (`.greet`), and the script at 663–719 (`typeOut`, `powerOn`, `boot`). Read with:
- the Plan 2a outline this plan closes, `docs/superpowers/plans/2026-10-21-ghost-shell-plan-2-dashboard.md:1543–1557`;
- the Plan 2a ledger `.superpowers/sdd/2026-10-21-ghost-shell-plan-2-dashboard/progress.md`, carry-forwards **C11** (line 161) and **C19** (line 246), both closed here;
- the lock and login design `docs/superpowers/specs/2026-10-22-ghost-shell-lock-design.md` §6.5 and §11 spike 3, which this plan's Task 1 answers.

**Prerequisite:** Plan 2b is complete at `ce3e248`.

## Decisions (user, 2026-10-22)

1. **Alerts re-arm on the reveal (closes C19).** The greeting's reveal is a new showing for alerts as it is for scramble: every alert blinks for its attention window again.
2. **Under the lock, the overlay rests on ground + dots.** This matches the lock design's [call] that its exit scan ends on bare ground + dots (lock design §6.5). After an unlock or a wake the greeting starts at the tube power-on over the printed dots and skips the dot print. At login it plays the full §7 sequence.
3. **A new colour token `burn`**, default `#ffffff`, for the burn-in's starting colour and the tube line (the mockup's `#ffffff` and `#f2fbfa`), so every colour stays configurable (spec §14).
4. **Once per login.** A restart of the shell mid-session (a reinstall, `Restart=on-failure`) does not greet again.

## Calls

Marked **[call]**; the user may override any of them.

1. **[call] Lock state comes from `org.freedesktop.ScreenSaver` on the session bus** (`GetActive`, `ActiveChanged(b)` at `/ScreenSaver`, owned by `kwin_wayland` here), not from logind's session `Lock`/`Unlock` signals, which ask lockers to lock rather than report that the screen is locked. Sleep comes from logind `org.freedesktop.login1.Manager.PrepareForSleep(b)`.
2. **[call] The feed runs whether or not any client wants `session`.** A lock missed while no shell was connected would break the wake-plus-unlock pairing. It costs two signal subscriptions and one property read per change.
3. **[call] Greeting events.** `Unlock` when the screen unlocks; `Wake` when the screen unlocks after a resume that happened while it was locked, or at a resume with no lock. A wake and the unlock that follows are one event (spec §7). `LockOnResume` is at its default (on) here, so kscreenlocker locks before the machine sleeps; with it off, a frame of windows may show at resume, which this plan documents and does not engineer around.
4. **[call] `SESSION nnnn`** is the graphical session's logind id (`org.freedesktop.login1.User.Display`, `"2"` today), padded with zeros to 4 characters. When ghostd has not published it by the time the subtitle starts typing, the item is left out. `N NEW SIGNALS` waits for the notification server (Plan 3).
5. **[call] Subtitle with Japanese `off`** drops `おかえりなさい`. With Japanese off and no session id there is no subtitle line.
6. **[call] The login marker** is `$XDG_RUNTIME_DIR/ghost-shell-greeted`. The runtime directory is a tmpfs removed at logout and at reboot (`Linger=no` here), so an absent marker means the first shell start of this login.
7. **[call] Skip jumps to the end:** the overlay goes at once with no scan pass, and the dashboard (if uncovered) appears with its scramble. The overlay takes the keyboard exclusively only while it plays; under the lock it takes none, so it can never compete with the lock screen.
8. **[call] The greeting has its own 16 ms tick while it plays** (about 62.5 frames/s). The shortest mockup step is the scan pass's 26 ms, and the tube, burn-in and roll band move continuously. PrintSwap set the precedent that a transient sequence may run its own timer. Nothing ticks while the overlay is armed under the lock.
9. **[call] Other screens** get the overlay too, so no window shows on any screen first, but only ground, dots and the scan pass. Text, tube and roll band appear on the main screen only (this machine has one screen).
10. **[call] Losing ghostd disarms.** An armed overlay whose daemon connection drops goes back to `idle`, because nothing would ever tell it about the unlock. A flash of windows after a daemon crash beats an overlay stuck over the session.
11. **[call] Title fits the screen.** Line 1 keeps 56 design px unless the whole line (`Welcome back, ` + name) would be wider than the screen less two 12 px margins. Then its font scales down to fit, decided once from the full line so the size never changes while it types.
12. **[call] A reveal under windows still resets.** If windows cover the dashboard at the reveal, the reset still runs, so the dashboard's first uncover after a greeting scrambles. That uncover is its first showing since the greeting.

## Global Constraints

Plan 2a's and Plan 2b's Global Constraints still apply: pluggable contract, design units, palette, colour for attention, panels, font, Japanese, motion, budgets, D-Bus in tests, formats, config, files ≤ 800 lines, commits. These add to them.

- **Process rules** (`.superpowers/sdd/2026-10-21-ghost-shell-plan-2b-machine-depth/process-rules.md`) bind every agent unchanged, with smokes on `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-2c-t<N>.sock` and headless KWins named `2c-t<N>…`. In short:
  - never `rm -rf`, `pkill`, `git reset`/`rebase`/`push`/`stash`;
  - stage by explicit path, never `docs/IDEAS.md`, `.gitignore`, `.superpowers/` or the untracked lock design files;
  - no `qs` on the user's display;
  - never touch the running services or `~/.config/ghost-shell/`;
  - Conventional Commits with **no trailer or attribution of any kind**;
  - WHY-only comments, braces always.
- **Never lock, unlock or suspend the user's session**, and never send `loginctl lock-session`, `unlock-session` or a `Lock` call to the session bus. The only exception is Task 1, step by step, after the user says yes.
- **Mockup values are binding** (from the reference script):
  - tube: the line grows from `scaleX .05` to `1` in 120 ms (ease-out); at 130 ms it opens to `scaleY 220` while fading `1 → 0` over 300 ms (`cubic-bezier(.2,.8,.3,1)`); the glow also starts at 130 ms and runs 900 ms (ease-out over the whole run): its clip opens from the centre (`inset 50% → 0`) and its opacity goes `1 → .9` by 35 % of the run, then `.9 → 0`; the tube ends at 1030 ms;
  - dot print: 25 states (0–24 of 24 rows), each held 36 ms;
  - roll band: shown 350 ms before typing starts, until the overlay goes; 140 design px tall; travels from `top −140` to `top = designHeight` every 3400 ms, linear, repeating; gradient stops `transparent 0 %`, `hi @ 4.5 % at 60 %`, `hi @ 7 % at 85 %`, `transparent 100 %`;
  - typing: line 1 at 62 ms per character, the subtitle at 26 ms; after each character the wait is `base × (0.6 + 0.8 r)` plus `0.6 × base` after a space, `r` uniform in [0, 1); 420 ms after line 1 ends, its cursor moves to the subtitle;
  - burn-in: each character starts in `burn` and settles to its final colour over 550 ms, ease-out;
  - hold: 1500 ms after the subtitle ends;
  - scan pass: 31 states (0–30 of 30 steps), each held 26 ms; a 2 design px line in `hi` at 55 % on the edge (`PrintSwap.scanlinePx`/`scanlineOpacity`); above the edge the overlay is fully transparent;
  - text: line 1 is 56 design px, weight 700, `hi`, with the name in `signal`; the subtitle is 13 px, weight 500, letter spacing 0.25 em, `signal`, 14 px below line 1; both lines are centred on the screen and re-centre as they grow;
  - cursor: `0.5 em × 0.82 em` of its line's font, `signal`, 4 px after the last character, lowered 0.04 em; it blinks with `FrameClock.blinkSteps` at `FrameClock.blinkStepMs` (spec: the boot cursor *is* the alert blink), its phase counted from typing start;
  - texts: `Welcome back, ` + name; subtitle parts joined by `  ·  ` (two spaces each side), e.g. `おかえりなさい  ·  SESSION 0002`.
- **Easings** are CSS cubic Béziers: `ease-out = (0, 0, 0.58, 1)` and the tube open `(.2, .8, .3, 1)`.
- **No glow** (spec §16): the mockup's `box-shadow`/`text-shadow` are dropped.
- **No QML `Animation` types** (Plan 1 R9): everything is a function of the greeting's elapsed time, evaluated on its tick.
- **Quickshell 0.2.1 keeps a destroyed `IpcHandler` registered**, and calling it then crashes `qs` (`shell/kit/GhostKit/Views.qml:126–131`). Every `IpcHandler` this plan adds lives in an object that is never destroyed.
- **Logging (always on, one line per change):** `greeting armed`, `greeting disarmed`, `greeting playing <login|unlock|wake>`, `greeting revealed`, `greeting skipped`, `greeting done`, and once per overlay `greeting first frame at <epoch ms>`. The smokes and the acceptance read these.
- **Environment controls** (read once at start):
  - `GHOST_GREETING` = unset or `auto` (default), `off` (never arm or play), `login` (play the login greeting at start, ignoring the marker);
  - `GHOST_GREETING_AT=<ms>` freezes a login greeting at that time with the jitter centred (`r = 0.5`), and `GHOST_GREETING_SNAPSHOT=<path>` saves the main overlay 1500 ms later.

  Every existing script that starts `qs` sets `GHOST_GREETING=${GHOST_GREETING:-off}`: `scripts/qml-smoke.sh`, `scripts/smoke-holds.sh`, `scripts/measure-cpu.sh`, `scripts/measure-covered.sh`.
- **Reinstall after each reviewed task** (user rule): the controller installs from a clean detached worktree as before. The first install after Task 4 finds no marker, so the greeting plays once on the live desktop. Tell the user before that restart.

## Review Focus

1. **An overlay that never goes away.** ghostd crashes or restarts while the screen is locked, its `ActiveChanged(false)` is missed, or the shell reconnects mid-lock. The overlay must never stay up while the session is neither locked nor asleep and no greeting plays. *Tests: Task 3 (`disarms_when_neither_locked_nor_sleeping`, `disconnect_disarms_an_armed_overlay`), Task 2 (`lost_screensaver_owner_unlocks_without_a_greeting`).*
2. **Lock or sleep in the middle of a greeting.** Meta+L during the typing, or a suspend from the power button as the scan pass runs: the greeting stops at once, the overlay goes back to its armed rest, and the next unlock plays a fresh greeting from the start. *Tests: Task 3 (`lock_during_play_rearms`), Task 5 smoke.*
3. **Sleep and lock in an unusual order.** A resume with the lock already up; a suspend that fails (`PrepareForSleep(true)` then `(false)` with no lock); a lock that lands after the resume. Each return plays at most one greeting per unlock, never one per signal. *Tests: Task 2 (`wake_then_unlock_is_one_greeting`, `failed_suspend_without_lock_greets_once`, `lock_after_resume_waits_for_the_unlock`).*
4. **No ghostd at login, or a config that arrives late.** The first frame shows within 500 ms with the default palette and `$USER`; a config that arrives mid-sequence recolours but never re-times or re-types it; a 48-character name fits the screen. *Tests: Task 3 (`text_is_fixed_when_typing_starts`), Task 4 (first-frame measurement, long-name snapshot).*
5. **Skip at any moment.** A key or click during the tube, the dot print, the typing or the scan pass ends cleanly: the overlay is gone, nothing keeps exclusive keyboard focus, and the dashboard is revealed exactly once (one `greeting revealed`, and scramble lines after it). *Tests: Task 3 (`skip_from_any_time_reveals_once`), Task 4/5 smokes through the debug IPC.*

---

### Task 1: Spike — the overlay under the lock screen

Answers lock design §11 spike 3 for both projects: can a layer-shell Overlay surface, mapped while kscreenlocker is up, be the first thing on screen when it unlocks? Everything after this task assumes yes. **If the answer is no, stop and report to the user with the evidence. Do not build Tasks 4–5 on a flash** (lock design §6.5).

**Files:**
- Create: `spikes/greeting-handoff/shell.qml` (a full-screen `WlrLayer.Overlay` window in solid `#0a0f11` with a frame counter, mapped when a file named by an env var appears), `spikes/greeting-handoff/window.qml` (a normal toplevel filled red, standing in for the user's windows), `spikes/greeting-handoff/run.sh`.
- Create: `docs/superpowers/notes/2026-10-22-greeting-handoff-spike.md`.

**Interfaces:**
- Produces: the note, with a yes/no answer and the facts below. Tasks 4 and 5 cite it.

- [ ] **Step 1: Read KWin 6.7.5's lock-screen visibility rule.** Read it read-only from the KDE source at tag `v6.7.5` (`gh api` on the `KDE/kwin` mirror is fine): what `WindowItem`/`Window` visibility does while `waylandServer()->isScreenLocked()`, and whether a layer-shell Overlay surface counts as a lock-screen overlay. Expected: only lock-screen, input-method and lock-screen-overlay windows render while locked. Record the file and line numbers in the note.

- [ ] **Step 2: Run a headless KWin with its locker.** Use a spike-local copy of `scripts/headless-kwin.sh` without `--no-lockscreen`, on its own private bus. Start `window.qml`, then the overlay, then lock through that bus only: `dbus-send --session --dest=org.freedesktop.ScreenSaver /ScreenSaver org.freedesktop.ScreenSaver.Lock` with that bus's address. Record:
  - whether the overlay can be mapped while locked;
  - whether it stays hidden under the lock screen;
  - whether Qt keeps rendering it while KWin sends no frame callbacks.

  Capture the screen through that KWin's `org.kde.KWin.ScreenShot2` with its permission check off for the sandbox. Never use spectacle (Plan 2a R3).

- [ ] **Step 3: Unlock the headless locker.** Find the route without the user's password and without any signal that reaches the user's own session. If the only route is `loginctl unlock-session` on the user's real session, or typing the password, **stop and ask the user**. Offer the alternative: they run `spikes/greeting-handoff/run.sh --live` themselves, press Meta+L, unlock, and report whether any red showed. Record:
  - whether the first frames after unlock show the overlay and no red;
  - the delay from `ActiveChanged(false)` to the overlay's next rendered frame;
  - whether `WlrKeyboardFocus.Exclusive` set after unlock takes the keyboard;
  - whether the lock screen fades out (which would decide what the overlay must look like underneath).

- [ ] **Step 4: Write the note** with a top-line answer (yes/no), the source citations, the measured delay, the unlock route used, and what project 2 needs from this side: the overlay is up and resting on ground + dots before the lock screen exits.

- [ ] **Step 5: Commit**

```bash
git add spikes/greeting-handoff docs/superpowers/notes/2026-10-22-greeting-handoff-spike.md
git commit -m "docs(spike): show the greeting overlay can wait under the lock screen"
```
(Reword the subject to the finding if it differs.)

---

### Task 2: The `session` topic in ghostd

**Files:**
- Modify: `crates/ghost-proto/src/lib.rs` (`topics::SESSION = "session"`, `BUILTIN` grows to 19), `crates/ghost-proto/src/snapshot.rs`.
- Create: `crates/ghostd/src/feeds/session.rs`.
- Modify: `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/main.rs` (`host_feeds`), `crates/ghostd/src/fake/feeds.rs`.
- Create: `crates/ghostd/tests/session.rs`, `crates/ghostd/tests/common/session.rs` (a fake ScreenSaver and a fake logind, each served by zbus on a `PrivateBus`).
- Test: `crates/ghost-proto/tests/wire.rs`.

**Interfaces:**
- Produces, in `ghost_proto`:
  ```rust
  pub struct SessionSnapshot {
      /// logind's id for the graphical session, "" until known.
      pub id: String,
      pub locked: bool,
      pub sleeping: bool,
      /// The latest greeting asked for, None before the first.
      pub greeting: Option<GreetingKind>,
      /// Counts greetings, so two unlocks in a row are two changes.
      pub greeting_seq: u64,
  }
  #[serde(rename_all = "snake_case")] pub enum GreetingKind { Unlock, Wake }
  ```
  Wire form: `{"id":"2","locked":false,"sleeping":false,"greeting":"unlock","greeting_seq":3}`.
- Produces, in `ghostd::feeds::session`:
  - `SessionState` with `lock(&mut self, active: bool) -> bool`, `sleep(&mut self, going: bool) -> bool`, `set_id(&mut self, id: String) -> bool` and `owner_lost(&mut self) -> bool`, each returning whether the snapshot changed; `snapshot(&self) -> SessionSnapshot`;
  - `feed() -> Feed`;
  - `feed_on(session: Connection, system: Option<Connection>) -> Feed`.
- Consumes: `Feed`, `Publisher`, `next`, `drain` from `feeds/mod.rs`; the R30 owner-change pattern of `feeds/batteries.rs`.

- [ ] **Step 1: Write the failing state-machine tests** (unit tests in `feeds/session.rs`):

```rust
#[test] fn unlock_greets_once() // lock(true); lock(false) → greeting Some(Unlock), seq 1, locked false
#[test] fn wake_then_unlock_is_one_greeting() // lock(true); sleep(true); sleep(false) → seq 0, sleeping false; lock(false) → Wake, seq 1
#[test] fn failed_suspend_without_lock_greets_once() // sleep(true); sleep(false) → Wake, seq 1; no further change on a repeated sleep(false)
#[test] fn lock_after_resume_waits_for_the_unlock() // sleep(true); sleep(false) → Wake seq 1; lock(true); lock(false) → Unlock seq 2
#[test] fn repeated_reports_are_no_change() // lock(true) twice → second returns false; lock(false) while unlocked → false, seq unchanged
#[test] fn lost_screensaver_owner_unlocks_without_a_greeting() // lock(true); owner_lost() → locked false, seq 0
#[test] fn session_id_is_published_once_known() // set_id("2") → true, snapshot().id == "2"; set_id("2") → false
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ghostd --lib feeds::session`
Expected: FAIL (`SessionState` not defined).

- [ ] **Step 3: Implement `SessionState`** with one private `woke_while_locked: bool`, set by `sleep(false)` while locked and spent by the next `lock(false)`. Add the proto types and the topic, and extend `crates/ghost-proto/tests/wire.rs` with a round trip of the wire form above.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ghostd --lib feeds::session && cargo test -p ghost-proto`
Expected: PASS.

- [ ] **Step 5: Write the failing feed tests** in `crates/ghostd/tests/session.rs`. Use two `PrivateBus`es, one standing in for the system bus. The fakes are `FakeScreenSaver` (`GetActive`, signal `ActiveChanged`, at `/ScreenSaver` under `org.freedesktop.ScreenSaver`) and `FakeLogin1` (`Manager.GetUser(u) → o`, `Manager` signal `PrepareForSleep(b)`, `User.Display (so)`).

```rust
#[tokio::test] async fn publishes_locked_state_read_at_start() // fake GetActive true → first snapshot locked true, seq 0
#[tokio::test] async fn active_changed_drives_unlock_greeting() // emit true then false → snapshot greeting "unlock", seq 1
#[tokio::test] async fn prepare_for_sleep_marks_sleeping_and_wake() // emit PrepareForSleep(true) → sleeping; (false) → greeting "wake"
#[tokio::test] async fn reads_session_id_from_user_display() // Display ("2", path) → id "2"
#[tokio::test] async fn screensaver_appearing_later_is_read_in_full() // name has no owner at start; owner appears with GetActive true → locked true
#[tokio::test] async fn signals_from_a_non_owner_are_ignored() // a second connection emits ActiveChanged(false) → no greeting
#[tokio::test] async fn no_system_bus_still_tracks_the_lock() // feed_on(session, None) → lock/unlock work, id ""
```

- [ ] **Step 6: Run them to see them fail**

Run: `cargo test -p ghostd --test session`
Expected: FAIL (`feeds::session::feed_on` not defined).

- [ ] **Step 7: Implement the feed.** Subscribe to the signals before reading the state, so no change between the read and the subscription is lost. Use `NO_AUTO_START` on every call (the 2b `14fc2f4` rule). Publish on every change, under the state's lock as `DesktopService::update` does. Then:
  - in `main.rs`, `host_feeds()` adds `feeds::session::feed()`;
  - in `fake/feeds.rs`, a still `session` publishes `{ id: "0412", locked: false, sleeping: false, greeting: None, greeting_seq: 0 }` (the mockup's session number), so a fake run touches neither real bus.

- [ ] **Step 8: Run everything**

Run: `cargo test --workspace && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 9: Live read-only check.** Run a dev daemon on its own socket and `--config <temp>`, with the real buses and `scripts/topic-tap.py session`. Expected: one snapshot with `id` `"2"` and `locked: false`. Do not lock anything.

- [ ] **Step 10: Commit**

```bash
git add crates/ghost-proto crates/ghostd/src/feeds/session.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/main.rs crates/ghostd/src/fake/feeds.rs crates/ghostd/tests/session.rs crates/ghostd/tests/common/session.rs crates/ghostd/tests/common/mod.rs
git commit -m "feat(ghostd): publish the session's lock, sleep and greeting events"
```

---

### Task 3: The greeting's rules in `lib/greeting.mjs`

**Files:**
- Create: `shell/kit/GhostKit/lib/greeting.mjs`, `shell/tests/greeting.test.mjs`.
- Modify: `shell/kit/GhostKit/lib/subscriptions.mjs` (`ALWAYS_SUBSCRIBED` gains `'session'`), `shell/tests/subscriptions.test.mjs`.
- Modify: `shell/kit/GhostKit/lib/attention.mjs` (`rearmAttention`), `shell/tests/attention.test.mjs`.

**Interfaces:**
- Produces, from `greeting.mjs`:
  - `cubicBezier(x1, y1, x2, y2) -> (t: number) => number`; `EASE_OUT`, `TUBE_OPEN_EASING`.
  - `sequence({ fromDots: bool }) -> Plan`: the timed head (tube, dot print unless `fromDots`, roll lead), with `plan.typingStartMs`.
  - `typeText(plan, { title, nameStart, subtitle }, random) -> Plan`: adds each character's appear time for both lines, the line pause, the hold and the scan pass. `nameStart` is the index where the name starts in `title`. `random` returns a number in [0, 1).
  - `frameAt(plan, ms) -> Frame`, where `Frame` is:
    ```js
    { tube: null | { lineScaleX, lineScaleY, lineOpacity, glowInset, glowOpacity }, // glowInset: fraction of height cut from top and bottom, 0..0.5
      dotStep,            // 0..24, 24 = every row printed
      rollTop,            // design px, or null before the roll starts
      lines: [{ shown, burn: number[] }, { shown, burn: number[] }], // burn[i] 0 = burn colour .. 1 = settled
      cursorLine,         // 0, 1 or null
      cursorPhaseMs,      // ms since typing start, for the blink
      scanStep,           // null before the scan pass, then 0..30
      done }
    ```
    Before `typeText`, a frame at or past `typingStartMs` shows no characters and no scan.
  - `restingFrame() -> Frame`: the armed overlay under the lock, every dot printed and nothing else.
  - `subtitleText({ japanese, sessionId }) -> string`.
  - Phase machine, with `state = { phase: 'idle'|'armed'|'playing', kind: null|'login'|'unlock'|'wake', fromDots: bool, seen: null|{ connection, seq } }`:
    - `initialGreeting({ mode: 'auto'|'off'|'login', greeted: bool }) -> state`;
    - `withSession(state, session, connection) -> state`;
    - `disconnected(state) -> state`;
    - `finished(state) -> state`;
    - `skipped(state) -> state`.
- Produces, from `attention.mjs`: `rearmAttention(seen)`, which drops every record with `showing === 0` and keeps the rest.

- [ ] **Step 1: Write the failing tests** (`shell/tests/greeting.test.mjs`; values from the Global Constraints):

```js
test('ease-out matches CSS at its midpoint', () => { assert.ok(Math.abs(EASE_OUT(0.5) - 0.6846) < 0.001); assert.equal(EASE_OUT(0), 0); assert.equal(EASE_OUT(1), 1); });
test('tube grows, opens and ends at 1030 ms', () => { const p = sequence({ fromDots: false });
  assert.ok(Math.abs(frameAt(p, 0).tube.lineScaleX - 0.05) < 1e-9); assert.equal(frameAt(p, 120).tube.lineScaleX, 1);
  assert.equal(frameAt(p, 430).tube.lineOpacity, 0); assert.equal(frameAt(p, 130 + 315).tube.glowInset, 0);
  assert.equal(frameAt(p, 1029).tube !== null, true); assert.equal(frameAt(p, 1030).tube, null); });
test('dot print holds each of 25 states for 36 ms', () => { const p = sequence({ fromDots: false });
  assert.equal(frameAt(p, 1030).dotStep, 0); assert.equal(frameAt(p, 1030 + 36 * 12).dotStep, 12); assert.equal(frameAt(p, 1030 + 36 * 24).dotStep, 24); assert.equal(p.typingStartMs, 1030 + 900 + 350); });
test('from dots skips the print', () => { const p = sequence({ fromDots: true }); assert.equal(frameAt(p, 0).dotStep, 24); assert.equal(p.typingStartMs, 1030 + 350); });
test('roll band starts 350 ms before typing and wraps every 3400 ms', () => { const p = sequence({ fromDots: true });
  assert.equal(frameAt(p, 1029).rollTop, null); assert.equal(frameAt(p, 1030).rollTop, -140); assert.equal(frameAt(p, 1030 + 3400).rollTop, -140); });
test('typing waits 62 ms per character, more after a space', () => { const p = typeText(sequence({ fromDots: true }), { title: 'a b', nameStart: 2, subtitle: '' }, () => 0.5);
  const t = p.typingStartMs; assert.equal(frameAt(p, t).lines[0].shown, 1); assert.equal(frameAt(p, t + 62).lines[0].shown, 2); assert.equal(frameAt(p, t + 62 + 62 + 37.2 - 0.01).lines[0].shown, 2); });
test('jitter stays within 0.6 to 1.4 of the base', () => { /* random () => 0 and () => 0.999…; per-char waits within [37.2, 86.8) for non-spaces */ });
test('burn settles over 550 ms with ease-out', () => { /* burn[0] at typing start 0, at +275 EASE_OUT(0.5), at +550 1 */ });
test('cursor moves to the subtitle 420 ms after line 1 ends', () => { /* cursorLine 0 until lineEnd + 420, then 1 */ });
test('empty subtitle goes from the line pause to the hold', () => { /* subtitle '' → scan starts at line1End + 420 + 1500 */ });
test('scan pass holds 31 states for 26 ms and then is done', () => { /* scanStep 0 at scanStart, 30 at scanStart + 780, done at scanStart + 806 */ });
test('text_is_fixed_when_typing_starts', () => { /* typeText applied once: frames after it never change length when called with the same plan */ });
test('resting frame is ground and every dot', () => { const f = restingFrame(); assert.equal(f.dotStep, 24); assert.equal(f.tube, null); assert.equal(f.rollTop, null); assert.equal(f.scanStep, null); assert.equal(f.done, false); });
test('subtitle parts and padding', () => {
  assert.equal(subtitleText({ japanese: 'mixed', sessionId: '2' }), 'おかえりなさい  ·  SESSION 0002');
  assert.equal(subtitleText({ japanese: 'off', sessionId: '2' }), 'SESSION 0002');
  assert.equal(subtitleText({ japanese: 'kanji', sessionId: '' }), 'おかえりなさい');
  assert.equal(subtitleText({ japanese: 'off', sessionId: '' }), '');
  assert.equal(subtitleText({ japanese: 'off', sessionId: 'c12345' }), 'SESSION c12345'); });
test('login plays at start only without the marker', () => {
  assert.equal(initialGreeting({ mode: 'auto', greeted: false }).phase, 'playing');
  assert.equal(initialGreeting({ mode: 'auto', greeted: true }).phase, 'idle');
  assert.equal(initialGreeting({ mode: 'login', greeted: true }).kind, 'login');
  assert.equal(initialGreeting({ mode: 'off', greeted: false }).phase, 'idle'); });
test('arms on lock or sleep', () => { /* idle + {locked:true} → armed, fromDots true; idle + {sleeping:true} → armed */ });
test('first value on a connection is a baseline', () => { /* seen null, greeting_seq 5, unlocked → idle, no play */ });
test('a new greeting_seq plays from dots', () => { /* armed, seq 1 → 2 same connection → playing, kind 'unlock', fromDots true */ });
test('a repeated value does not restart a play', () => { /* playing, then the same session value again (same seq, unlocked) → still playing, same kind */ });
test('disarms_when_neither_locked_nor_sleeping', () => { /* armed + unlocked, same seq → idle */ });
test('disconnect_disarms_an_armed_overlay', () => { /* armed → disconnected → idle; playing → disconnected → playing */ });
test('lock_during_play_rearms', () => { /* playing + {locked:true} → armed */ });
test('skip_from_any_time_reveals_once', () => { /* playing → skipped → idle; idle → skipped → idle (unchanged object) */ });
test('off never arms', () => { /* initialGreeting off + {locked:true} → idle */ });
```

Fill in the commented bodies with the stated assertions. A test whose body is only a comment is rejected in review (Plan 2b P11).

Also, in `shell/tests/attention.test.mjs`: `test('rearm forgets spent alerts that no box shows', …)`. A record `{ spentMs: Infinity, showing: 0 }` is gone and one with `showing: 1` is kept. And in `shell/tests/subscriptions.test.mjs`: `session` is subscribed while covered.

- [ ] **Step 2: Run them to see them fail**

Run: `node --test shell/tests/greeting.test.mjs shell/tests/attention.test.mjs shell/tests/subscriptions.test.mjs`
Expected: FAIL (module `greeting.mjs` not found).

- [ ] **Step 3: Implement.** Name every mockup value as a constant at the top of `greeting.mjs`. `cubicBezier` solves `x(t)` by bisection (no Newton step needed at this precision). `frameAt` is a pure function of `(plan, ms)`: no state between calls, so a skip, a freeze (`GHOST_GREETING_AT`) and a late tick all read the same frame. Write `withSession` in the shape of `views.mjs`'s `acceptAction`: the first value on a connection only sets the baseline.

- [ ] **Step 4: Run them to see them pass**

Run: `node --test 'shell/tests/*.test.mjs'`
Expected: PASS, every earlier test included.

- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib/greeting.mjs shell/tests/greeting.test.mjs shell/kit/GhostKit/lib/subscriptions.mjs shell/tests/subscriptions.test.mjs shell/kit/GhostKit/lib/attention.mjs shell/tests/attention.test.mjs
git commit -m "feat(kit): time the greeting and follow the session's lock and sleep"
```

---

### Task 4: The overlay, the login greeting and the reveal

**Files:**
- Modify: `crates/ghost-proto/src/config.rs` (`Colors.burn`, default `"#ffffff"`), `crates/ghostd/src/config/load.rs` (validate it with the other colours), `crates/ghostd/src/config/default.toml` (`burn = "#ffffff"  # Greeting: each letter's first colour, and the screen's power-on line.`), `crates/ghostd/tests/config_load.rs`.
- Modify: `shell/kit/GhostKit/Theme.qml` (`burn` in `defaultAppearance.colors` and a `burn` colour property).
- Modify: `shell/kit/GhostKit/AlertAttention.qml` (`rearm()` → `Attention.rearmAttention(seen)`).
- Create: `shell/host/Greeting.qml`, `shell/host/GreetingScreen.qml`.
- Modify: `shell/shell.qml`, `shell/host/Desktop.qml`.
- Create: `scripts/smoke-greeting.sh`. Modify: `Makefile` (`smoke-greeting`), and `scripts/qml-smoke.sh`, `scripts/smoke-holds.sh`, `scripts/measure-cpu.sh`, `scripts/measure-covered.sh` (`GHOST_GREETING=${GHOST_GREETING:-off}`).

**Interfaces:**
- Consumes: Task 3's `sequence`, `typeText`, `frameAt`, `subtitleText`, `initialGreeting`, `finished`, `skipped`; `ViewsLib.revealRows`; `FirstWrite.reset()`; `FrameClock.blinkSteps`/`blinkStepMs`; `Scale.designHeight`; `DotGrid`, `Scanlines`.
- Produces:
  - `Greeting.qml`, a `Scope` with:
    - `readonly property bool hidesDashboard`: true while armed, and while playing until the scan pass starts;
    - `function skip()`;
    - `property var state`, internal, owned here.
  - `Desktop.qml` gains `property bool greetingHides`. It hides the dashboard exactly as `covered` does: the view `Loader`, `FrameClock.hostControl.suspended` and `Data.hostControl.covered` follow `covered || greetingHides`. The covered log line still reports windows only.
  - `GreetingScreen.qml`, a `PanelWindow`:
    - `required property ShellScreen modelData`, `property bool isMain`, `property var frame`, `property bool playing`;
    - on `WlrLayer.Overlay`, namespace `ghost-greeting`, anchored to every edge, `ExclusionMode.Ignore`, `color: "transparent"`;
    - `WlrKeyboardFocus.Exclusive` while `playing`, `None` otherwise.

- [ ] **Step 1: The `burn` colour, test first.** Extend `default_file_equals_builtin_defaults` (it must pass unchanged once both sides gain `burn`). Add `invalid_burn_colour_is_rejected`, which mirrors the existing bad-colour test. Run `cargo test -p ghostd --test config_load` and see FAIL; add the field, default, validation, `default.toml` line and Theme mirror; see PASS.

- [ ] **Step 2: `GreetingScreen.qml` draws one frame from `frame`.** Every number below is a design px through `Theme.d()`. On every screen:
  - a ground `Rectangle` and a `DotGrid`, both clipped to the rows below the scan edge;
  - the dots clipped above to `revealRows(frame.dotStep, 24, heightPx)`;
  - the scan line on the edge while `frame.scanStep !== null`.

  On the main screen only:
  - the tube: a line in `Theme.burn`, 2 px tall, centred, scaled by `frame.tube`; a glow `Rectangle` in `signal` at 10 %, clipped by `glowInset`;
  - the roll band, a `Rectangle` with the gradient, at `frame.rollTop`;
  - the two text lines as `Row`s of one `Text` per character. Each character's colour mixes `Theme.burn` into its final colour by `burn[i]`; characters past `shown` don't exist. Line 1's characters from `nameStart` are `signal`;
  - the cursor;
  - `Scanlines` on top.

  Above the scan edge nothing at all is drawn, so the windows or the dashboard show through. Fit line 1 per call 11.

- [ ] **Step 3: `Greeting.qml` runs the sequence.**
  - At start it reads the marker (`FileView` with `blockLoading`). It computes `initialGreeting({ mode, greeted })` and, when that plays, writes the marker at once (`$XDG_RUNTIME_DIR/ghost-shell-greeted`, any content).
  - While playing, a 16 ms repeating `Timer` sets `elapsedMs = Date.now() - startedMs` and `frame = frameAt(plan, elapsedMs)`. The timer stops when `done`, so nothing ticks outside a greeting.
  - At `plan.typingStartMs` it calls `typeText` once with:
    - `title`: `"Welcome back, " + (Data.config ? Data.config.greeting.name : Quickshell.env("USER"))`, with `nameStart` the length of `"Welcome back, "`;
    - `subtitle`: `subtitleText({ japanese: Theme.japanese, sessionId: Data.topic("session").value?.id ?? "" })`;
    - `random`: `Math.random`, or `() => 0.5` when frozen.
  - **The reveal:** on the first frame with `scanStep !== null`, run `FirstWrite.reset()` and then `AlertAttention.rearm()`, log `greeting revealed`, and only then set `hidesDashboard` false. In that order, the dashboard's widgets are created after both resets (C11, C19).
  - `skip()` reveals if it has not yet (the same three steps), logs `greeting skipped`, and goes to `idle`. At `done` it logs `greeting done` and goes to `idle`.
  - The screens are a `Loader` with `active: state.phase !== "idle"` around `Variants { model: Quickshell.screens; GreetingScreen {} }`. A `MouseArea` filling each screen and a `Keys.onPressed` on the main screen's content call `skip()` while playing; the key is accepted, so it never reaches a window.
  - Freeze and snapshot per the Global Constraints. The snapshot grabs the main screen's content item, as `Desktop.qml`'s snapshot does, because the window's own `contentItem` refuses `grabToImage()`.
  - The first frame: each `GreetingScreen` logs `greeting first frame at <Date.now()>` on its window's first `frameSwapped`.

- [ ] **Step 4: Wire it.** In `shell.qml`, add `Greeting { id: greeting }` and `Desktop { greetingHides: greeting.hidesDashboard }`. The greeting goes first, so its overlay is created before the dashboard.

- [ ] **Step 5: Write `scripts/smoke-greeting.sh`** (target `make smoke-greeting`), modelled on `scripts/qml-smoke.sh`: headless KWin, `ghostd --fake` on its own socket, `GHOST_GREETING=login GHOST_DEBUG=1`. It fails unless, in the shell log and in order:
  1. `greeting playing login`;
  2. `greeting first frame at <t>`, with `t − launch_ms ≤ 500`, where the script takes `launch_ms` (`date +%s%3N`) just before starting `qs`;
  3. `greeting revealed`;
  4. at least one `scramble ` line after `greeting revealed` and none between `playing` and `revealed`;
  5. `greeting done`, within 12 s of launch.

  It also fails on `shell-log-errors.sh` matches, as every smoke does. A second run with `GHOST_GREETING=auto` and the marker already in the sandbox's runtime dir expects no `greeting playing` line within 5 s.

- [ ] **Step 6: Run the smokes**

Run: `make shaders && make smoke-greeting && make smoke && make smoke-layout && make smoke-holds`
Expected: all `ok`. The older smokes are unchanged because their scripts now default to `GHOST_GREETING=off`.

- [ ] **Step 7: Snapshots for review.** Take them at `QT_SCALE_FACTOR=1.15`, headless, with `GHOST_GREETING=login GHOST_GREETING_AT=<ms> GHOST_GREETING_SNAPSHOT=/tmp/gs-2c-t4/<ms>.png`, at 60 (tube line), 400 (tube open), 1500 (half the dots), 3000 (typing line 1), 6500 (subtitle and cursor), and halfway through the scan pass. Repeat the 6500 one with `greeting.name` set to 48 characters in the temp config, and with `japanese = "off"`. Expected:
  - no text overflows;
  - the name is `signal`;
  - nothing glows;
  - above the scan edge the snapshot is transparent.

  Put the paths in the task report; the controller shows them to the user.

- [ ] **Step 8: Measure.** Run `scripts/measure-cpu.sh` with the default views and `GHOST_GREETING=off`, before and after this task, and record smaps_rollup Rss for the shell 10 s after a login greeting ends against a run with `GHOST_GREETING=off`. Expected:
  - no change in idle CPU;
  - Rss within the run-to-run spread, since the overlay is destroyed at `idle`.

  Report the numbers; the controller writes README.

- [ ] **Step 9: Commit** (two commits: the config token first, then the overlay)

```bash
git add crates/ghost-proto/src/config.rs crates/ghostd/src/config crates/ghostd/tests/config_load.rs shell/kit/GhostKit/Theme.qml
git commit -m "feat(config): add the burn colour for the greeting's first light"
git add shell/host/Greeting.qml shell/host/GreetingScreen.qml shell/shell.qml shell/host/Desktop.qml shell/kit/GhostKit/AlertAttention.qml scripts/smoke-greeting.sh scripts/qml-smoke.sh scripts/smoke-holds.sh scripts/measure-cpu.sh scripts/measure-covered.sh Makefile
git commit -m "feat(shell): greet on login and reveal the dashboard with one scan pass"
```

---

### Task 5: Greet on unlock and on wake

**Files:**
- Modify: `shell/host/Greeting.qml`, `scripts/smoke-greeting.sh`.

**Interfaces:**
- Consumes: Task 2's `session` topic; Task 3's `withSession` and `disconnected`; the connection-counting pattern of `Views.qml:66–69, 160–170`.
- Produces: a debug `IpcHandler` (`target: "greeting"`, served only under `GHOST_DEBUG=1`, in `Greeting.qml`'s never-destroyed scope) with:
  - `session(json: string): void`, which feeds a session snapshot through the same `withSession` path as the topic, on the current connection;
  - `skip(): void`.

- [ ] **Step 1: Follow the topic.**
  - Each `Data.topic("session")` value goes through `withSession`. `Data.connected` going false goes through `disconnected`; going true counts a new connection.
  - On `armed`, log `greeting armed`. The overlay's `Loader` activates, and the screens show `restingFrame()`: ground and every dot. Nothing ticks.
  - On `playing`, start the sequence `fromDots` and log `greeting playing <kind>`.
  - On leaving `armed` for `idle`, log `greeting disarmed`.

  Spike note (Task 1) facts that change any of this are applied here and cited in the commit body.

- [ ] **Step 2: Extend `scripts/smoke-greeting.sh`** with a third run (`GHOST_GREETING=auto`, marker present). Through `qs ipc call greeting session '<json>'` it sends, in order:
  1. the baseline `{seq 0, unlocked}`;
  2. `{locked: true}`: expect `greeting armed`, and no `scramble` lines while armed;
  3. `{locked: false, greeting: "unlock", greeting_seq: 1}`: expect `greeting playing unlock`, `greeting revealed`, scrambles, `greeting done`;
  4. `{locked: true}`, then `{locked: false}` with the seq unchanged: expect `greeting armed`, then `greeting disarmed`, no `playing`;
  5. `{locked: true}`, then `{locked: false, greeting: "unlock", greeting_seq: 2}`, then 1 s later `{locked: true, greeting_seq: 2}`: expect `greeting playing unlock`, then `greeting armed`, with no `greeting done` between them (Review Focus 2);
  6. `{locked: false, greeting: "unlock", greeting_seq: 3}`, then `skip` 2 s later: expect `greeting skipped`, exactly one `greeting revealed` for that play, and no `greeting done`.

- [ ] **Step 3: Run**

Run: `make smoke-greeting && make smoke && make smoke-holds`
Expected: all `ok`.

- [ ] **Step 4: Commit**

```bash
git add shell/host/Greeting.qml scripts/smoke-greeting.sh
git commit -m "feat(shell): greet after an unlock or a wake, from under the lock screen"
```

---

### Task 6: Install, acceptance with the user, and the record

The controller does this task. It changes the live desktop, so it asks before each step that does.

**Files:**
- Modify: `README.md` (Performance: the first-frame time, CPU and Rss before/after, the locked-session cost; Config: `burn`).
- Modify: `docs/superpowers/specs/2026-10-20-ghost-shell-design.md` §7: record the decisions and calls that changed the text (the resting frame under the lock, the omitted `N NEW SIGNALS`, once per login, the subtitle with Japanese off).
- Create: `docs/superpowers/notes/2026-10-22-plan2c-acceptance.md`, including the **hand-off note for project 2**: what the lock screen can rely on (an overlay resting on ground + dots under it from the moment of locking; no keyboard taken while locked) and what it must do (its exit pass ends on ground + dots).

- [ ] **Step 1: Install** per the reinstall procedure (clean detached worktree, `make shaders`, `CARGO_TARGET_DIR=/tmp/gs-install-target`, `scripts/install.sh`, restart `ghostd`, then `ghost-shell`). Tell the user first that the greeting will play once as the shell restarts. Check the journal for `greeting playing login` and `greeting done` and no errors.

- [ ] **Step 2: Acceptance with the user.** Each is a user step; the controller reads the journal (`journalctl --user -u ghost-shell -u ghostd`) after each:
  1. Meta+L, then unlock with windows open: the greeting plays over the windows and the scan pass reveals them as they were, with no frame of windows before it;
  2. unlock with no windows: the pass reveals the dashboard, whose texts scramble;
  3. suspend, resume and unlock: one greeting, `greeting playing wake`;
  4. a key during a greeting: it ends at once and the key does not reach the window;
  5. log out and in: the greeting plays over restored windows, and the user reports whether the Plasma splash hides any of it. If it does, ask whether to switch the splash off (`ksplashrc`, the user's config) before changing anything.

- [ ] **Step 3: Measure** the overlay's cost while locked: shell + daemon CPU over 60 s with the session locked, via `scripts/measure-cpu.sh` run by the user or with their go-ahead. Expected: at most the covered budget (≤ 0.5 %), since the dashboard is unloaded and nothing ticks.

- [ ] **Step 4: Record and commit**

```bash
git add README.md docs/superpowers/specs/2026-10-20-ghost-shell-design.md docs/superpowers/notes/2026-10-22-plan2c-acceptance.md
git commit -m "docs: record plan 2c acceptance and the greeting's hand-off to the lock screen"
```

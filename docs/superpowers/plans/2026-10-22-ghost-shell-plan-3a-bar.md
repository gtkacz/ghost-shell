# ghost-shell Plan 3 — Part 3a: The Bar and the Launcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retire the Plasma panel and Kickoff. The ghost-shell bar, in layouts A, B (with its dock) and C, at the top or bottom, with mono or colour icons, and the drop-panel launcher do everything the user did with them, while `plasmashell` keeps running out of sight for the clipboard, the OSDs and KDE's applet popups.

**Architecture:**
- **Daemon (`ghostd`):**
  - **Config.** The config gains bar layouts: a preset per `bar.layout`, which a user's `layout.bar` / `layout.dock` replaces, resolved against widgets whose manifests allow `kinds = ["bar"]`. It also gains `launcher.style` and `shortcuts.tasks`.
  - **Calls.** A new client message, `call`, lets the shell ask the daemon to act. The server checks the method name and fans the call out to the feeds that own the matching connection.
  - **Windows.** The `ghost-shell-desktop` KWin script also reports the window list (most recently used first), which ghostd publishes as the `windows` topic. The script takes commands (activate, minimise, close) by long-polling ghostd over D-Bus.
  - **New feeds:**
    - `launcher.entries`: app badges and progress, from `com.canonical.Unity.LauncherEntry`;
    - `ticker`: failed units, firewalld and pending updates;
    - `recent`: recent files from `recently-used.xbel`;
    - session, KRunner and Plasma-popup actions;
    - pinning, which edits `config.toml` with its comments kept, plus a one-time import of the Plasma panel's launchers.
  - **The `desktop` topic** carries the Meta-key launcher toggle and the Meta+1…0 presses.
- **Shell:**
  - **Bar.** `shell/host/Bar.qml` puts a layer-shell bar on the main screen (Top layer, namespace `dock`, exclusive zone). It hosts the resolved bar slots through the same `WidgetHost` as the dashboard: left items at their implicit width, centre items sharing the rest, right items at their implicit width. Layout B adds a floating dock surface.
  - **Bar widgets** are ordinary widgets in `widgets/`: `brand`, `tasks` (`icons` or `tabs`), `ticker`, `status`, `tray`, `view-chip`, `bar-clock`, `launcher-tile`.
  - **Launcher.** `shell/host/Launcher.qml` is the drop panel, opened through a public `Launcher` singleton in GhostKit, so any widget can open it.
  - **Rules in libraries.** Every rule lives in a node-tested `lib/*.mjs`: tasks, tabs, ticker line, tray states, search ranking, keyboard moves, power arming and the content bands.
  - **While covered,** the dashboard view is unloaded, as before. The frame clock is no longer suspended and wants are kept, so the bar keeps its clock and topics.
- **Plasma:** `scripts/plasma-panel.sh retire|restore|status` removes the panel, moves the System Tray and Digital Clock onto Plasma's desktop under the bar's strip, frees bare Meta (KGlobalAccel, ledger R7) and Meta+1…0. Everything is backed up first. It runs on the live desktop only at acceptance, with the user's yes.

**Tech Stack:**
- Rust 1.98, zbus 6, tokio, serde, `notify` (already used by the config watcher). **New crates:** `toml_edit` (config edits that keep comments) and `quick-xml` (the XBEL file), both pure Rust.
- Quickshell 0.2.1 on Qt 6.11:
  - `Quickshell` (`DesktopEntries`, `PanelWindow`);
  - `Quickshell.Wayland` (`WlrLayershell`, `WlrKeyboardFocus`);
  - `Quickshell.Services.SystemTray`, `Quickshell.Services.Pipewire`, `Quickshell.Bluetooth`, `Quickshell.Io`.
- Node 24 for the `.mjs` and KWin-script tests.
- KWin 6.7.5, plasma-workspace 6.7.5, kglobalaccel 6.31 for the spikes.

**Spec:** `docs/superpowers/specs/2026-10-22-ghost-shell-plan-3a-bar-design.md` (this part), on top of `docs/superpowers/specs/2026-10-20-ghost-shell-design.md` §4, §5, §8, §9, §13, §14, §15. The visual truth:
- the bar: `docs/superpowers/specs/mockups/2026-10-20-ghost-shell-reference.html`. CSS lines 73–94 (bar, brand, ticker, status, view chip, clock) and 128–175 (tasks, tips, tabs, tray, dock, flip); script lines 403 (`TICK`), 519–538 (tray and bar markup) and 1211–1226 (`renderTasks`);
- the launcher: `docs/superpowers/specs/mockups/2026-10-22-ghost-shell-launcher-directions.html`, direction A. CSS `.panel`, `.search`, `.rows`/`.row`, `.pins`/`.pin`, `.file`, `.power`/`.pw`, `.scan`, `.dA`; script `score`, `search`, `Launcher.print`, `Launcher.power`, `Launcher.key`.

Read with:
- the greeting hand-off spike `docs/superpowers/notes/2026-10-22-greeting-handoff-spike.md` (layer-shell namespaces and KWin effects);
- Plan 2c's process rules `.superpowers/sdd/2026-10-22-ghost-shell-plan-2c-greeting/process-rules.md`.

**Prerequisite:** Plan 2c is accepted on `main`. This plan was written at `9dd4457`, while 2c was still landing. Every task re-reads the files it edits at its own base. In particular, `shell/host/Desktop.qml`, `Data.qml`, `subscriptions.mjs`, `scripts/*.sh` and `crates/ghostd/src/main.rs` changed under 2c.

## Decisions (user, 2026-10-22)

1. **Split:**
   - 3a is the bar and the launcher;
   - 3b is notifications, including the bell;
   - 3c is Alt+Tab and the audio leftovers.

   The launcher is in 3a so the panel and Kickoff retire together.
2. **3a is built after 2c's acceptance**, on `main`.
3. **The daily setup is layout A, top, mono.** It gets the most careful live acceptance.
4. **The launcher is direction A, the drop panel.** `launcher.style` exists with only `drop` built. `panes` and `console` are in `docs/IDEAS.md`.
5. **`plasmashell` keeps running in project 1, out of sight.**
6. **An app with several windows opens its title list on click**, focused or not. A single-window app focuses on click, and minimises when clicked while focused.
7. **The tray ends with an overflow chevron** that opens KDE's full tray popup. No other applet gets its own icon in 3a.
8. **Clicking the clock opens KDE's calendar.** The panel's standalone Media Player applet is dropped.

## Calls

Marked **[call]**; the user may override any of them. These add to the design's own calls.

1. **[call, amended by Task 1 / ledger R6] Namespaces:** the bar and the dock use `dock` (mapped once, so a fade-in at start is harmless); every transient surface (the launcher, hover lists, menus, tooltips and the outside-click catcher) uses `utility`. Spike `docs/superpowers/notes/2026-10-23-bar-bridge-spike.md` showed Quickshell's `PopupWindow` and `dock` surfaces fade in under the user's effects (`kinetic_fadingpopups`) and only `utility` never does; Show Desktop hiding a transient surface only closes something that should close anyway. Nothing uses `PopupWindow`.
2. **[call] A `call` message**, `{"t":"call","method":"<name>","args":{…}}`, is the shell's only way to ask ghostd to act.
   - Method names are a closed list in `ghost_proto::calls`.
   - An unknown method, or a call from a provider connection, gets `{"t":"call_error","method","message"}`.
   - A known method is broadcast to the feeds. The owning feed logs its own failures, since the shell has nothing to do with them beyond what the next topic value shows.
3. **[call] The KWin script takes commands by long-polling:**
   - `org.ghostshell.WindowCommands1.Next() → s` returns the oldest queued command as JSON at once, or `""` after 20 s;
   - the script calls it again on every answer, and after a 1 s pause on a failure;
   - a command older than 2 s when delivered is dropped, so a click made while KWin was away never fires late;
   - the queue keeps at most 32 commands, dropping the oldest.
4. **[call] The `windows` topic** lists the taskbar's windows: `normalWindow || dialog`, not `skipTaskbar`, not deleted, on any desktop or activity (the user doesn't use either). They're listed most recently activated first, with ids from KWin's `internalId`.
5. **[call] Apps and windows are matched in this order:**
   - the window's `desktopFileName` equals an entry id;
   - an entry's `startupClass` equals the resource class, ignoring case;
   - the lower-cased resource class equals an entry id;
   - `DesktopEntries.heuristicLookup(resourceClass)`.

   An unmatched window is its own task, named by its resource class, with the generic `application-x-executable` icon.
6. **[call] The bar's look follows the mockup**, with these choices:
   - the bar's fill is `ground` at 92 %, and the text on the brand block and on a selected tab is `ground` (the mockup's `#071012` is not a palette token);
   - at most 4 window marks under an icon;
   - hover lists open after 300 ms and close 200 ms after the pointer leaves both the icon and the list;
   - tray built-ins that are off or disconnected are drawn in `dim`;
   - a pinned app's short name is its full name, elided. The mockup's first word made VS Code read "Visual".
7. **[call] No text scrambles in the bar.** Scramble stays a panel thing. The launcher's header scrambles on each opening (design §5.1).
8. **[call] Weather in the status item** reads `WX <temp>°` in `text`, plus ` · <EVENT>` in `warn` while an alert is active (`Fmt.activeAlerts`). It is never animated.
9. **[call] The status item asks for its topics with `Data.want()`**, only for the parts its `parts` setting shows. Layout A then never samples the GPU for the bar while covered.
10. **[call] While covered,** `subscriptionFor` keeps every wanted topic, and the host no longer suspends `FrameClock`. Both used to stop on cover, but the dashboard's widgets are destroyed while covered, so their wants and clock holds are gone anyway; only the bar's remain. `smoke-holds` keeps proving the dashboard's release.
11. **[call] The launcher is on the Overlay layer**, so Meta opens it even over a fullscreen window, with exclusive keyboard focus. While it or a menu is open, a full-screen transparent `dock` surface on the layer below catches outside clicks and closes it. That click is consumed.
12. **[call] Volume:** scrolling over the volume icon moves the default sink by 5 % a notch, clamped to 0–100 %, and middle-click toggles mute.
13. **[call] Tabs (layout C):** most recently activated first, as the mockup's `focusWin` orders them. As many tabs as fit at 128 px each, then `+N`. Clicking a tab focuses its window, and clicking the focused tab minimises it.
14. **[call] Recent files come from `$XDG_DATA_HOME/recently-used.xbel`.** KDE Frameworks and GTK both write it, and on this machine it is live: 300 entries, the newest from minutes before this plan. Task 2 compares it with what Kickoff shows. If the two differ in a way the user would notice, stop and ask.
15. **[call] Pins are desktop-file ids without `.desktop`** (`org.kde.dolphin`, `steam`). The first-run import reads the task manager's `launchers=` from `plasma-org.kde.plasma.desktop-appletsrc`:
    - `applications:<id>.desktop` becomes `<id>`;
    - `file:///…/<id>.desktop` becomes `<id>`;
    - `preferred://filemanager` becomes the `inode/directory` default in `mimeapps.list`, and is dropped if there is none;
    - it runs only when the user's file has no `bar.pinned` key at all.
16. **[call] `GHOST_BAR=off`** (read once) keeps the bar and the launcher from loading. Only `scripts/measure-covered.sh` sets it, so its dashboard-only "0 frames" check stays exact. A new run measures the covered state with the bar on.

## Global Constraints

Plan 2a's, 2b's and 2c's Global Constraints still apply:
- pluggable contract, design units, palette, colour for attention, panels, font, Japanese and motion;
- budgets and the 2b margin rule O1 (+0.25 % CPU / +10 MiB Rss accepted and flagged);
- D-Bus in tests, formats, config, files ≤ 800 lines, commits;
- no QML `Animation` types, and `IpcHandler`s only in objects that are never destroyed.

These add to them.

- **Process rules** (`.superpowers/sdd/2026-10-22-ghost-shell-plan-2c-greeting/process-rules.md`) bind every agent unchanged.
  - Smokes use `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-3a-t<N>.sock`, and headless KWins are named `3a-t<N>…`. Every `qs` run sets `GHOST_GREETING=${GHOST_GREETING:-off}`.
  - In short:
    - never `rm -rf`, `pkill`, `git reset`/`rebase`/`push`/`stash`;
    - stage by explicit path, never `docs/IDEAS.md`, `.gitignore` or `.superpowers/`;
    - no `qs` on the user's display;
    - never touch the running services or `~/.config/ghost-shell/`;
    - Conventional Commits with **no trailer or attribution of any kind**;
    - WHY-only comments, braces always.
- **Never touch the user's Plasma.** No `evaluateScript` on their `plasmashell`, no `kwriteconfig6` on their `kwinrc` or `kglobalshortcutsrc`, and no KGlobalAccel registration, `invokeShortcut` or `org.kde.Shutdown`/`ScreenSaver`/`login1` call on their buses.
  - Feeds that act take their connections injected, and every test runs them on a private bus with stub services.
  - The spikes run `plasmashell` and KWin only on private buses with copied config directories.
  - The only exception is Task 19, step by step, after the user says yes.
- **Never lock, log out, suspend, restart or power off** any session, not even through a stub that could fall through to a real bus.
- **Bar values** (design px, mockup CSS 73–94 and 128–175):
  - **Bar:**
    - 30 high, fill `ground` at 92 %, a 1 px `hair` line on the edge facing the screen's middle;
    - items 14 apart, 12 right padding;
    - text 10 px, tracking 0.08 em.
  - **Brand:**
    - 30 high, padding 0 26 0 12, fill `signal`, text `ground`;
    - `GS//01` at 15 px / 700 with the `//` at 50 % opacity, then 8 px, then the subtitle at 10 px / 600 at 75 %;
    - a 14 px chamfer cuts its bottom-right corner (its top-right corner with the bar at the bottom);
    - the subtitle is hidden when `japanese = "off"`, and the whole block when `bar.brand.enabled = false`.
  - **Tasks (icons):**
    - cells 28 × 28, 1 apart, icon 17;
    - hover fill `signal` at 8 %;
    - marks 4 × 2, 2 apart, 2 from the cell's bottom (top for a bottom bar), in `signal`. The focused app's first mark is 12 wide in `hi`;
    - a pinned app that isn't running is at 40 % opacity.
  - **Tip:**
    - 9.5 px / 600, tracking 0.04 em;
    - fill `ground` at 96 %, 1 px `hair` border, padding 2 × 8;
    - 1 px past the bar (46 px from the dock's far edge, for the dock).
  - **Tab (C):**
    - 22 high, padding 0 14 0 6, at most 128 wide, an 8 px chamfer on its top-right corner;
    - index 9 px `dim`, then a 6 px gap, the icon, the name 9.5 px / 600 tracking 0.08 em `text`, UPPERCASE;
    - selected: fill `signal`, text and icon `ground`;
    - `+N` is 9.5 px `dim`, padding 0 8.
  - **Status:**
    - each part is a label (tracking 0.12 em, `dim`) and a value, 5 apart;
    - `CPU 7%`, `GPU 12%`, `MEM 23.4/92G` (used and total GiB, one decimal and whole);
    - traffic is the up chevron, up rate, down chevron and down rate (`Fmt.rateShort`), with 8 px chevrons;
    - `VPN <n> UP`;
    - load parts turn `warn`/`crit` at the load thresholds.
  - **Tray:** icons 13 px, 10 apart, stroke 1.4, in `signal`. The paths come from the mockup's `TRAY` table.
  - **View chip:**
    - `MAIN`/`MONITOR` at 10 px / 600, tracking 0.14 em, `hi`, 1 px `hair` border, padding 2 × 8;
    - then the `swap` glyph at 12 px in `signal`, 4 apart.
  - **Clock:** `HH:MM` 15 px / 700 `hi`, seconds 9 px `signal` raised, 3 apart, then the date `DD.MM.YY` in `dim` 8 px further, on one baseline.
  - **Ticker:**
    - `dim`, segments joined by ` // `, with a trailing ` //`;
    - crit segments in `crit`;
    - two copies 24 px apart;
    - one copy's width scrolls by in 60 s, in held steps on `FrameClock` (≤ 15 fps).
  - **Dock (B):**
    - 10 from the screen edge, centred;
    - padding 6 12 6 6, fill `ground` at 90 %, 12 px chamfers top-right and bottom-left, edge and brackets as a panel;
    - cells 2 apart;
    - launcher tile 40 × 38, `signal` fill, `GS` 12 px / 700 in `ground`, 8 right margin, 9 px chamfer bottom-left;
    - a 1 × 24 `hair` separator, 6 each side, between pinned and unpinned running apps.
  - **Bands:** the dashboard keeps 30 for the bar on the bar's side, plus 58 for the dock on the other side in layout B. That's the mockup's `.bar-b .layer { bottom: 70px }` against 12.
- **Launcher values** (design px, launcher mockup direction A):
  - **Panel:** 392 × 560, at the screen's left edge, flush under (or above) the bar; fill `ground` at 94 %, chamfer 12, edge and brackets.
  - **Header:** a Panel header with the label `LAUNCH`, tag key `launch` and meta `<n> APPS`.
  - **Search field:**
    - margin 2 14 4, 30 high, 1 px `hair` border, fill `signal` at 3 %, 8 padding;
    - the `chevron-right` glyph at 10 px in `signal`, then 8 px;
    - the query at 13 px `hi`, tracking 0.04 em;
    - the block cursor 7 × 14 in `signal`, blinking with `FrameClock.blinkSteps`;
    - the placeholder `SEARCH APPS` at 11 px `dim`, tracking 0.08 em;
    - an `ESC` chip in `dim` on the right.
  - **Section header:** 9 px / 600, tracking 0.18 em, `dim`, the tag at 70 %, then a 1 px `hair2` rule to the right edge; padding 8 14 4.
  - **Pins:**
    - 6 columns in 10 px side padding;
    - a cell is padding 7 2 5, icon 22, name 8.5 px `dim` elided below it, 4 apart;
    - selected: 1 px `hair` border, fill `signal` at 7 %, name `hi`.
  - **App row:**
    - 26 high, padding 0 14 0 12;
    - icon 17, 10 apart;
    - name 11 px `text` (`hi` selected), with the matched part in `hi`;
    - generic name 10 px `dim` 8 px after it;
    - the category chip on the right, `signal` when selected and `dim` otherwise;
    - selected: fill `signal` at 9 % and a 2 px `signal` left edge.
  - **File row:**
    - 24 high, padding 0 14;
    - the `file` glyph 12 in `signal`, 10 apart;
    - name 10.5 px `text`, folder 9.5 px `dim` 8 after it, age 9.5 px `dim` on the right;
    - selected: fill `signal` at 9 %.
  - **Power row:**
    - 1 px `hair2` top rule, padding 8 10 12;
    - 5 equal buttons 2 apart, each padding 6 0 5, glyph 14 in `signal` over its label 8.5 px tracking 0.1 em `dim`;
    - hover or keyboard selection: 1 px `hair` border;
    - armed: border, glyph and label `warn`, label `AGAIN`.
  - **Opening:** 10 held steps 18 ms apart, the panel clipped from its bar edge, with a 2 px line in `hi` at 55 % on the moving edge (`PrintSwap.scanlinePx`/`scanlineOpacity`). Closing is instant.
  - **Launch:** the selected row's name shows in `burn` for 140 ms, then the panel closes.
- **Japanese tags** (add to `TAGS` in `lib/theme.mjs`):

  | Key | Kanji | Katakana |
  |---|---|---|
  | launch | 起動 | ランチャー |
  | pin | 固定 | ピン |
  | recent | 最近 | ヒストリー |
  | all | 全部 | オール |
  | results | 検索 | サーチ |

- **Glyphs** (add to `Glyph.qml`, 12-unit box). Stroked unless marked filled:
  - `bluetooth`: `M4 4l5 4.5L6.5 11V1L9 3.5 4 8`
  - `network`: `M2 11V7h8v4M4 7V4h4v3M6 4V1`
  - `volume`: `M1.5 4.5h2.5L7 2v8L4 7.5H1.5zM9 4a3 3 0 0 1 0 4`
  - `phone`: `M3.5 1h5v10h-5zM5.5 9h1`
  - `clipboard`: `M2.5 2h7v9h-7zM4.5 1h3v2h-3z`
  - `file`: `M2.5 1h5l2 2v8h-7zM7.5 1v2h2`
  - `lock`: `M2.5 5.5h7v5.5h-7zM4 5.5V3.5a2 2 0 0 1 4 0v2`
  - `logout`: `M6.5 1.5h-5v9h5M8 3.5 10.5 6 8 8.5M5 6h5.5`
  - `sleep`: `M9.8 7.6A4.3 4.3 0 1 1 4.4 2.2a3.6 3.6 0 0 0 5.4 5.4z`
  - `restart`: `M10 6a4 4 0 1 1-1.2-2.85M6.6 3.4 9 3.1 9.3 0.8`
  - `power`: `M6 1v5M3.4 3a4 4 0 1 0 5.2 0`
- **Bar topics while covered.** Only these keep sampling: `desktop`, `perf`, `session`, the bar slots' `requires`, and what bar widgets `want()`.
- **Budgets:**
  - the bar counts inside the shell's §15 budgets;
  - covered with layout A: shell + daemon ≤ 0.5 %, frames from the bar only;
  - every task that adds bar items, a feed or a surface reports its CPU and Rss delta (`scripts/measure-cpu.sh`, `smaps_rollup` Rss);
  - a budget check past the O1 margin stops the task, and the numbers go to the user. Nothing is slowed or re-budgeted unilaterally.
- **New topics** (all built in; `BUILTIN` grows by these four from whatever 2c left it at, 19 when this plan was written): `windows`, `launcher.entries`, `ticker`, `recent`.
- **Logging** (one line per change, always on):
  - `bar on <output> <top|bottom> layout <a|b|c>`;
  - `launcher opened`, `launcher closed`, `launched <desktop id>`;
  - `call <method>` on the daemon at debug level, and `call refused <method>: <reason>` at warn.

## Review Focus

1. **A click that lands after the world moved.**
   - The user clicks a task while that window is closing, KWin restarts, or the KWin script is reloaded. Nothing must act on the wrong window, and nothing must fire seconds later.
   - Commands carry window ids, unknown ids are ignored by the script, and stale commands are dropped.
   - *Tests: Task 4 (`stale_command_is_dropped`, `unknown_window_command_is_ignored`, `kwin_restart_keeps_the_poll_alive`).*
2. **The same app twice in the list.**
   - Duplicate `.desktop` entries (two Dolphins, two Steams, a Flatpak and a native Firefox on this machine), or a window whose class matches nothing, or matches a different entry than its pin.
   - One task per app, pins win, and nothing is lost.
   - *Tests: Task 12 (`window_joins_its_pinned_app_through_startup_class`, `unmatched_window_is_its_own_task`), Task 16 (`duplicates_merge_user_over_system_over_flatpak`).*
3. **A power action from a stray input.**
   - A double-click, a key held down, Enter pressed twice in a row on `SHUT DOWN`, or the launcher closed and reopened while armed. Only a second deliberate press within 3 s of the first acts, and closing disarms.
   - *Tests: Task 16 (`power_needs_a_second_press_within_three_seconds`, `closing_disarms`, `key_repeat_does_not_confirm`).*
4. **The config file the user also edits.**
   - Pinning while the file has comments, unusual formatting, a syntax error, or an explicit `pinned = []`. Comments and formatting survive, a broken file is never overwritten, and an explicit empty list is never re-imported.
   - *Tests: Task 10.*
5. **The bar in the other configurations.**
   - Bottom position, layout B with its dock, layout C with 20 windows, brand turned off, `japanese = "off"`, colour icons, a widget missing or broken in a bar slot.
   - Nothing overlaps or clips, menus and the launcher open towards the middle, and a broken bar slot shows a small error mark instead of breaking the bar.
   - *Tests: Task 11 (`broken_bar_slot_shows_an_error_mark` smoke), Task 13 (`tabs_overflow_counts_hidden_windows`), Task 18 snapshots.*

---

### Task 1: Spike A — the window bridge, the Meta key, launch focus and popup fades

Answers design §8 questions 1 and 3, plus two questions this plan adds: does a launched app get focus, and does KWin fade `xdg_popup`s. **If the KWin script can't report windows or act on commands, or Meta alone can't reach a D-Bus method, stop and report to the user with the evidence.**

**Files:**
- Create: `spikes/bar-bridge/` (`run.sh`, a copy of the KWin script extended as below, `stub_ghostd.py` serving the two interfaces on the private bus, `shell.qml` for the launch and popup checks)
- Create: `docs/superpowers/notes/2026-10-23-bar-bridge-spike.md`

**Interfaces:**
- Produces: the note, with a yes/no per question and the measured figures. Tasks 4, 5, 11 and 16 cite it.

- [ ] **Step 1: Window list and commands.** In a headless KWin (`scripts/headless-kwin.sh 3a-t1`) on a private session bus:
  - install a spike copy of the KWin script that reports `normalWindow || dialog` windows on every change as JSON through `DBusCall`;
  - have it long-poll `Next()` on the stub, with `DBusCall`'s `finished(returnValue)` and `failed()` handlers;
  - open three toplevels from two "apps" (`FloatingWindow`s with distinct app ids).

  Record:
  - that the reports arrive in activation order;
  - the latency from the stub queuing `activate`/`minimize`/`close` to KWin's `windowActivated`/`minimizedChanged`/`windowRemoved` (target ≤ 50 ms, median of 20);
  - that `failed()` exists and fires when the stub is gone;
  - the D-Bus traffic per minute while idle (expected: one `Next` per 20 s).

- [ ] **Step 2: Meta on its own.** Copy the user's `kwinrc` and `kglobalshortcutsrc` read-only into the sandbox's `XDG_CONFIG_HOME`. Set `[ModifierOnlyShortcuts] Meta=org.ghostshell.Ghostd,/org/ghostshell/Desktop,org.ghostshell.Desktop1,ToggleLauncher` there. Press Meta alone with `spikes/greeting-handoff/fake_key.py`. Record:
  - that the stub's `ToggleLauncher` is called once per tap;
  - that Meta+S still reaches the KRunner binding's KGlobalAccel action, through the stub's `globalShortcutPressed` on the private bus;
  - that Meta held with another key does not toggle.

- [ ] **Step 3: Launch focus.** From a spike `qs` in that KWin, with the user's `[Windows] FocusStealingPreventionLevel` copied:
  - call `DesktopEntry.execute()` on a test entry whose window opens 1 s later;
  - record whether it gets focus;
  - if not, record whether `Quickshell.execDetached(["gtk-launch", id])` or `kioclient exec` does.

- [ ] **Step 4: Popup fades.** Capture with the 2c spike's `grab.py watch` at ≥ 1 500 captures/s. Show a Quickshell `PopupWindow` and a layer-shell `dock` surface of the same content. Record whether either fades in (frames between empty and full) with KWin's default effects from the user's copied `kwinrc`.

- [ ] **Step 5: Write the note.** Include:
  - the top-line answers;
  - the latency and traffic figures;
  - the exact `DBusCall` signal names;
  - the launch route that gives focus;
  - which surface kind to use for popups.

- [ ] **Step 6: Commit**

```bash
git add spikes/bar-bridge docs/superpowers/notes/2026-10-23-bar-bridge-spike.md
git commit -m "docs(spike): show the KWin script can list and focus windows for the bar"
```
(Reword the subject to the finding if it differs.)

---

### Task 2: Spike B — Plasma without its panel

Answers design §8 questions 2–4: popups, applets alive, Meta+1…0 ownership and recent files. It produces the prototype of `scripts/plasma-panel.sh`. **If no route opens the network, Bluetooth, volume, KDE Connect, clipboard, full tray and calendar popups from outside, and `plasmawindowed` fails too, stop and report.**

**Files:**
- Create: `spikes/plasma-hidden/` (`run.sh`, `plasma-panel.sh` prototype, `popup.sh`)
- Create: `docs/superpowers/notes/2026-10-23-plasma-hidden-spike.md`

**Interfaces:**
- Produces:
  - the note, which names the popup mechanism `plasma.popup` uses (Task 7), and the retire/restore steps and files Task 17 productionises;
  - the comparison of recent files (Task 9).

- [ ] **Step 1: A sandboxed plasmashell.**
  - Start a headless KWin and run `plasmashell --no-respawn` in it.
  - Point `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` and `XDG_STATE_HOME` at copies of the user's directories, made read-only from the originals.
  - Give it a private session bus with KDE's service files available, so kded, kglobalaccel and kactivitymanagerd activate there, and a second private bus as `DBUS_SYSTEM_BUS_ADDRESS` (2c ruling R2).
  - Confirm with a screenshot that the panel shows as on the user's desktop.

- [ ] **Step 2: Retire the panel in the sandbox.** Through `evaluateScript` on the sandbox's `org.kde.plasmashell`:
  - remove the panel;
  - add `org.kde.plasma.systemtray` and `org.kde.plasma.digitalclock` to the desktop containment, near the top-right of the screen;
  - copy the old applets' config (the tray's `extraItems`/`hiddenItems`/`shownItems`, the clock's calendar plugins and week numbers).

  Record:
  - the scripting calls that work;
  - where the widgets land (a screenshot);
  - that `org.freedesktop.Notifications` is still owned on the private bus;
  - that clipboard history survives (copy through `wl-copy` against the sandbox, then Meta+V with `fake_key.py`).

- [ ] **Step 3: Open each popup from outside.** Try in order, and record the first route that works for each of the seven:
  1. assigning each applet a KGlobalAccel shortcut through its `[Shortcuts] global=` config and calling `invokeShortcut` on `/component/plasmashell`;
  2. the applet's existing `activate widget <id>` action;
  3. `plasmawindowed <plugin>`.

  Screenshot where each opens, relative to a 52 logical px strip at the top (the bar's height at scale 1.15).

- [ ] **Step 4: Meta+1…0 and memory.**
  - After the retire, list the holders of Meta+1 with `globalShortcutsByKey` on the sandbox's kglobalaccel, and test removing the `activate task manager entry N` actions.
  - Record `plasmashell`'s `smaps_rollup` Rss before and after the retire.

- [ ] **Step 5: Restore.** Run the prototype's restore (put the backed-up files back, then restart `plasmashell`) and screenshot that the panel returns as it was.

- [ ] **Step 6: Recent files.** Read-only:
  - list the 10 newest `file://` bookmarks of `~/.local/share/recently-used.xbel`;
  - list the 10 newest files from a **copy** of `~/.local/share/kactivitymanagerd/resources/database*` with `sqlite3` (`ResourceScoreCache`, ordered by `lastUpdate`).

  Record how many of the first five agree. Report the paths only as counts and file extensions in the note, never as names. They are the user's files.

- [ ] **Step 7: Write the note** with:
  - the popup route per applet;
  - placement screenshots described in words;
  - the exact retire and restore steps;
  - the files to back up;
  - the Meta+1…0 finding;
  - the Rss delta;
  - the recent-files agreement.

- [ ] **Step 8: Commit**

```bash
git add spikes/plasma-hidden docs/superpowers/notes/2026-10-23-plasma-hidden-spike.md
git commit -m "docs(spike): show Plasma's popups open from a tray hidden under the bar"
```
(Reword to the finding.)

---

### Task 3: Bar layouts, the launcher style and the task shortcuts in config

**Files:**
- Create: `crates/ghost-proto/src/bar.rs` (re-exported from `lib.rs`)
- Modify:
  - `crates/ghost-proto/src/config.rs` (`LayoutConfig`, `Shortcuts`, `Launcher`);
  - `crates/ghost-proto/src/layout.rs` (`ResolvedLayout.bar`);
  - `crates/ghostd/src/widgets/resolve.rs`;
  - `crates/ghostd/src/server/feed.rs` (`resolve_layout` passes `config.bar`);
  - `crates/ghostd/src/config/load.rs` (`validate_shortcuts`, `validate_bar_layout`);
  - `crates/ghostd/src/config/default.toml`.
- Test: `crates/ghostd/tests/config_bar.rs` (new: `config_load.rs` is at 798 of 800 lines), `crates/ghostd/tests/widgets.rs`.

**Interfaces:**
- Consumes: `BarLayout`, `BarConfig`, `resolve()`, `Kind::Bar`.
- Produces:
  - **`bar.rs`:**
    - `BarSlotSpec { widget: String, settings: Map<String, Value> }`, with `deny_unknown_fields` and `settings` defaulting to empty;
    - `BarLayoutSpec { left: Vec<BarSlotSpec>, centre: Vec<BarSlotSpec>, right: Vec<BarSlotSpec> }`, each list defaulting to empty;
    - `pub fn bar_preset(layout: BarLayout) -> BarLayoutSpec` and `pub fn dock_preset() -> Vec<BarSlotSpec>`;
    - `ResolvedBar { left, centre, right, dock: Vec<ResolvedSlot> }`.
  - **Config additions:**
    - `LayoutConfig.bar: Option<BarLayoutSpec>` and `LayoutConfig.dock: Option<Vec<BarSlotSpec>>` (TOML `[layout.bar]` with `left`/`centre`/`right` arrays of inline tables, and `layout.dock` as an array);
    - `ResolvedConfig.launcher: Launcher { style: LauncherStyle }`, where `LauncherStyle` is `drop` only (default);
    - `Shortcuts.tasks: Vec<String>`, default `["Meta+1", …, "Meta+9"]` (9 entries; at most 10 allowed). Meta+0 is KWin's `view_actual_size` (spike `docs/superpowers/notes/2026-10-23-plasma-hidden-spike.md`), and Plasma's task manager never held it here (ledger R8).
  - **`ResolvedLayout.bar: ResolvedBar`.** `resolve(layout: &LayoutConfig, bar: &BarConfig, manifests)` resolves `layout.bar.unwrap_or(bar_preset(bar.layout))`, and the dock (`layout.dock.unwrap_or(dock_preset())`) only when `bar.layout == B`, else empty.
  - **Error paths** read `layout.bar.left[0]` and `layout.dock[1]`. A bar slot whose manifest lacks `Kind::Bar` gets `widget "x" can't be placed in the bar`, and a view slot lacking `Kind::Panel` keeps its message.
  - **Presets** (exact):
    - **A:** left `[brand, tasks{style="icons"}]`, centre `[]`, right `[status{parts=["cpu","net","vpn","weather"]}, tray, view-chip, bar-clock]`.
    - **B:** left `[brand]`, centre `[ticker]`, right `[status{parts=["cpu","gpu","mem","net","vpn","weather"]}, tray, view-chip, bar-clock]`, and dock `[launcher-tile, tasks{style="icons"}]`.
    - **C:** left `[brand, tasks{style="tabs"}]`, centre `[]`, right `[status{parts=["weather"]}, tray, view-chip, bar-clock]`.

- [ ] **Step 1: Write the failing tests.** In `config_bar.rs`, using fixture manifests in a temp widgets dir (`brand`, `tasks`, `ticker`, `status`, `tray`, `view-chip`, `bar-clock`, `launcher-tile` with `kinds=["bar"]`, and `processor` with `kinds=["panel"]`):

```rust
#[test] fn bar_preset_follows_bar_layout()      // layout "c": left == [brand, tasks{style:"tabs"}], dock empty
#[test] fn dock_exists_only_in_layout_b()       // "b": dock == [launcher-tile, tasks{style:"icons"}]; "a": dock empty
#[test] fn user_bar_layout_replaces_the_preset() // [layout.bar] left=[{widget="bar-clock"}] → left == [bar-clock], right == []
#[test] fn panel_widget_in_the_bar_is_an_error() // left=[{widget="processor"}] → slot.error == "widget \"processor\" can't be placed in the bar", errors[0] starts "layout.bar.left[0]: "
#[test] fn bar_widget_in_a_view_is_an_error()   // a view slot "tray" → "widget \"tray\" can't be placed in a panel slot"
#[test] fn task_shortcuts_default_to_meta_digits() // tasks[0] == "Meta+1", tasks[8] == "Meta+9", len 9
#[test] fn task_shortcut_clashing_with_monitor_is_rejected() // tasks=["Meta+M"] → error names "shortcuts.tasks[0]"
#[test] fn more_than_ten_task_shortcuts_are_rejected()
#[test] fn launcher_style_other_than_drop_is_rejected() // style="panes" → error names "launcher.style"
#[test] fn unknown_bar_layout_key_names_its_path() // [layout.bar] middle=[] → error names "layout.bar" and "middle"
```

- [ ] **Step 2: Run them and watch them fail**

Run: `~/.cargo/bin/cargo test -p ghostd --test config_bar`
Expected: FAIL to compile (`BarLayoutSpec`, `Shortcuts::tasks`, `Launcher` do not exist).

- [ ] **Step 3: Implement** the types, presets, resolution and validation named above. `validate_shortcuts` checks `tasks` for parsing, at most 10 entries, and no key shared with `monitor`, `swap` or another task entry. `default.toml` documents:
  - `[launcher] style = "drop"` ("only \"drop\" is built so far");
  - `shortcuts.tasks`;
  - a commented `[layout.bar]` example equal to preset A.

- [ ] **Step 4: Run the tests and the workspace checks**

Run: `~/.cargo/bin/cargo test --workspace && ~/.cargo/bin/cargo fmt --check && ~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. `ghostd default-config` prints the new keys.

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src/bar.rs crates/ghost-proto/src/lib.rs crates/ghost-proto/src/config.rs crates/ghost-proto/src/layout.rs crates/ghostd/src/widgets/resolve.rs crates/ghostd/src/server/feed.rs crates/ghostd/src/config/load.rs crates/ghostd/src/config/default.toml crates/ghostd/tests/config_bar.rs crates/ghostd/tests/widgets.rs
git commit -m "feat(config): resolve bar and dock layouts from presets or the user's layout"
```

---

### Task 4: Calls and the window bridge

**Files:**
- Modify:
  - `crates/ghost-proto/src/msg.rs` (`ClientMsg::Call`, `ServerMsg::CallError`);
  - `crates/ghost-proto/src/lib.rs` (`topics::WINDOWS = "windows"`, and a `pub mod calls` with every method name and `ALL`);
  - `crates/ghost-proto/src/snapshot.rs` (`WindowsSnapshot`, `WindowInfo`).
- Modify the server: `crates/ghostd/src/server/session.rs`, `crates/ghostd/src/server/publish.rs` (`FeedContext.calls`), `crates/ghostd/src/server/mod.rs`.
- Create: `crates/ghostd/src/feeds/windows.rs`.
- Modify: `crates/ghostd/src/feeds/desktop.rs` (serves the windows objects on the same connection), `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/fake/feeds.rs`.
- Modify the KWin script: `kwin/ghost-shell-desktop/contents/code/main.js`, `kwin/ghost-shell-desktop/contents/ui/main.qml`, `kwin/ghost-shell-desktop/metadata.json` (description).
- Modify: `shell/kit/GhostKit/Data.qml` (`call(method, args)`).
- Test: `crates/ghostd/tests/windows.rs` (new), `crates/ghostd/tests/server_handshake.rs`, `shell/tests/kwin-desktop.test.mjs`.

**Interfaces:**
- Consumes: Task 1's note (signal names, channel). If it chose a different channel, follow the note and keep the names below.
- Produces:
  - **The call message:** `ClientMsg::Call { method: String, args: serde_json::Value }`. `ServerMsg::CallError { method: String, message: String }`.
  - **`calls`:**
    - `WINDOWS_ACTIVATE = "windows.activate"`, `WINDOWS_MINIMIZE = "windows.minimize"`, `WINDOWS_CLOSE = "windows.close"` (args `{"id": "<internalId>"}`);
    - `SESSION_LOCK`, `SESSION_LOGOUT`, `SESSION_REBOOT`, `SESSION_SHUTDOWN`, `SESSION_SUSPEND` (`"session.lock"` …, args `{}`);
    - `KRUNNER_QUERY = "krunner.query"` (`{"text"}`), `PLASMA_POPUP = "plasma.popup"` (`{"applet"}`);
    - `BAR_PIN = "bar.pin"`, `BAR_UNPIN = "bar.unpin"` (`{"app"}`);
    - `ALL: [&str; 12]`.
  - **`FeedContext.calls: tokio::sync::broadcast::Receiver<Call>`**, where `Call { method: String, args: Value }` has capacity 64. A lagged receiver logs `warn` once per lag.
  - **The `windows` topic:**
    - `WindowsSnapshot { windows: Vec<WindowInfo> }`, most recently activated first;
    - `WindowInfo { id, app_id, resource_class, caption: String, pid: u32, active, minimized, fullscreen, demands_attention: bool }`.
  - **On the bus** (at `/org/ghostshell/Windows` on the desktop feed's connection):
    - `org.ghostshell.Windows1.Report(s windows_json)` (`spawn = false`);
    - `org.ghostshell.WindowCommands1.Next() → s` (spawned).
  - **Command JSON:** `{"action":"activate"|"minimize"|"close","id":"…"}`.
  - **The KWin script** (`main.js`):
    - `start(host)` also takes `host.sendWindows(json)`, `host.nextCommand()` and `host.onCommand`;
    - it exports `handleCommand(json)`;
    - it reports the window list only when its JSON changes.
  - **`Data.call(method: string, args: object)`** sends the message, or logs `ghostd not connected; dropped call <method>`.

- [ ] **Step 1: Write the failing Rust tests** in `windows.rs`, on a private bus, with `feeds::windows` built with `NEXT_TIMEOUT` and `STALE_AFTER` injected (100 ms and 50 ms):

```rust
#[tokio::test] async fn report_publishes_windows_in_the_reported_order()
#[tokio::test] async fn malformed_report_keeps_the_last_list()          // Report("{") → no publish, warn
#[tokio::test] async fn next_returns_a_queued_command_at_once()         // call windows.activate {id:"a"} → Next() == r#"{"action":"activate","id":"a"}"#
#[tokio::test] async fn next_returns_empty_after_the_timeout()
#[tokio::test] async fn stale_command_is_dropped()                      // queue, wait 60 ms, Next() == ""
#[tokio::test] async fn queue_keeps_the_newest_32()
#[tokio::test] async fn call_without_an_id_is_refused()                 // windows.close {} → no command, warn
```
In `server_handshake.rs`:
```rust
#[tokio::test] async fn unknown_call_gets_call_error()   // {"t":"call","method":"nope","args":{}} → {"t":"call_error","method":"nope","message":"unknown method"}
#[tokio::test] async fn provider_cannot_call()           // after provide → call_error "providers cannot call"
#[tokio::test] async fn known_call_reaches_the_feeds()   // a test feed sees Call { method: "windows.close", .. }
```

- [ ] **Step 2: Write the failing KWin-script tests** in `kwin-desktop.test.mjs`, with the existing stand-ins:

```js
test('reports windows most recently activated first', …)          // activate b then a → ids [a, b, c]
test('leaves skip-taskbar and non-normal windows out', …)
test('reports only when the list changes', …)                    // two identical changes → one sendWindows
test('a caption change is reported', …)
test('activate unminimizes and focuses the window', …)           // handleCommand('{"action":"activate","id":"x"}') → minimized false, workspace.activeWindow === x
test('minimize and close act on that window only', …)
test('unknown window command is ignored', …)                     // id not present → no throw, nothing changes
test('a malformed command is ignored', …)
```
Plus one for the poll loop in `main.qml` terms: `kwin_restart_keeps_the_poll_alive` asserts that `start()` asks for the next command again after `onCommand('')`. The `failed()` → 1 s retry lives in `main.qml` and is checked by the Step 5 smoke.

- [ ] **Step 3: Run them and watch them fail**

Run: `~/.cargo/bin/cargo test -p ghostd --test windows --test server_handshake; node --test shell/tests/kwin-desktop.test.mjs`
Expected: FAIL (missing types and functions).

- [ ] **Step 4: Implement:**
  - **Server.** It answers unknown methods and provider calls with `CallError`, and broadcasts the rest.
  - **`feeds/windows.rs`:**
    - `WindowsService` (Report) and `WindowCommands` (Next), sharing a `Mutex<VecDeque<(Instant, String)>>` and a `tokio::sync::Notify`;
    - a task that takes `windows.*` calls from `context.calls`, checks `args.id` is a non-empty string, and queues;
    - `pub(crate) const NEXT_TIMEOUT: Duration = 20 s`, `STALE_AFTER = 2 s`, `QUEUE_CAP = 32`.
  - **`desktop.rs`** serves both on its connection before taking the bus name.
  - **The fake daemon** publishes a fixed `windows` list (the mockup's `WINS` minus personal titles) for smokes.
  - **The script** keeps its MRU array from `windowActivated`. It reports on `windowAdded`, `windowRemoved`, `windowActivated`, `captionChanged`, `minimizedChanged`, `fullScreenChanged`, `demandsAttentionChanged` and `skipTaskbarChanged`.
  - **`main.qml`** adds a `DBusCall` for `Report` on `org.ghostshell.Windows1` and one for `Next`, whose `finished` hands `returnValue[0]` to `Desktop.handleCommand` and calls again. On `failed`, a 1 s `Timer` calls again.

- [ ] **Step 5: Run the tests, then the headless check**

Run: `~/.cargo/bin/cargo test --workspace && node --test 'shell/tests/*.test.mjs'`
Expected: PASS.

Then, in a headless KWin `3a-t4` with the script installed into the sandbox's `XDG_DATA_HOME` (as Task 1 did), a dev ghostd on a private bus, and `topic-tap.py windows`:
- open two windows and read the topic;
- send `windows.minimize` through a test client;
- check that `minimizedChanged` fires.

Record the click-to-act latency (median of 20).

- [ ] **Step 6: Measure.** Idle daemon CPU with the script connected for 60 s (expected ≤ +0.01 %) and the Rss delta.

- [ ] **Step 7: Commit**

```bash
git add crates/ghost-proto/src/msg.rs crates/ghost-proto/src/lib.rs crates/ghost-proto/src/snapshot.rs crates/ghostd/src/server/session.rs crates/ghostd/src/server/publish.rs crates/ghostd/src/server/mod.rs crates/ghostd/src/feeds/windows.rs crates/ghostd/src/feeds/desktop.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/fake/feeds.rs crates/ghostd/tests/windows.rs crates/ghostd/tests/server_handshake.rs kwin/ghost-shell-desktop shell/kit/GhostKit/Data.qml shell/tests/kwin-desktop.test.mjs
git commit -m "feat(ghostd): publish KWin's window list and pass the bar's window commands back"
```

---

### Task 5: The launcher toggle and the task shortcuts

**Files:**
- Modify: `crates/ghost-proto/src/snapshot.rs` (`DesktopSnapshot`), `crates/ghostd/src/feeds/desktop.rs`, `crates/ghostd/src/feeds/shortcuts.rs`
- Test: `crates/ghostd/tests/desktop.rs`, `crates/ghostd/tests/shortcuts.rs`

**Interfaces:**
- Consumes: `Shortcuts.tasks` (Task 3); Task 1's note on the Meta binding.
- Produces:
  - **`DesktopSnapshot` gains** `launcher_seq: u64`, `task: Option<u8>` (0-based pinned index) and `task_seq: u64`.
  - **`org.ghostshell.Desktop1.ToggleLauncher()`** bumps `launcher_seq` whether or not the desktop has focus.
  - **The Meta key (ledger R7):** `Shortcuts.launcher: String`, default `"Meta"`, registered as KGlobalAccel action `launcher` ("Open the launcher"), whose press bumps `launcher_seq`. `keyseq::parse("Meta")` returns `Qt::Key_Meta` (16777250) with no modifier bits, the value the spike saw KGlobalAccel store. Validation rejects a `launcher` key shared with `monitor`, `swap` or a task entry. KWin 6.7.5 has no `[ModifierOnlyShortcuts]`; until Task 17's retire frees bare Meta from plasmashell, `warn_if_shared` logs that plasmashell's launcher wins.
  - **KGlobalAccel actions** `task-1` … `task-10` ("Activate pinned app N") bump `task_seq` with `task = N-1`, whatever has focus. `Registration::release_on` also releases them.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test] async fn toggle_launcher_counts_even_over_a_window()  // active=false → launcher_seq 1
#[tokio::test] async fn launcher_shortcut_registers_bare_meta()      // setShortcutKeys(["ghost-shell","launcher",…], [16777250,0,0,0]); a press → launcher_seq 1
#[test] fn bare_meta_parses_as_the_meta_key()                         // keyseq::parse("Meta") == Ok(16777250); "Meta+" still rejected
#[tokio::test] async fn task_shortcut_publishes_its_index()          // stub kglobalaccel emits globalShortcutPressed("ghost-shell","task-3",0) → task Some(2), task_seq 1
#[tokio::test] async fn task_shortcuts_follow_config()               // tasks=["Meta+F1"] → setShortcutKeys for task-1 only; task-2..10 marked inactive
#[tokio::test] async fn release_covers_task_shortcuts()
#[tokio::test] async fn view_actions_still_need_the_desktop()        // regression: activate("monitor") with active=false → no change
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test desktop --test shortcuts`. Expected: FAIL.

- [ ] **Step 3: Implement.** Register task actions only for the configured entries, and mark the rest inactive so a shortened list frees its keys. `warn_if_shared` runs for each. A task press goes through `DesktopService::press_task(index)`, which never checks `active`.

- [ ] **Step 4: Run the tests and the checks.** Run: `~/.cargo/bin/cargo test --workspace && ~/.cargo/bin/cargo fmt --check && ~/.cargo/bin/cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src/snapshot.rs crates/ghost-proto/src/config.rs crates/ghostd/src/keyseq.rs crates/ghostd/src/config/load.rs crates/ghostd/src/config/default.toml crates/ghostd/src/feeds/desktop.rs crates/ghostd/src/feeds/shortcuts.rs crates/ghostd/tests/desktop.rs crates/ghostd/tests/shortcuts.rs
git commit -m "feat(ghostd): toggle the launcher from Meta and pass Meta+1…0 to the bar"
```

---

### Task 6: App badges and progress

**Files:**
- Create: `crates/ghostd/src/feeds/launcher_entries.rs`
- Modify: `crates/ghost-proto/src/lib.rs` (`topics::LAUNCHER_ENTRIES = "launcher.entries"`), `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/main.rs` (`host_feeds`), `crates/ghostd/src/fake/feeds.rs`
- Test: `crates/ghostd/tests/launcher_entries.rs`

**Interfaces:**
- Produces:
  - `LauncherEntries { entries: BTreeMap<String, LauncherEntry> }`, keyed by desktop id (`application://org.kde.dolphin.desktop` becomes `org.kde.dolphin`);
  - `LauncherEntry { count: Option<i64>, progress: Option<f64>, urgent: bool }`. `count` is `None` unless `count-visible` is true, and `progress` likewise with `progress-visible`; `progress` is clamped to 0–1.
  - The feed runs only while `launcher.entries` is wanted (`until_wanted`/`until_unwanted`). It matches `Update` signals from any sender, keeps each sender's entries, and drops them when that sender leaves the bus.

- [ ] **Step 1: Write the failing tests** (private bus, a client emitting `com.canonical.Unity.LauncherEntry.Update`):

```rust
#[tokio::test] async fn count_and_progress_are_published_by_desktop_id()
#[tokio::test] async fn count_hidden_unless_count_visible()
#[tokio::test] async fn later_update_merges_into_the_entry()       // progress then count → both kept
#[tokio::test] async fn entries_of_a_vanished_sender_are_dropped()
#[tokio::test] async fn uri_without_application_scheme_is_ignored()
#[tokio::test] async fn feed_is_quiet_while_unwanted()             // no match rule installed (bus's ListNames/GetConnectionUnixProcessID untouched; check no publish)
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test launcher_entries`. Expected: FAIL.

- [ ] **Step 3: Implement** with a `MatchRule` on interface `com.canonical.Unity.LauncherEntry`, member `Update`, and a `NameOwnerChanged` stream for cleanup. Publish only on change.

- [ ] **Step 4: Run the tests and the checks.** As in Task 5. Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src/lib.rs crates/ghost-proto/src/snapshot.rs crates/ghostd/src/feeds/launcher_entries.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/main.rs crates/ghostd/src/fake/feeds.rs crates/ghostd/tests/launcher_entries.rs
git commit -m "feat(ghostd): publish app badges and progress from the launcher entry API"
```

---

### Task 7: Session, KRunner and Plasma popup actions

**Files:**
- Create: `crates/ghostd/src/feeds/actions.rs`
- Modify: `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/main.rs`
- Test: `crates/ghostd/tests/actions.rs`, with stub services in `crates/ghostd/tests/common/actions.rs`

**Interfaces:**
- Consumes: `context.calls` (Task 4); Task 2's note for the popup route.
- Produces:
  - `pub fn feed() -> Feed` and `pub fn feed_on(session: Option<Connection>, system: Option<Connection>) -> Feed`. Nothing is published: the feed publishes no topic and waits for calls.
  - **Mapping:**
    - `session.lock` → `org.freedesktop.ScreenSaver /ScreenSaver Lock()` (session);
    - `session.logout` / `session.reboot` / `session.shutdown` → `org.kde.Shutdown /Shutdown logout()` / `logoutAndReboot()` / `logoutAndShutdown()` (session);
    - `session.suspend` → `org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager.Suspend(false)` (system);
    - `krunner.query {text}` → `org.kde.krunner /App org.kde.krunner.App.query(s)`. Text is trimmed and capped at 256 characters, and empty text is refused;
    - `plasma.popup {applet}` → Task 2's route, for `applet` in exactly `["org.kde.plasma.networkmanagement", "org.kde.plasma.bluetooth", "org.kde.plasma.volume", "org.kde.kdeconnect", "org.kde.plasma.clipboard", "org.kde.plasma.systemtray", "org.kde.plasma.digitalclock"]`; anything else is refused.
  - Every call uses `no_autostart` except `krunner.query`, since KRunner is D-Bus activated. Each gives up after 2 s, and failures are logged with the method name.

- [ ] **Step 1: Write the failing tests.** A private session bus and a private system bus each carry stubs that record every call. Stubs only, so nothing can reach a real session:

```rust
#[tokio::test] async fn lock_calls_the_screen_saver()
#[tokio::test] async fn logout_reboot_shutdown_go_through_ksmserver()   // three calls, three methods, in order
#[tokio::test] async fn suspend_asks_logind_without_interaction()        // Suspend(false)
#[tokio::test] async fn krunner_gets_the_trimmed_query()                 // "  fire " → query("fire")
#[tokio::test] async fn empty_krunner_query_is_refused()
#[tokio::test] async fn popup_for_a_known_applet_uses_the_spike_route()
#[tokio::test] async fn popup_for_an_unknown_applet_is_refused()
#[tokio::test] async fn missing_service_is_logged_not_fatal()            // no ScreenSaver stub → feed keeps running
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test actions`. Expected: FAIL.

- [ ] **Step 3: Implement** as above. `main.rs` adds `feeds::actions::feed()` to `host_feeds`, and the fake daemon gets nothing, so dev daemons can never act.

- [ ] **Step 4: Run the tests and the checks.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/actions.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/main.rs crates/ghostd/tests/actions.rs crates/ghostd/tests/common/actions.rs crates/ghostd/tests/common/mod.rs
git commit -m "feat(ghostd): run the launcher's session actions and open KDE's popups on request"
```

---

### Task 8: The ticker feed

**Files:**
- Create: `crates/ghostd/src/feeds/ticker.rs`
- Modify: `crates/ghost-proto/src/lib.rs` (`topics::TICKER = "ticker"`), `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/main.rs`, `crates/ghostd/src/fake/feeds.rs`
- Test: `crates/ghostd/tests/ticker.rs`

**Interfaces:**
- Produces: `TickerSnapshot { failed_units: Option<u32>, firewalld: Option<bool>, updates: Option<u32>, nvidia_driver: Option<String> }`. `None` means unknown, and the shell leaves that segment out. Kernel, IPs, VPN and critical items come from the existing `host`, `net.links`, `disks.health` and `weather.alerts` topics in the shell.
- **Cadence** (all event-driven, and only while `ticker` is wanted):
  - **`failed_units`** is the system manager's `NFailedUnits` plus the user manager's. It is read at want, after `Subscribe()`, and again after each `JobRemoved` and `Reloading(false)`, coalesced to at most one read per 500 ms.
  - **`firewalld`** is `org.fedoraproject.FirewallD1`'s `state == "RUNNING"`, re-read on its `NameOwnerChanged`. With no owner it is `false`.
  - **`updates`** comes from a PackageKit `CreateTransaction` and `GetUpdates(filter = 0)`, counting `Package` signals until `Finished`, with a 30 s timeout. It runs at want and on `UpdatesChanged`, and never calls `RefreshCache`.
  - **`nvidia_driver`** is `/sys/module/nvidia/version`, trimmed, read once at want.

- [ ] **Step 1: Write the failing tests** (private system and session buses with systemd, firewalld and PackageKit stubs; a temp sysroot for the driver file):

```rust
#[tokio::test] async fn failed_units_sum_system_and_user()          // 1 + 2 → Some(3)
#[tokio::test] async fn failed_units_refresh_after_a_job_ends()
#[tokio::test] async fn bursts_of_job_signals_coalesce()             // 20 JobRemoved in 100 ms → ≤ 2 property reads
#[tokio::test] async fn firewalld_absent_reads_false()
#[tokio::test] async fn updates_are_counted_from_the_cached_list()  // 16 Package signals → Some(16); no RefreshCache call recorded
#[tokio::test] async fn packagekit_error_leaves_updates_unknown()
#[tokio::test] async fn missing_nvidia_module_leaves_the_driver_unknown()
#[tokio::test] async fn ticker_idles_while_unwanted()                // stub sees no calls until wanted
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test ticker`. Expected: FAIL.

- [ ] **Step 3: Implement** as above, with the sysroot injected the way `host` takes `SysRoot`.

- [ ] **Step 4: Run the tests and the checks; measure.** Report the wall time and on-CPU time per refresh of each source against the live buses read-only. Run once by hand, with no `RefreshCache`, using a throwaway harness binary in `/tmp`.

- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto/src/lib.rs crates/ghost-proto/src/snapshot.rs crates/ghostd/src/feeds/ticker.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/main.rs crates/ghostd/src/fake/feeds.rs crates/ghostd/tests/ticker.rs
git commit -m "feat(ghostd): publish failed units, firewalld and pending updates for the ticker"
```

---

### Task 9: Recent files

**Files:**
- Create: `crates/ghostd/src/feeds/recent.rs`
- Modify: `Cargo.toml` (workspace `quick-xml`), `crates/ghostd/Cargo.toml`, `crates/ghost-proto/src/lib.rs` (`topics::RECENT = "recent"`), `crates/ghost-proto/src/snapshot.rs`, `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/main.rs`, `crates/ghostd/src/fake/feeds.rs`
- Test: `crates/ghostd/tests/recent.rs`

**Interfaces:**
- Consumes: Task 2's recent-files finding. If it says the xbel list differs from Kickoff's, stop and ask before this task.
- Produces:
  - `RecentFiles { files: Vec<RecentFile> }`, at most 5, newest `modified` first;
  - `RecentFile { path: String, name: String, dir: String, modified_s: u64 }`, where `dir` is shortened to `~/…` under `$HOME`.
  - Only `file://` bookmarks whose file exists are listed, percent-decoded, one per path.
  - The feed reads `$XDG_DATA_HOME/recently-used.xbel` (default `~/.local/share`) at want. It watches the file's directory while wanted, re-reading 250 ms after the last change, and stops watching when unwanted.

- [ ] **Step 1: Write the failing tests** (temp data home with fixture XBEL files):

```rust
#[tokio::test] async fn newest_five_files_first()
#[tokio::test] async fn non_file_urls_are_skipped()                 // http://, trash:/ skipped
#[tokio::test] async fn missing_files_are_skipped()
#[tokio::test] async fn percent_encoded_paths_decode()              // file:///tmp/a%20b.txt → "a b.txt"
#[tokio::test] async fn home_folder_is_shortened()                  // dir "~/Downloads"
#[tokio::test] async fn rewritten_file_republishes()                // atomic rename of a new xbel → new list within 1 s
#[tokio::test] async fn malformed_xbel_publishes_an_empty_list()
#[tokio::test] async fn no_watch_while_unwanted()
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test recent`. Expected: FAIL.

- [ ] **Step 3: Implement** with `quick_xml::Reader` over `<bookmark href modified>` start tags, and the `notify` watcher pattern of `config/watch.rs`.

- [ ] **Step 4: Run the tests and the checks; measure** one read of the live 206 KiB file (wall time and CPU time).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghostd/Cargo.toml crates/ghost-proto/src/lib.rs crates/ghost-proto/src/snapshot.rs crates/ghostd/src/feeds/recent.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/main.rs crates/ghostd/src/fake/feeds.rs crates/ghostd/tests/recent.rs
git commit -m "feat(ghostd): publish the five newest recent files for the launcher"
```

---

### Task 10: Pinning and the first-run import

**Files:**
- Create: `crates/ghostd/src/config/edit.rs`, `crates/ghostd/src/feeds/pins.rs`
- Modify: `Cargo.toml` (workspace `toml_edit`), `crates/ghostd/Cargo.toml`, `crates/ghostd/src/config/mod.rs`, `crates/ghostd/src/feeds/mod.rs`, `crates/ghostd/src/main.rs` (passes the config path)
- Test: `crates/ghostd/tests/pins.rs`

**Interfaces:**
- Consumes: `bar.pin`/`bar.unpin` calls (Task 4).
- Produces:
  - **`edit::set_pinned(path: &Path, change: PinChange) -> Result<bool, EditError>`**, with `PinChange::Add(String)` and `PinChange::Remove(String)`. It returns whether the file changed.
    - Add appends to `bar.pinned` if absent, creating `[bar]` and the key if needed. Remove deletes every equal entry.
    - It refuses (`EditError::Unparsable`) a file that doesn't parse, and never writes one.
    - It writes to `<path>.tmp` and renames, keeping comments, order and formatting elsewhere.
    - An app id must match `^[A-Za-z0-9._-]+$`.
  - **`edit::import_plasma_launchers(config: &Path, appletsrc: &Path, mimeapps: &Path) -> Result<Option<Vec<String>>, EditError>`** applies call 15 and writes only when the config file has no `bar.pinned` key. It returns the imported list.
  - **`feeds::pins::feed(config_path: PathBuf, plasma_dir: PathBuf) -> Feed`** imports once at start, then serves the two calls. The config watcher picks up its writes like any save.

- [ ] **Step 1: Write the failing tests** (temp files):

```rust
#[test] fn pin_appends_and_keeps_comments()            // file with comments above and inside [bar] → only the array changes
#[test] fn pin_twice_is_one_entry()
#[test] fn pin_into_a_file_without_a_bar_table()
#[test] fn unpin_removes_only_that_app()
#[test] fn unparsable_file_is_never_written()           // returns Unparsable, bytes unchanged
#[test] fn invalid_app_id_is_refused()                  // "../x", "a b"
#[test] fn import_reads_plasma_launchers_once()         // fixture appletsrc with the user's launchers= shape → ["org.kde.dolphin","steam",…]
#[test] fn explicit_empty_pinned_is_not_overwritten()
#[test] fn preferred_filemanager_resolves_through_mimeapps()
#[test] fn unresolvable_preferred_launcher_is_dropped()
```

- [ ] **Step 2: Run them and watch them fail.** Run: `~/.cargo/bin/cargo test -p ghostd --test pins`. Expected: FAIL.

- [ ] **Step 3: Implement** with `toml_edit::DocumentMut`, and the appletsrc read as INI lines: find the `plugin=org.kde.plasma.icontasks` or `taskmanager` applet's `[Configuration][General]` `launchers=`.

- [ ] **Step 4: Run the tests and the checks.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghostd/Cargo.toml crates/ghostd/src/config/edit.rs crates/ghostd/src/config/mod.rs crates/ghostd/src/feeds/pins.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/src/main.rs crates/ghostd/tests/pins.rs
git commit -m "feat(ghostd): pin and unpin apps in config.toml and import the Plasma panel's launchers"
```

---

### Task 11: The bar surface, its bands, and the first bar widgets

**Files:**
- Create the hosts: `shell/host/Bar.qml` (the bar surface per main screen), `shell/host/BarRow.qml` (left/centre/right hosts), `shell/host/Dock.qml` (layout B).
- Create the kit pieces:
  - `shell/kit/GhostKit/BarPopup.qml`: a `utility` layer surface (call 1) placed beside an anchor item, towards the screen's middle;
  - `shell/kit/GhostKit/OutsideCatcher.qml`;
  - `shell/kit/GhostKit/AppIcon.qml`: an icon by name through `image://icon/`, mono through `Duotone` with `shadowColor: "transparent"`, `hi` when `focused`.
- Create the widgets: `widgets/brand/`, `widgets/view-chip/`, `widgets/bar-clock/`, each with `manifest.toml` (`kinds = ["bar"]`) and `Widget.qml`.
- Modify the shell:
  - `shell/shell.qml`;
  - `shell/host/LayoutView.qml` (bands from `Layout.contentArea`);
  - `shell/host/Desktop.qml` (stop suspending `FrameClock` on hide);
  - `shell/kit/GhostKit/lib/layout.mjs`, `shell/kit/GhostKit/lib/subscriptions.mjs`, `shell/kit/GhostKit/Data.qml` (`barRequires` from the layout), `shell/kit/GhostKit/lib/theme.mjs` (new tags);
  - `shell/kit/GhostKit/Glyph.qml` (new shapes), `shell/kit/GhostKit/qmldir`.
- Modify the scripts: `scripts/qml-smoke.sh` and `scripts/smoke-holds.sh` (the bar on, plus `GHOST_SMOKE_BAR` with `top|bottom` and `a|b|c`), `scripts/measure-covered.sh` (`GHOST_BAR=off`).
- Test: `shell/tests/layout.test.mjs`, `shell/tests/subscriptions.test.mjs`, `shell/tests/theme.test.mjs`.

**Interfaces:**
- Consumes: `Data.layout.bar` (Task 3), `Data.call` (Task 4), `plasma.popup` (Task 7), `Views`.
- Produces:
  - **`layout.mjs`:** `contentArea({designWidth, designHeight, margin, position, hasDock})` returns `{x, y, w, h}`:
    - the 30 px bar band goes on the bar's side;
    - with `hasDock`, a 58 px dock band goes on the other side;
    - `margin` applies on every side.
  - **`subscriptions.mjs`:**
    - `barRequiresOf(layout)` collects the `requires` of `layout.bar.left/centre/right/dock`, sorted and unique;
    - `subscriptionFor` keeps `wanted` while covered (call 10).
  - **`Data.barRequires`** is `Subscriptions.barRequiresOf(layout)` once a layout arrives.
  - **Bar slots** are created by `WidgetHost` with `settings` and with `firstWriteScope: FirstWrite.hostScope("bar", path)`. The row lays a slot out at its `implicitWidth`, full bar height.
  - A slot with an error shows a 30 × 30 mark: the `crit` square and the widget id in `dim` as a tooltip. That is the bar's error card.
  - **`BarPopup`:**
    - properties `anchorItem: Item`, `open: bool`, `content: Component`, `catchOutside: bool`;
    - it opens on the anchor's far side from the bar (below a top bar, above a bottom one, above the dock in B), clamped to the screen;
    - it is a `PanelWindow` in namespace `utility` (call 1) with no exclusive zone and keyboard focus `None` unless set.
  - **`brand`:**
    - reads `Data.config.bar.brand`;
    - a click calls `Launcher.toggle()` once Task 16 lands; until then it logs `launcher not built yet`;
    - `implicitWidth` is 0 when disabled.
  - **`view-chip`:** the chip calls `Views.toggleMonitor()` and the chevron calls `Views.swap()`. It reads `Views.isMonitor`.
  - **`bar-clock`:**
    - holds `FrameClock.acquireSeconds()` while visible;
    - a click calls `Data.call("plasma.popup", {applet: "org.kde.plasma.digitalclock"})`;
    - manifest `requires = []`.
  - **`Bar.qml`** logs `bar on <output> <position> layout <a|b|c>`. It is not created when `GHOST_BAR=off`.

- [ ] **Step 1: Write the failing node tests**

```js
test('content area reserves the bar band on top', () => assert.deepEqual(contentArea({designWidth: 1280, designHeight: 720, margin: 12, position: 'top', hasDock: false}), {x: 12, y: 42, w: 1256, h: 666}));
test('content area reserves the bar band at the bottom', () => assert.deepEqual(contentArea({…, position: 'bottom', hasDock: false}), {x: 12, y: 12, w: 1256, h: 666}));
test('layout b reserves the dock band opposite the bar', () => assert.deepEqual(contentArea({…, position: 'top', hasDock: true}), {x: 12, y: 42, w: 1256, h: 608}));
test('bar requires collect bar and dock slots', …);        // fixture layout → ['host','windows', …] sorted, unique
test('covered keeps wanted topics and bar topics', () => assert.deepEqual(subscriptionFor({requires: ['cpu'], wanted: ['net'], covered: true, barRequires: ['windows']}), ['desktop','net','perf','session','windows']));
test('launcher tags', () => { assert.equal(tag('launch', 'mixed'), '起動'); assert.equal(tag('recent', 'katakana'), 'ヒストリー'); assert.equal(tag('pin', 'off'), ''); });
```
The existing covered test in `subscriptions.test.mjs` that expects wanted topics dropped changes to the new rule. Its comment says why (call 10).

- [ ] **Step 2: Run them and watch them fail.** Run: `node --test 'shell/tests/*.test.mjs'`. Expected: FAIL.

- [ ] **Step 3: Implement** the libraries, hosts, kit pieces and three widgets to the Global Constraints' bar values. `LayoutView` uses `contentArea`, with `position` from `Data.config.bar.position` and `hasDock` from `layout === "b"`.

- [ ] **Step 4: Run the tests and the smokes**

Run: `node --test 'shell/tests/*.test.mjs' && make smoke && make smoke-holds`
Expected: PASS.
- The smoke snapshot (`QT_SCALE_FACTOR=1.15`) shows the bar with the brand, the view chip and the clock on the right. The other preset slots show error marks until their widgets land, and the smoke's log scan allows exactly those `no widget named` lines until Task 15.
- `GHOST_SMOKE_BAR=bottom` puts it at the bottom with the dashboard's band moved.
- A layout naming `processor` in the bar shows one error mark (`broken_bar_slot_shows_an_error_mark`).

- [ ] **Step 5: Measure.** Covered with the bar on (a new `measure-covered.sh` mode with `GHOST_BAR=on`): CPU and frames/s. Then `measure-cpu.sh` on V1 and MONITOR, with Rss for each. Compare with 2b's acceptance figures and apply the O1 stop rule.

- [ ] **Step 6: Commit**

```bash
git add shell/host/Bar.qml shell/host/BarRow.qml shell/host/Dock.qml shell/host/LayoutView.qml shell/host/Desktop.qml shell/shell.qml shell/kit/GhostKit/BarPopup.qml shell/kit/GhostKit/OutsideCatcher.qml shell/kit/GhostKit/AppIcon.qml shell/kit/GhostKit/Glyph.qml shell/kit/GhostKit/qmldir shell/kit/GhostKit/Data.qml shell/kit/GhostKit/lib/layout.mjs shell/kit/GhostKit/lib/subscriptions.mjs shell/kit/GhostKit/lib/theme.mjs shell/tests/layout.test.mjs shell/tests/subscriptions.test.mjs shell/tests/theme.test.mjs widgets/brand widgets/view-chip widgets/bar-clock scripts/qml-smoke.sh scripts/smoke-holds.sh scripts/measure-covered.sh
git commit -m "feat(shell): add the bar surface with the brand block, view chip and clock"
```

---

### Task 12: Tasks as icons, and the dock

**Files:**
- Create: `shell/kit/GhostKit/lib/tasks.mjs`, `widgets/tasks/` (`manifest.toml`, `Widget.qml`, `IconTasks.qml`, `TitleList.qml`, `TaskMenu.qml`), `widgets/launcher-tile/`
- Modify: `shell/host/Dock.qml`
- Test: `shell/tests/tasks.test.mjs`

**Interfaces:**
- Consumes:
  - `windows`, `launcher.entries` and `desktop` (`task`, `task_seq`) topics;
  - `Data.call("windows.*")` and `Data.call("bar.pin"|"bar.unpin")`;
  - `DesktopEntries`, `AppIcon`, `BarPopup`.
- Produces (`tasks.mjs`, pure, with lookups injected):
  - **`buildTasks({pinned: string[], windows: WindowInfo[], findEntry: (window) => entryId|null})`** returns `Task[]`:
    - pinned apps in config order, then running unpinned apps in first-seen MRU order;
    - `Task = { appId, pinned: bool, windows: WindowInfo[] (MRU), focused: bool }`;
    - `focused` means one of its windows is `active`.
  - **`clickAction(task)`** returns one of:
    - `{kind: 'launch'}` with no windows;
    - `{kind: 'list'}` with ≥ 2;
    - `{kind: 'minimize', id}` when its one window is active;
    - `{kind: 'activate', id}` otherwise.
  - **`marks(task)`** returns `{count: min(windows, 4), focusedFirst: bool}`.
  - **`badge(entries, appId)`** returns `{count: number|null, progress: number|null, urgent: bool}`.
  - **`matchOrder`** is the call 5 order, used by `findEntry`, built in the widget from `DesktopEntries`.
- **Widget settings:** `style` enum `["icons", "tabs"]`, default `icons`. `requires = ["windows", "launcher.entries", "desktop"]`.
- **Pointer behaviour:**
  - middle-click runs the entry (`execute()` or Task 1's route);
  - hover shows `TitleList` (app name UPPERCASE, then one row per window, clicking a row activates it), or the name and `NOT RUNNING`;
  - right-click shows `TaskMenu`: the entry's actions, then `PIN`/`UNPIN`, then `CLOSE` (closes every window of the app; hidden when not running);
  - `desktop.task_seq` changes run `clickAction` on `pinned[task]`, if it exists.
- **Badges:**
  - the count is a `signal` chip of 8 px text on the icon's top-right;
  - progress is a 4-segment thin mark under the icon in place of the window marks;
  - urgent uses the `warn` colour for the marks.
- **Dock** (layout B): `launcher-tile` (calls `Launcher.toggle()`), then the tasks, with a separator before the first unpinned task.

- [ ] **Step 1: Write the failing tests**

```js
test('pinned first then running unpinned', …)                      // pinned [a,b], windows of c,a → [a(1),b(0),c(1)]
test('window joins its pinned app through startup class', …)        // findEntry maps class "Steam" → "steam"
test('unmatched window is its own task', …)                         // appId = 'class:<resourceClass>'
test('click on an app with no window launches', …)
test('click on a single window activates it', …)
test('click on the focused single window minimizes it', …)
test('click on an app with two windows opens the list, focused or not', …)
test('marks cap at four and flag the focused app', …)
test('badge hides a count the app made invisible', …)               // entries[app].count null → null
test('task shortcut beyond the pinned list does nothing', …)       // index 5 with 3 pins → null
```

- [ ] **Step 2: Run them and watch them fail.** Run: `node --test shell/tests/tasks.test.mjs`. Expected: FAIL.

- [ ] **Step 3: Implement** the library, the widget and the dock to the Global Constraints' values.

- [ ] **Step 4: Run the tests and the smokes.**
  - `make smoke` with the fake `windows` topic: the snapshot shows 11 pinned icons with marks, the focused one's long mark, and dimmed non-running pins.
  - `GHOST_SMOKE_BAR=b` shows the dock with its tile and separator, and the dashboard's band.
  - `make smoke-holds` shows that a hover and a menu leave no holds.

- [ ] **Step 5: Measure.** CPU and Rss deltas, covered and on V1. Apply O1.

- [ ] **Step 6: Commit**

```bash
git add shell/kit/GhostKit/lib/tasks.mjs shell/tests/tasks.test.mjs widgets/tasks widgets/launcher-tile shell/host/Dock.qml
git commit -m "feat(widgets): show pinned and running apps in the bar and the dock"
```

---

### Task 13: Tasks as window tabs

**Files:**
- Modify: `shell/kit/GhostKit/lib/tasks.mjs`, `widgets/tasks/Widget.qml`
- Create: `widgets/tasks/TabTasks.qml`
- Test: `shell/tests/tasks.test.mjs`

**Interfaces:**
- Produces:
  - **`tabsFor(windows, availablePx, tabPx = 128, morePx = 32)`** returns `{shown: WindowInfo[], hidden: number}`, MRU order. `shown` has as many windows as fit with room for `+N` when any are hidden.
  - **`tabClick(window)`** returns `{kind: 'minimize', id}` when the window is active and `{kind: 'activate', id}` otherwise.
  - The `+N` label opens `TitleList` with the hidden windows.
  - Tab labels are `pad2(index + 1)`, the icon, and the app name UPPERCASE (the entry's name, or the resource class).

- [ ] **Step 1: Write the failing tests**

```js
test('tabs are most recently activated first', …)
test('tabs overflow counts hidden windows', () => assert.deepEqual(tabsFor(twenty, 600).hidden, 16))  // 4 × 128 + 32 ≤ 600
test('no overflow label when everything fits', …)
test('clicking the active tab minimizes it', …)
test('index is two digits', …)                                       // 1 → "01"
```

- [ ] **Step 2: Run them and watch them fail.** Expected: FAIL.
- [ ] **Step 3: Implement** to the tab values in Global Constraints.
- [ ] **Step 4: Run the tests and `GHOST_SMOKE_BAR=c make smoke`.** The snapshot shows tabs, the first one selected, and `+N`.
- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib/tasks.mjs shell/tests/tasks.test.mjs widgets/tasks
git commit -m "feat(widgets): show open windows as tabs in bar layout C"
```

---

### Task 14: Status and ticker

**Files:**
- Create: `shell/kit/GhostKit/lib/ticker.mjs`, `widgets/status/`, `widgets/ticker/`
- Test: `shell/tests/ticker.test.mjs`, `shell/tests/format.test.mjs`

**Interfaces:**
- Consumes: the `cpu`, `gpu`, `mem`, `net`, `net.links`, `weather`, `weather.alerts`, `host`, `disks.health` and `ticker` topics; `Fmt`.
- Produces:
  - **`tickerSegments({ticker, host, links, health, alerts, nowS})`** returns `[{text, level}]` in this order, each left out when unknown:
    - `SYSTEMD // <n> FAILED UNITS` (`crit` when n > 0);
    - `FIREWALLD ACTIVE|INACTIVE` (`warn` when inactive);
    - `KERNEL <UPPER kernel>`;
    - `NVIDIA <driver>`;
    - `<IFACE> <ip> <speed>` per LAN link that is up (`ENP14S0 192.168.15.5 1GBE`);
    - `<IFACE> <ip>` for Tailscale;
    - `<VPN NAME> UP|DOWN` per VPN;
    - each dead drive `<NAME> CONTROLLER DEAD` (`crit`);
    - each active weather alert's event and level (`warn`/`crit`);
    - `<n> UPDATES PENDING`.
  - **`tickerText(segments)`** joins the segments with ` // ` and appends ` //`.
  - **`format.mjs`** gains `memShort(usedKib, totalKib)` → `"23.4/92G"`.
  - **`status`** has a `parts` setting (list, default all six). It wants only its parts' topics (call 9) and draws them in the order `cpu, gpu, mem, net, vpn, weather`.
  - **`ticker`:**
    - `requires = ["ticker", "host", "net.links", "disks.health", "weather.alerts"]`;
    - it holds `FrameClock.acquire()` only while its text is wider than its slot, and releases it otherwise and when hidden;
    - the offset is `(FrameClock.epochMs % 60000) / 60000 × copyWidth`, snapped to whole device pixels, so held frames step it.

- [ ] **Step 1: Write the failing tests**

```js
test('ticker reads like the mockup', () => assert.equal(tickerText(tickerSegments(MOCKUP_FIXTURE)), 'SYSTEMD // 0 FAILED UNITS // FIREWALLD ACTIVE // KERNEL 7.2.11-200.FC44 // NVIDIA 615.78.08 // ENP14S0 192.168.15.5 1GBE // TAILSCALE0 100.122.159.93 // NVME1 CONTROLLER DEAD // 16 UPDATES PENDING //'));
test('failed units turn crit', …)
test('unknown parts are left out', …)                     // ticker {} → no SYSTEMD, FIREWALLD, UPDATES segments
test('memory reads used over total', () => assert.equal(memShort(24536678, 96366592), '23.4/92G'));
```

- [ ] **Step 2: Run them and watch them fail.** Expected: FAIL.
- [ ] **Step 3: Implement** to the status and ticker values in Global Constraints.
- [ ] **Step 4: Run the tests and the smokes** for A (status: CPU, traffic, VPN, WX) and B (ticker scrolling, which smoke-holds sees acquire and release). Measure B's covered CPU with the ticker scrolling and report it; past budget, stop and ask (B isn't the user's daily layout, but its numbers must be on record).
- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib/ticker.mjs shell/kit/GhostKit/lib/format.mjs shell/tests/ticker.test.mjs shell/tests/format.test.mjs widgets/status widgets/ticker
git commit -m "feat(widgets): add the bar's status readout and the scrolling ticker"
```

---

### Task 15: Tray

**Files:**
- Create: `shell/kit/GhostKit/lib/tray.mjs`, `widgets/tray/` (`manifest.toml`, `Widget.qml`, `TrayItem.qml`, `SniItem.qml`)
- Test: `shell/tests/tray.test.mjs`

**Interfaces:**
- Consumes: `Quickshell.Bluetooth`, `Quickshell.Services.Pipewire`, `Quickshell.Services.SystemTray`, the `net.links` and `batteries` topics, and `Data.call("plasma.popup")`.
- Produces (`tray.mjs`):
  - `networkState(links)` returns `{glyph: 'network', dim: !anyLanUp}`;
  - `bluetoothState({enabled, connected})` returns `{glyph: 'bluetooth', dim: !enabled}`;
  - `volumeState({volume, muted})` returns `{glyph: 'volume', dim: muted, label: 'VOLUME 64%'}`;
  - `kdeConnectState(batteries)` returns `{glyph: 'phone', dim: !anyPhone}`;
  - `stepVolume(volume, notches)` returns the volume clamped to 0–1 in steps of 0.05;
  - `APPLETS = {network: 'org.kde.plasma.networkmanagement', bluetooth: 'org.kde.plasma.bluetooth', volume: 'org.kde.plasma.volume', kdeconnect: 'org.kde.kdeconnect', clipboard: 'org.kde.plasma.clipboard', overflow: 'org.kde.plasma.systemtray'}`.
- **Widget behaviour:**
  - built-ins, then the SNI items, then the overflow chevron (`chevron-down` on a top bar, `chevron-up` on a bottom one);
  - left-click on a built-in or the chevron sends `plasma.popup`;
  - an SNI item activates on left-click, secondary-activates on middle-click, and shows its `DBusMenu` on right-click. The menu is a `BarPopup` listing the menu's entries in the panel style. Submenus open beside it;
  - SNI icons follow mono/colour through `AppIcon`;
  - the volume icon steps on wheel and mutes on middle-click (call 12);
  - tooltips show each item's title.

- [ ] **Step 1: Write the failing tests**

```js
test('network is dim with no LAN link up', …)
test('volume steps by five percent and clamps', () => { assert.equal(stepVolume(0.98, 1), 1); assert.equal(stepVolume(0.5, -2), 0.4); });
test('muted volume is dim', …)
test('kde connect is dim without a phone', …)
test('every applet name is one the daemon accepts', …)   // compares APPLETS values to the list in Task 7
```

- [ ] **Step 2: Run them and watch them fail.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run the tests and the smokes.** The snapshot shows the tray icons and the chevron. The smoke's log scan no longer allows `no widget named` lines.
- [ ] **Step 5: Commit**

```bash
git add shell/kit/GhostKit/lib/tray.mjs shell/tests/tray.test.mjs widgets/tray scripts/qml-smoke.sh
git commit -m "feat(widgets): add the tray with KDE's popups behind its icons and overflow"
```

---

### Task 16: The launcher

**Files:**
- Create:
  - `shell/kit/GhostKit/lib/launcher.mjs`;
  - `shell/kit/GhostKit/Launcher.qml` (public singleton: `open`, `toggle()`, `close()`);
  - `shell/host/LauncherPanel.qml` (the drop surface);
  - `shell/host/LauncherList.qml`.
- Modify: `shell/shell.qml`, `shell/kit/GhostKit/qmldir`, `widgets/brand/Widget.qml`, `widgets/launcher-tile/Widget.qml`
- Test: `shell/tests/launcher.test.mjs`

**Interfaces:**
- Consumes:
  - `DesktopEntries`;
  - the `recent` topic (wanted only while open) and `desktop.launcher_seq`;
  - `Data.call("session.*"|"krunner.query")`;
  - Task 1's launch route.
- Produces (`launcher.mjs`, pure):
  - **`dedupe(entries)`** keeps one entry per `name.toLowerCase()` and `command` pair, preferring user over system over Flatpak exports by entry path. Entries with `noDisplay` are dropped.
  - **`category(categories)`** returns the first match in this order:
    - `Game` → `GAME`;
    - `Development` → `DEV`;
    - `Network` → `NETWORK`;
    - `AudioVideo|Audio|Video` → `MEDIA`;
    - `Graphics` → `GRAPHICS`;
    - `Office` → `OFFICE`;
    - `Settings` → `SETTINGS`;
    - `System` → `SYSTEM`;
    - anything else → `UTILITY`.
  - **`rank(entries, query)`** is the mockup's `score`, in order: name prefix 0, name word prefix 1, generic-name or keyword word prefix 2, name substring 3, else none. Ties sort by name; the query is matched case-insensitively.
  - **`sections({query, pinned, recent, all})`** returns the list of selectable items in display order:
    - with an empty query: `pin` × pinned, `file` × recent, `app` × all;
    - otherwise: `app` × ranked.
  - **`move(state, key)`:**
    - in the pin grid, Down moves by 6 while that stays in the grid, and otherwise to the first item after it;
    - in the pin grid, Up moves by 6 while that stays at or above 0;
    - Left and Right move by 1 inside the grid;
    - past the grid, Up and Down move by 1, clamped, and Up from the first item after the grid goes to the last pin.

    This fixes the mockup's rule, which moved from the second row of pins to the next pin instead of down into the list.
  - **`power(state, action, nowMs)`** returns `{state, fire: action|null}`:
    - `lock` fires at once;
    - another action arms on its first press and fires on a press of the same action within 3000 ms of arming;
    - presses within 250 ms of the arming press are ignored, so key repeat and double-clicks never confirm;
    - a different action re-arms.
  - **`disarm(state)`** is called on close.
- **The host:**
  - opens on `Launcher.toggle()` and on each new `launcher_seq`;
  - closes on Esc, a launch, a fired power action, or `OutsideCatcher`;
  - runs the print reveal and the header scramble;
  - logs `launcher opened` / `launcher closed` / `launched <id>`.
- **With no match,** Enter sends `krunner.query` with the typed text, and the list reads `NO MATCH · ENTER RUNS "<QUERY>" IN KRUNNER`.

- [ ] **Step 1: Write the failing tests**

```js
test('name prefix beats word prefix beats keyword beats substring', …)   // "fire": Firefox, Firewall (name prefix) before "…fire…" substring
test('generic name and keywords are searched', …)                        // "browser" finds Firefox, Thorium, Google Chrome
test('duplicates merge user over system over flatpak', …)                // two Dolphins → the user entry
test('noDisplay entries are dropped', …)
test('categories map to chips', () => { assert.equal(category(['Qt','KDE','Game']), 'GAME'); assert.equal(category(['Utility']), 'UTILITY'); });
test('empty query lists pins then recent then all', …)
test('down moves a row in the pin grid then one row at a time', …)       // 0→6, 6→11 (first file) with 11 pins
test('power needs a second press within three seconds', …)              // arm at 0, press at 2999 → fire; at 3001 → re-arm
test('key repeat does not confirm', …)                                   // presses at 0 and 100 → no fire
test('lock fires at once', …)
test('closing disarms', …)
```

- [ ] **Step 2: Run them and watch them fail.** Expected: FAIL.
- [ ] **Step 3: Implement** to the launcher values in Global Constraints.
- [ ] **Step 4: Run the tests and the smokes.**
  - A debug IPC `launcher open|type <text>|key <name>` (in the singleton, served under `GHOST_DEBUG=1`) drives a smoke. It opens, types `fire`, takes a snapshot (top and bottom bar), presses Down and Enter on the fake entries, and expects `launched` in the log.
  - It then opens, presses `SHUT DOWN` twice through the IPC, and expects one `call session.shutdown` at the fake daemon, which acts on nothing.
  - `smoke-holds` shows that open-and-close leaves no `recent` want and no clock hold.
- [ ] **Step 5: Measure** the Rss with the launcher closed and after one open, and CPU closed (expected unchanged).
- [ ] **Step 6: Commit**

```bash
git add shell/kit/GhostKit/lib/launcher.mjs shell/kit/GhostKit/Launcher.qml shell/kit/GhostKit/qmldir shell/host/LauncherPanel.qml shell/host/LauncherList.qml shell/shell.qml shell/tests/launcher.test.mjs widgets/brand widgets/launcher-tile scripts/qml-smoke.sh
git commit -m "feat(shell): add the drop-panel launcher with search, recent files and power"
```

---

### Task 17: Handing over from Plasma

**Files:**
- Create: `scripts/plasma-panel.sh` (from Task 2's prototype)
- Modify: `scripts/install.sh` (prints the retire step, never runs it), `README.md` (install, retire, restore)

**Interfaces:**
- Consumes: Task 2's note (steps, files, popup shortcuts).
- Produces: `scripts/plasma-panel.sh retire|restore|status`.
  - **`retire`:**
    - backs up `plasma-org.kde.plasma.desktop-appletsrc`, `plasmashellrc`, and `kglobalshortcutsrc`'s `[plasmashell]` into `$XDG_STATE_HOME/ghost-shell/plasma-backup/<UTC yyyymmddThhmmssZ>/`, with a `manifest` naming what was there;
    - then removes the panel and adds the System Tray and Digital Clock to the desktop under the bar's strip, with the old applets' config;
    - assigns the popup route's shortcuts (if Task 2 chose that route);
    - removes bare Meta from `[plasmashell] activate application launcher`, keeping its other keys (ledger R7);
    - frees Meta+1…0;
    - reconfigures KWin;
    - refuses to run twice without a `restore` in between.
  - **`restore`** puts the latest backup back, restarts `plasma-plasmashell.service`, and reconfigures KWin.
  - **`status`** prints `retired <stamp>` or `plasma panel in place`.
  - Each command echoes every change before making it. The script takes `XDG_CONFIG_HOME` and the D-Bus addresses from its environment, so the sandbox test can run it.

- [ ] **Step 1: Write the sandbox test** `spikes/plasma-hidden/test-plasma-panel.sh`. In Task 2's sandbox it runs `retire`, checks the screenshot regions and `status`, runs `restore`, and diffs every backed-up file against the original (expected: identical).
- [ ] **Step 2: Run it and watch it fail** (no script yet).
- [ ] **Step 3: Implement** `plasma-panel.sh`.
- [ ] **Step 4: Run the sandbox test.** Expected: PASS, with the restore byte-identical.
- [ ] **Step 5: Commit**

```bash
git add scripts/plasma-panel.sh scripts/install.sh README.md spikes/plasma-hidden/test-plasma-panel.sh
git commit -m "feat(install): retire and restore the Plasma panel with a full backup"
```

---

### Task 18: Budgets, snapshots and the record

**Files:**
- Modify: `README.md` (Performance: the bar and launcher figures), `scripts/measure-cpu.sh` (bar on by default; `GHOST_SMOKE_BAR` passthrough)

- [ ] **Step 1: Snapshots.** In headless KWins at `QT_SCALE_FACTOR=1.15`, take the 12 combinations of layout × position × icons on V1, plus the launcher open with the bar at the top and at the bottom, plus `brand.enabled = false` and `japanese = "off"`. Check each by eye against the mockups. Text never clips; menus and the launcher open towards the middle.
- [ ] **Step 2: Budget runs** with layout A, top, mono, the installed-style build and the real daemon. Each run is 60 s, three runs, reported as the median:
  - V1 UPLINK and V2 SILICON animating (≤ 2 %);
  - MONITOR (≤ 1.25 %, the user's accepted figure);
  - covered with the bar (≤ 0.5 %);
  - Rss per state (≤ 307 200 KiB).

  Repeat covered for layout B with the ticker scrolling, and record it. **Past O1, stop and bring the numbers to the user.**
- [ ] **Step 3: Write README's Performance section** for 3a: each figure, its conditions, and the per-task deltas the earlier tasks reported.
- [ ] **Step 4: Commit**

```bash
git add README.md scripts/measure-cpu.sh
git commit -m "docs(readme): record the bar's and the launcher's budget figures"
```

---

### Task 19: Install, the Plasma hand-over and acceptance with the user

A controller task: every live step asks the user first.

- [ ] **Step 1: Reinstall** from a clean detached worktree as before. The bar now shows at the top next to the Plasma panel. Tell the user before restarting the services.
- [ ] **Step 2: Ask the user to retire the panel.** On yes, run `scripts/plasma-panel.sh retire`, show its echoed changes, and tell the user that `scripts/plasma-panel.sh restore` undoes it.
- [ ] **Step 3: The user's checklist:**
  - taskbar clicks (single, multi-window list, minimise, middle-click, right-click menu, pin/unpin);
  - Meta+1…0;
  - Meta alone and the brand block;
  - launcher search, recent files and the KRunner fallback;
  - every tray popup, the overflow and the calendar;
  - Meta+V;
  - the volume, brightness and keyboard-layout OSDs;
  - KRunner on Meta+S, Alt+Space and Alt+F2;
  - Lock;
  - whichever other power actions the user chooses to try;
  - a fullscreen video or game covers the bar (design §4.3).
- [ ] **Step 4: Live budgets.** Read the live services' CPU over 60 s and their `smaps_rollup` Rss on V1, and covered.
- [ ] **Step 5: Write `docs/superpowers/notes/<YYYY-MM-DD>-plan3a-acceptance.md`, dated the day it runs.** Record:
  - the checklist results;
  - the user's decisions;
  - the live figures;
  - what 3b inherits: the bell slot in the tray, `N NEW SIGNALS`, and Plasma's notification server still running until 3b replaces it.
- [ ] **Step 6: Commit**

```bash
git add docs/superpowers/notes/<YYYY-MM-DD>-plan3a-acceptance.md
git commit -m "docs: record plan 3a acceptance"
```

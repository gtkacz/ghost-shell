# ghost-shell — Plan 3a Design: the Bar and the Launcher

- **Date:** 2026-10-22
- **Status:** design written after brainstorming; implementation plan not yet written
- **Parent design:** [`2026-10-20-ghost-shell-design.md`](2026-10-20-ghost-shell-design.md). Its bar (§8), launcher (§9), stack (§4), extensibility (§13), configuration (§14) and performance rules (§15) apply here unless this document says otherwise. This document adds what the parent left open and records the decisions made since.
- **Reference mockups:**
  - the bar: [`mockups/2026-10-20-ghost-shell-reference.html`](mockups/2026-10-20-ghost-shell-reference.html), with its `A · ONE BAR / B · BAR + DOCK / C · WINDOW TABS` and `MONO / COLOUR` switches;
  - the launcher: [`mockups/2026-10-22-ghost-shell-launcher-directions.html`](mockups/2026-10-22-ghost-shell-launcher-directions.html). Open it in a browser. All three directions are live: type into one to search this machine's apps. **Direction A is the one to build.**

Items marked **[call]** were decided by the designer (Claude), not the user. The user can override them. Everything else was decided by the user, in the parent design or during this brainstorm.

---

## 1. Intent

Plan 3a **retires the Plasma panel**. After it, the floating bottom Plasma panel is gone, and the ghost-shell bar and launcher do everything the user did with the panel and Kickoff: switch apps, launch apps, reach the tray, see the time, lock and power off.

The user switches apps from the taskbar slightly more than with Alt+Tab, and uses both heavily (parent §8). So the taskbar has to be at least as fast and as predictable as Plasma's. Plasma's habits carry over unchanged (parent §8, "Task behaviour").

**Success looks like this:**
- The user's desktop shows the bar in **layout A, at the top, with mono icons**, matching the reference mockup on the real 2560×1440 screen.
- Nothing the user relied on in the panel or Kickoff is lost (parent §4 list, the parts that live in the panel).
- The Meta key opens the ghost-shell launcher. Meta+S, Alt+Space, Alt+F2 and Search still open KRunner.
- The budgets in §7 hold with the bar on screen.

## 2. Plan 3 is split into three parts

The user's decision (2026-10-22), following the way Plan 2 became 2a/2b/2c:

| Part | Content | Why in this order |
|---|---|---|
| **3a** (this document) | The bar in every layout, position and icon style, the dock (layout B), and **the launcher** | The brand block and the Meta key open the launcher. Shipping the bar without it would leave the Meta key and the brand block dead from the panel's removal until 3c. So the panel and Kickoff are retired together. |
| **3b** | Notifications: the `org.freedesktop.Notifications` server, history, the bell, the drop-down centre, toasts, the dashboard Notifications widget, the INMET and dead-drive notifications, and `N NEW SIGNALS` in the greeting (parent §11) | The bell lives in the bar, so it needs 3a's bar. |
| **3c** | The Alt+Tab switcher (KWin TabBox, parent §10), and the audio items parked by Plan 2b: split `widgets/media/Widget.qml` before adding to it (M9), level meters beyond the default mic (M4), and capture streams that count as "recording" forever (C24) | Independent of 3a and 3b. |

Each part gets its own design check and plan. 3b and 3c are designed after 3a ships.

**Order with Plan 2c (user's decision, 2026-10-22).** Plan 2c (the greeting) is being built on `main` in another session. 3a is designed and planned now, and **built only after 2c's acceptance**, also on `main`. The reasons:
- both would reinstall on the user's live desktop after each task, and would overwrite each other's install;
- both edit ghostd's feed registration, its config, the shell's root, `install.sh` and the smoke scripts;
- parallel CPU measurements would measure each other.

If 2c stalls, a separate worktree for 3a can be reconsidered.

## 3. Goals, non-goals, anti-goals

**Goals:**
- Every bar item in parent §8 except the bell: brand block, tasks, ticker, status, tray, view chip, clock.
- Layouts **A, B (with the dock) and C**, positions **top and bottom**, icon styles **mono and colour**. Every combination is built and switchable in config (parent §8).
- The launcher, direction A of the mockup (§5).
- Retiring the Plasma panel and re-pointing the Meta key, with a backup that uninstall restores (§6).

**Non-goals for 3a:**
- the bell and everything else in notifications (3b);
- the Alt+Tab switcher (3c);
- restyling KDE's tray popups, menus and OSDs (parent §17: later work);
- launcher directions B and C (§5.6: backlog);
- replacing `plasmashell` itself (§6);
- the panel's standalone Media Player applet, which is dropped (§6);
- drag-to-reorder pinned apps, and scroll actions on task icons. The parent design doesn't ask for them; Plasma's defaults leave scrolling off.
- secondary monitors. The bar appears on the main screen only, as with the dashboard (parent §16) **[call]**.

**Anti-goals** (failure even if it works): those of parent §16, plus:
- a window of time in which the Meta key or the brand block does nothing;
- losing clipboard history, Meta+V, the OSDs, or any tray applet the user uses;
- a bar that costs CPU while nothing changes on it;
- a power action that fires from one stray click.

## 4. The bar

Parent §8 and the reference mockup define the look, the items and the A/B/C behaviour. This section adds what they left open.

### 4.1 The user's daily setup

The user will run **layout A, top, mono** day to day (user's decision, 2026-10-22). It gets the most careful acceptance on the real screen. The other layouts, the bottom position and colour icons are accepted on headless snapshots, plus one live switch through each.

### 4.2 The bar as a layout

Parent §13 says the bar is a layout: left, centre and right lists of bar widgets, written exactly as a third party would write them. **[call]**:
- `bar.layout = "a" | "b" | "c"` chooses a built-in **preset**. Each preset is the default `layout.bar` (and, for B, `layout.dock`) that reproduces parent §8.
- A `layout.bar` (or `layout.dock`) written in the user's config replaces the preset's. `bar.layout` still decides whether the dock exists.
- The bar items are widgets with `kinds = ["bar"]` in their manifests: `brand`, `tasks`, `ticker`, `status`, `tray`, `view-chip`, `clock`, plus `dock` for B. The ticker is the flexible spacer (parent §8).
- `tasks` has a style setting, `icons` (layouts A and B) or `tabs` (layout C). In A it sits in the bar, and in B in the dock.
- The status item's parts (CPU, GPU, memory, traffic, VPN, weather) are settings of one widget, so presets can hide GPU and memory in A, and status and VPN in C, as the mockup does.
- The daemon validates bar layouts against the manifests, like the dashboard layouts (parent §13).

### 4.3 Placement and size

- The bar is a layer-shell surface on the **Top** layer, on the main screen, and reserves its height (an exclusive zone), so maximised windows stop at it. The dashboard already leaves a 30 px band for it (Plan 1), and must not reserve it twice.
- Height: 30 px at mockup scale, through the global scale constant (parent §5: about 52 logical px on this screen).
- `bar.position = "bottom"` moves it to the bottom edge. In B, the dock then moves to the top, as parent §8 says, and tooltips, menus and the launcher open towards the middle of the screen.
- **Fullscreen windows cover the bar** **[call]**. KWin raises an active fullscreen window above the Top layer, so games and video stay clean.

### 4.4 Tasks

The behaviour is parent §8's list, which follows Plasma's defaults:
- click focuses, or launches if not running;
- clicking the focused app minimises it;
- middle-click opens a new instance;
- hover lists the window titles, and clicking a title focuses that window;
- Meta+1…0 activate the first ten pinned apps;
- unread badges and progress from apps;
- right-click opens a menu with the app's jump-list actions, pin/unpin, and close.

What the parent left open:
- **An app with several windows** (user's decision, 2026-10-22):
  - A click opens its list of window titles, the same list hovering shows, whether or not the app is focused. Clicking a title focuses that window.
  - This replaces the user's Plasma setting, which shows small window previews on click (`groupedTaskVisualization=1`). Live previews aren't possible from the bar: KWin lets no ordinary client capture other windows.
  - A single-window app focuses on click, and minimises when clicked while focused.
- **The window list comes from KWin through the existing KWin script** **[call]**:
  - KWin offers ordinary Wayland clients no window list. It has no `wlr-foreign-toplevel`, so Quickshell's `ToplevelManager` is empty on it, and `org_kde_plasma_window_management` is reserved for privileged clients.
  - The `ghost-shell-desktop` script already watches every window for the covered state. It is extended to report the window list: id, caption, app id (`desktopFileName`, falling back to `resourceClass`), pid, active, minimised, fullscreen, and skip-taskbar.
  - It reports on change only, never on a timer, through ghostd as a new topic.
- **Commands go back to the script** (activate, minimise, close one window) through a channel the builder chooses. The spike (§8) measures click-to-focus latency against a target of **≤ 50 ms** **[call]**.
- **Apps and windows are matched** by desktop-file id, then `StartupWMClass`, then the lower-cased resource class. A window that matches no `.desktop` file shows as its own task with a generic icon and its resource class as the name **[call]**.
- **Icons** come from the app's `.desktop` file through the user's icon theme (kora today). Mono is the parent's silhouette: alpha × luminance, tinted in the signal colour, and `hi` for the focused app. It is drawn with the existing duotone shader approach, not per-pixel JavaScript.
- **Badges and progress** come from the `com.canonical.Unity.LauncherEntry` D-Bus signals (count, progress, urgent), which ghostd listens to and publishes per desktop-file id **[call]**. A count is drawn in the signal colour, and progress as a segmented mark (parent §8).
- **Meta+1…0** are registered through ghostd's existing KGlobalAccel shortcuts. They're configurable like the other shortcuts. Plasma's task manager owned them until its panel was removed (§6).
- **Hover title lists, the right-click menu and tooltips are ghost-shell's own**, in the panel style **[call]**. Only KDE's applet popups and OSDs stay stock (§4.6).
- **Pin/unpin writes `bar.pinned` in `config.toml`** and keeps the user's comments and formatting **[call]**. The config file is the only settings store (parent §14).
- **First run imports the pinned apps** from the Plasma panel's task manager (`launchers=` in `plasma-org.kde.plasma.desktop-appletsrc`) into `bar.pinned` when the list is empty (parent §13). It runs before §6 removes the panel.

### 4.5 Ticker and status

- The ticker shows the mockup's line: failed systemd units, firewalld state, kernel, NVIDIA driver, the main IPs, VPN state, any critical item, and pending updates.
- **The ticker only scrolls in layout B.** In A and C it is the empty spacer, as in the mockup (`.bar-a .tk, .bar-c .tk { visibility: hidden }`). In A and C its data isn't even subscribed.
- In B it scrolls at the mockup's pace (one loop in 60 s), in held steps at the parent's capped 15 fps for decorative motion, and it stops while covered by a fullscreen window.
- **New ghostd data for it** **[call]**, read over D-Bus, event-driven, with no forks:
  - failed units from systemd's manager, refreshed on its unit signals;
  - firewalld's state from `org.fedoraproject.FirewallD1`;
  - pending updates from PackageKit. Its cached update list is read at start and whenever PackageKit signals `UpdatesChanged`; ghostd never asks it to refresh;
  - kernel from `uname`, and the NVIDIA driver from NVML (already open);
  - IPs and VPN from the existing `net.links` topic;
  - critical items from the existing alert sources.
- **Status** shows CPU %, GPU %, memory, up/down traffic (chevrons), the VPN count, and the weather summary in the warn colour, never animated (parent §8, §5.4). It reuses the existing topics through `Fmt`.
- **Covered dashboard:** only the bar's own topics keep sampling, through the `barRequires` input that Plan 2a left for this (`subscriptionFor`). Values repaint only when they change, and the clock's seconds once a second.

### 4.6 Tray

- **Built-in items** (Bluetooth, network, volume, KDE Connect, clipboard) are icons drawn by ghost-shell, never Unicode glyphs (parent §5). Their state comes from:
  - Quickshell's Bluetooth and PipeWire services;
  - ghostd's `net.links` topic;
  - the existing KDE Connect battery.
- **StatusNotifierItem apps** (Steam, Discord, Tixati and so on) come from Quickshell's SystemTray service, and follow the mono/colour setting. Right-click shows the app's own menu (DBusMenu). Activation is left-click, as in Plasma.
- **Plasma's habits** **[call]**: scrolling over volume changes it, and middle-click mutes.
- **Clicking a built-in item opens KDE's own popup for that applet** (network, Bluetooth, audio, KDE Connect, clipboard). The popups stay KDE-stock in project 1 (parent §4, §17). §6 says how, pending the §8 spike.
- **An overflow chevron at the tray's end opens KDE's full tray popup** (user's decision, 2026-10-22): the same expanded list Plasma's tray shows. Today the user's tray shows more applets than the five built-in icons: keyboard layout, Disks & Devices (USB eject), Vaults, printers, Brightness & Night Light, display configuration, camera indicator, input method, battery and weather. The chevron keeps every one of them reachable. None gets its own bar icon in 3a.
- The bell is not part of 3a.

### 4.7 View chip and clock

As parent §6.1 and §8:
- the chip reads `MAIN`/`MONITOR` from `Views.isMonitor`, and clicking it calls `Views.toggleMonitor()`;
- the chevron button calls `Views.swap()`;
- the clock is `HH:MM` with superscript seconds and the date as `DD.MM.YY`;
- **clicking the clock opens KDE's calendar popup** (user's decision, 2026-10-22), with the user's week numbers and holiday and PIM-event plugins. It works like the tray popups (§6).

## 5. The launcher

Direction A of the launcher mockup (user's decision, 2026-10-22).

### 5.1 Opening and closing

- It opens with **Meta on its own** or a click on the brand block, and closes on the same, on Esc, on a launch, or on a click outside it.
- **Placement:** it drops from the brand block, under the bar, at the screen's left edge. With the bar at the bottom it rises from the brand block instead. With the brand block turned off, it opens from the bar's left end (parent §8).
- It opens **over windows**: it appears whether or not the desktop is visible. While open, it takes keyboard focus exclusively.
- **Opening is a stepped print**, as in the mockup **[call]**: 10 held steps of about 18 ms, top to bottom (bottom to top for a bottom bar), with the 2 px light scan line at the edge. There is no fade (parent §16) and no closing animation.
- Its header text scrambles on each opening, which counts as a new showing (Plan 2b's rule). Results do **not** scramble while you type.

### 5.2 Layout

At mockup scale: a 392 × 560 panel in the panel style, with the chamfer, Grid brackets, and fill at about 94 % so the window under it doesn't show through.
1. **Header:** `LAUNCH 起動`, and on the right the number of apps.
2. **Search field:** a prompt chevron, the query, the boot-style block cursor, the dim placeholder `SEARCH APPS`, and an `ESC` chip.
3. **The list** stretches to fill the space (parent §5.4's one stretching element). With the field empty it holds, in order:
   - `PINNED 固定`: the `bar.pinned` apps in a 6-column icon grid, each icon with its short name below;
   - `RECENT 最近`: the five most recent files, each with an icon, file name, dim folder and age (`2H`, `1D`; Plan 2a's `ago()` buckets);
   - `ALL APPS 全部`: every app, alphabetical, each with an icon, name, dim generic name, and a category chip.

   As soon as you type, the list becomes `RESULTS 検索 n`: apps only, ranked, with the matched part of the name in `hi`.
4. **Power row:** `LOCK`, `LOG OUT`, `SLEEP`, `RESTART`, `SHUT DOWN`, each with a drawn icon above its label.

The Japanese tags follow the `japanese` setting (parent §14), and `off` hides them.

### 5.3 Search and keys

- **Search** covers installed apps by name, generic name and keywords (parent §9).
  - Ranking **[call]**: name prefix, then any word of the name, then a word of the generic name or keywords, then a substring of the name. Ties sort alphabetically.
  - Recent files and pinned apps are not searched separately: pinned apps are in the app list anyway.
- **Keys:**
  - typing goes straight to the field;
  - Up/Down move through the list. In the pinned grid they move by a row, and Left/Right move by an icon;
  - Enter launches the selection, or opens it if it's a recent file;
  - Esc closes.
- **No match:** the list reads `NO MATCH · ENTER RUNS "<query>" IN KRUNNER`, and Enter hands the query to KRunner (`org.kde.krunner`'s `query` method) **[call]**. Typing into a dead end never loses the text.

### 5.4 Data

- **Apps** come from the installed `.desktop` files (system, user, Flatpak exports), through Quickshell's `DesktopEntries`. Entries with `NoDisplay`, `Hidden`, or an `OnlyShowIn`/`NotShowIn` that excludes KDE are dropped.
- **Duplicates are merged** **[call]**. This machine has two `Dolphin`, `Steam`, `Firefox`, `Konsole` and `Obsidian` entries. The user's own entry wins over the system one, then the native over the Flatpak one.
- **The category chip** maps freedesktop main categories to `NETWORK`, `DEV`, `GAME`, `MEDIA`, `GRAPHICS`, `OFFICE`, `SYSTEM`, `SETTINGS` and `UTILITY` **[call]**.
- **Recent files** are the same list Kickoff shows today. The spike (§8) confirms which store that is on this machine: KActivities' statistics or `recently-used.xbel`.
- **Launching** goes through the `.desktop` entry, including its `Exec` field codes and D-Bus activation. A launched app counts as started by the user, so KWin's focus-stealing prevention gives it focus (`xdg_activation_v1`, which KWin offers).

### 5.5 Power and session actions

- **Lock** acts at once, through `org.freedesktop.ScreenSaver.Lock`.
- **The other four arm on the first press and act on a second press within 3 s** **[call]**. While armed, the button is in the warn colour and reads `AGAIN`. After 3 s it disarms. One stray click can never log out or power off.
- Log out, restart and shut down go through KDE's session manager (`org.kde.Shutdown`), so apps get the usual chance to save their state, without KDE's own confirmation dialog. The arming step replaces it.
- Sleep goes through logind's `Suspend`.

### 5.6 Backlog: other launcher styles

The user wants directions B (two panes, Kickoff-like) and C (console, centred over a dimmed backdrop) kept as alternatives, selected in config (user's decision, 2026-10-22):
- 3a adds the setting `launcher.style`, with only `drop` built;
- `panes` and `console` are recorded in `docs/IDEAS.md` with a pointer to the mockup;
- 3a must not hard-code the drop panel in a way that makes another style a rewrite. Search, data, keys and actions are shared code, and the style is only the view **[call]**.

## 6. Plasma after 3a

User's decision (2026-10-22): **`plasmashell` keeps running in project 1, but invisible.** Plasma 6 has no standalone home for several functions the user relies on (parent §4):
- clipboard history and its Meta+V popup;
- the volume, brightness, media and keyboard-layout OSDs;
- today's notification server, until 3b;
- the tray applets: network, Bluetooth, audio, keyboard layout, Disks & Devices, Brightness & Night Light, camera indicator, device batteries, printers, Vaults, input method, display configuration.

So 3a hides Plasma rather than replacing it. Replacing those functions belongs with the tray restyle in parent §17. `plasmashell` stays at about 650 MB of memory, as today. It is outside the §7 budget, which counts ghost-shell's own processes.

**[call] The plan, pending the §8 spike:**
- **The panel goes.** It holds Kickoff, the task manager, the pager, a Media Player applet, the system tray and the digital clock.
- **The standalone Media Player applet is dropped** (user's decision, 2026-10-22). The dashboard's media panel and the media keys cover it. The tray's own hidden media controller is unaffected.
- **Plasma's System Tray and Digital Clock widgets move onto Plasma's desktop**, under the strip that ghost-shell's bar covers, the tray near ghost-shell's tray and the clock near its clock. The bar takes every click in its strip, so no click reaches the hidden widgets. The tray's applets keep running, including the clipboard and notifications.
- **Clicking one of ghost-shell's built-in tray icons, the overflow chevron or the clock opens the matching KDE popup**, for example by invoking the widget's activation shortcut through KGlobalAccel. KDE positions the popup next to the hidden widget, which is next to the ghost-shell item that was clicked.
- **If the spike shows the popups can't be opened or placed that way**, the fallback is each applet in its own small window (`plasmawindowed <applet>`), placed by a KWin rule. If neither works, stop and ask the user.
- **The Meta key** is re-pointed in `kwinrc` (`[ModifierOnlyShortcuts] Meta=`) from `org.kde.plasmashell … activateLauncherMenu` to a ghostd D-Bus method, `org.ghostshell.Desktop1.ToggleLauncher` **[call]**. ghostd relays it to the shell. Meta+S and the other KRunner bindings are not touched.
- **Meta+1…0 are freed from `plasmashell`** **[call]**. On this machine they are bound under `kglobalshortcutsrc`'s `[plasmashell]` component as `activate task manager entry N`. That registration is plasmashell's, not the panel's, so removing the panel may not free them. Two holders of one key means only one fires (`warn_if_shared` in `feeds/shortcuts.rs`). The spike (§8) checks whether removing the panel frees them; if not, the install step unbinds them.
- **Backup and restore.** The install step saves Plasma's applet configuration, the `kwinrc` key and the `[plasmashell]` task-manager entries of `kglobalshortcutsrc` before changing them. Uninstall puts all three back, so today's panel returns as it was.
- **When it happens.** Removing the panel, moving the tray, re-pointing Meta and freeing Meta+1…0 touch the user's live Plasma setup. They are done **once, at 3a's acceptance, after asking the user.** Until then, the per-task reinstalls on the live desktop add the ghost-shell bar at the top while the Plasma panel stays at the bottom.

## 7. Performance

The parent's §15 budgets apply. **The bar and the launcher count inside them**, as part of the shell process.
- **Where the budget stands** (Plan 2b acceptance, headless):
  - V2 SILICON measured 1.87 % of 2 % CPU;
  - MONITOR used 251 508 of 307 200 KiB;
  - the live service read 289 880 KiB at one point.

  The bar has little room.
- **Covered** (≤ 0.5 %, 0 dashboard frames): the bar keeps repainting while windows cover the dashboard, and ghostd keeps sampling its topics. In layout A that means the clock's seconds, CPU, GPU, memory and traffic, at the existing 1 Hz. That costs a little where the dashboard previously cost 0.04 %, and must be measured.
- **The launcher costs nothing while closed** **[call]**: no surface, no timers, and the app list is built when it opens or when the `.desktop` files change, not polled.
- **Every task that adds bar items or collectors** reports its CPU and Rss deltas, as in Plan 2b. **A budget check that goes over stops the task**, and the numbers go to the user. Nothing is slowed down or re-budgeted unilaterally. The 2b margin rule (+0.25 % CPU and +10 MiB Rss accepted and flagged) carries over.

## 8. The spike that comes first **[call]**

One spike task, before any bar code, in a headless KWin with a private bus, with nothing on the user's display (the standing rule for agents). It answers:
1. **Window list and commands:**
   - can the KWin script report the window list (fields of §4.4) and act on commands?
   - what is the click-to-focus latency (target ≤ 50 ms)?
   - does focusing one window of an app with several, minimising, and closing behave as Plasma's task manager does?
2. **KDE popups** with the panel gone and the System Tray and Digital Clock widgets on the desktop:
   - do the applets keep running (clipboard history and Meta+V, notifications)?
   - can each built-in applet's popup, the full tray popup and the calendar be opened from outside, and where do they appear?

   If not, try `plasmawindowed`. Report either way, with screenshots from the headless session.
3. **Meta on its own:** does the `kwinrc` modifier-only binding reach a ghostd D-Bus method, and does Meta+S still reach KRunner? With the panel gone, are Meta+1…0 still held by `plasmashell`?
4. **Recent files:** which store holds the list Kickoff shows today?

Any "no" on 1–3 without a working fallback stops the plan, and the findings go to the user.

## 9. Acceptance

- **Headless:** snapshots of every bar layout × position × icon style, with the launcher open in each position. Budget runs for V1, V2, MONITOR and covered, with layout A.
- **On the user's desktop, after asking:**
  1. remove the panel, move the tray, re-point Meta, free Meta+1…0 (§6);
  2. the user tries the taskbar, launcher, tray popups, Meta+1…0, Meta+V, the OSDs, KRunner, and the power actions they're willing to run;
  3. the live budgets are read;
  4. check the uninstall restore without running it on the live desktop: run it against a copy of the configs and compare the restored files.
- Measurements go in README.md's Performance section, as with every earlier plan.

## 10. Left to the builder

- The command channel from ghostd to the KWin script, and the topic shapes (window list, launcher entries, ticker).
- How the bar's surfaces are split (one bar surface, separate popups, the dock's own surface) within §7.
- How Quickshell's `DesktopEntries` and the icon lookup are used, as long as closed costs nothing (§7).
- The exact keyboard-focus handling of the launcher and menus, within §5.1.
- Test strategy, following the existing node tests, Rust tests, smokes and snapshots.

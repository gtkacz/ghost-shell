# Plasma hidden spike: Plasma without its panel, its popups opened from outside

Date: 2026-10-23. Plan 3a Task 2, answering design §8 questions 2–4 (popups, applets alive, Meta+1…0, recent
files). It prototypes `scripts/plasma-panel.sh` (Task 17) and the call behind `plasma.popup` (Task 7).

## Answers

| Question | Answer |
|---|---|
| Does Plasma keep working with its panel gone? | **Yes.** Every tray applet, the clock, notifications and clipboard history kept working, and `org.freedesktop.Notifications` stayed with plasmashell |
| Can the seven popups be opened from outside? | **Yes, all seven, by the brief's first route:** a `[Shortcuts] global=` key on each widget, then `invokeShortcut("activate widget <id>")` on `/component/plasmashell`. A second call closes the popup. The clipboard's opens only while its history holds something |
| Where do they open? | **Every tray popup overlaps the bar's strip and is stacked above it; the calendar opens 12 px below it.** See "Placement" |
| Who holds Meta and Meta+1…9 once the panel is gone? | **Still plasmashell**, until the retire step changes their keys through KGlobalAccel's D-Bus API. Then a stand-in ghost-shell got every press, before and after a plasmashell restart. **Meta+0 belongs to KWin** (`view_actual_size`) |
| plasmashell's memory without the panel | **About 25 MB less Rss (−6 %)**: 389–414 MB retired against 413–430 MB with the panel, over warm restarts; −9 to −31 MB across three runs |
| Do `recently-used.xbel` and kactivitymanagerd agree on recent files? | **Yes: 5 of the first 5**, in the same order, and 10 of the first 10 |

## Setup

- Fedora 44, plasmashell 6.7.5, KWin 6.7.5. `spikes/plasma-hidden/sandbox.sh up` builds the sandbox under
  `/tmp/gs-sbx-3a-t2`; `run.sh` drives the whole spike through it and `sandbox.sh down` takes it apart.
- **Compositor:** a headless KWin (`--virtual --no-lockscreen`), one output, `--width 2226 --height 1252 --scale
  1.15`: the user's 2560x1440 screen at their 1.15. kded's kscreen module, finding no kscreen config for the
  virtual output, sets its scale back to 1, so `up` sets it again with `kscreen-doctor` before plasmashell starts.
  No `QT_SCALE_FACTOR` anywhere: the output is already at 1.15.
- **Buses:** a private session bus (`scripts/private-bus.conf` plus one `<servicedir>`) and a second private bus,
  without service dirs, as `DBUS_SYSTEM_BUS_ADDRESS`. The session bus's service dir is the sandbox's own, holding
  symlinks to 8 of `/usr/share/dbus-1/services`' files: `org.kde.kded6`, `org.kde.ActivityManager`,
  `org.kde.plasma.Notifications`, `org.kde.KScreen`, `org.kde.kiod6`, `org.kde.kpasswdserver6`, `org.kde.kssld6`,
  `ca.desrt.dconf`. **That departs from the brief's whole directory, on purpose:** it also holds kdeconnectd (which
  would announce itself on the user's LAN), Akonadi, the portals and the logout and shutdown services. The user's
  `~/.local/share/dbus-1` was never used. Activated services got the sandbox's Wayland socket through
  `UpdateActivationEnvironment`. kded6, kactivitymanagerd and dconf activated there; kglobalaccel runs inside KWin.
- **plasmashell:** `plasmashell --no-respawn` in an `env -i` environment with `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
  `XDG_CACHE_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR` and `HOME` all in the sandbox.
- **What was copied** (with `cp`, originals only read): 27 files from `~/.config` (the Plasma, KWin, kded,
  kactivitymanagerd, klipper, notification, locale, input and power configs, and `kconf_updaterc`) with its
  `kdedefaults/` directory, and `~/.local/share/plasma` and `~/.local/share/kactivitymanagerd`. Not copied: clipboard
  history, KDE Connect's config, icons, fonts and colour schemes, so the sandbox shows the stock icons and a fallback
  for the clock's font.
- **Things the sandbox needed:**
  - **`kconf_updaterc` must come along.** Without it kconf_update reran every update on the copies, and
    `migrate-calendar-to-plugin-id.py`, run on an already migrated clock, stored its plugin list as single
    characters (`enabledCalendarPlugins=h,o,l,i,d,…`). With it, nothing changed at start.
  - **An `org.bluez` stand-in** on the private system bus (`plasma_dbus.py own`): the tray loads its Bluetooth applet
    only while that name exists. NetworkManager, PulseAudio and powerdevil were absent, so those applets show
    "not running" pages, and battery, brightness and the media player don't load. The question was whether popups
    open, not what they show.
  - The clock's `pimevents` calendar plugin started a sandboxed Akonadi (its own MySQL under the sandbox's data dir,
    no accounts). `down` stopped it with everything else.
- **Cleanup:** `down` stops every process whose environment carries the sandbox's `XDG_RUNTIME_DIR`, then KWin, the
  session and the system bus, checks nothing of it is left, and removes the sandbox. The user's plasmashell, KWin,
  kded6 and kactivitymanagerd (pids taken before the first run) were never signalled.
- Helpers: `plasma_dbus.py` (evaluateScript, invokeShortcut, KGlobalAccel's `setForeignShortcutKeys` and
  `unregister`, owners, notifications), `windows.js` (a KWin script logging every window's logical frame geometry),
  `strip.qml` (a red 52 px `dock` layer-shell strip on the Top layer, standing in for the bar), `recent.py`, and
  spike 1's `meta_kglobalaccel.py`, `fake_chord.py` and the 2c spike's `grab.py`. Every Python helper refuses an
  empty, non-`unix:` or `/run/user/` bus address before connecting and runs with the user's bus variables unset.

## Step 1: the panel in the sandbox

- The copied layout came up as on the user's desktop: one floating panel at the bottom, 66 logical px tall
  (window at 0,1186, 2226x66), with Kickoff, the pinned launchers, the tray and the clock with seconds and the
  long date. The pager (one desktop) and the media controller (no player) stay hidden, as they would.
- 11 plasmashell actions held the keys in question: `activate application launcher` (Meta and Alt+F1) and
  `activate task manager entry 1…9` (Meta+1…9). `entry 10` has no key; Meta+0 is KWin's `view_actual_size`.

## Step 2: retire

### Scripting calls (`retire.js`, through `org.kde.PlasmaShell.evaluateScript`)

Work:
- `panels()`, `panel.widgets(plugin)`, `panel.remove()`;
- `desktops()` filtered on `d.screen === 0`, `screenGeometry(0)` (logical: 2226x1252), `d.addWidget(plugin, x, y, w,
  h)`;
- `w.currentConfigGroup = [group]`, `w.configKeys`, `w.readConfig(key)`, `w.writeConfig(key, value)`,
  `w.reloadConfig()`;
- `w.globalShortcut = "…"` on a top-level widget, which registers `activate widget <id>` at once.

Don't, and what the prototype does instead:
- **`readConfig` hands a list back as one comma-joined string, and `writeConfig` of a string into a list key stores
  its characters.** The prototype splits the 7 list keys it knows (`extraItems`, `hiddenItems`, `shownItems`,
  `knownItems`, `disabledStatusNotifiers`, `enabledCalendarPlugins`, `selectedTimeZones`).
- **A walk over `w.configGroups` that changes `currentConfigGroup` visits only the first group**, so each group is
  named: the tray's `General`, the clock's `Appearance`.
- **The tray's own applets are out of reach** (`desktopById(<tray id>)` is null), so their shortcuts go into the
  layout file with `kwriteconfig6` while plasmashell is stopped.
- `w.geometry` can't be assigned, and a desktop containment has no `location` to set.

Copied: the tray's `disabledStatusNotifiers`, `extraItems`, `hiddenItems`, `knownItems` (the user's has no
`shownItems`), and all 12 `Appearance` keys of the clock, calendar plugins (`holidaysevents,pimevents`) and week
numbers included. The calendar popup showed both: holidays in the grid, week numbers 40–45.

### Where the widgets land

Asked for: tray at 1506,0 480x52 and clock at 1986,0 240x52, logical. The folder view placed **the tray at
1520,14, 452x36, and the clock at 1986,0, 240x64**. The capture shows a 452 px dark box holding the keyboard
layout's "us" (the only status item active in the sandbox) and, to its right, the large clock with the date under
it, both at the top right with nothing else on the desktop. **With the strip on top, 6 rows of physical px (52–57
logical) still show:** the tray's frame and the bottom of the clock's date. The clock never got below 64 px tall.

### What kept working

- `org.freedesktop.Notifications` was owned by plasmashell after the retire (its pid). A notification sent
  through it showed as a toast at **1858,36, 332x38**, so its top 16 px sit under the strip. With a tray popup open
  it moved to 1858,476, below the popup.
- Clipboard: two `wl-copy`s against the sandbox gave 2 history entries, and Meta+V (`fake_chord.py`) opened
  "Clipboard Popup" at the pointer (1113,626, 564x633) showing both.
- Every tray applet answered its popup (step 3); the clock and calendar too.

## Step 3: the popups

Route 1 for all seven. `plasma-panel.sh retire` gives each widget a key (Meta+Ctrl+Alt+Shift+F1…F7, which nothing
in the user's shortcuts holds) in `[…][Applets][<id>][Shortcuts] global=`, and `popup.sh <name>` calls
`invokeShortcut("activate widget <id>")` on `/component/plasmashell`. The key itself is never pressed: Plasma
registers the action only for a widget that has one.

| Popup | Opens | Window (logical) | Closes on a second call |
|---|---|---|---|
| Network (`org.kde.plasma.networkmanagement`) | yes, "NetworkManager service is not running" | 1524,0 440x440 | yes |
| Bluetooth | yes, "Bluetooth is disabled" (the stand-in has no adapters) | 1524,0 440x440 | yes |
| Volume | yes, "Connection to the Sound Service Lost" | 1524,0 440x440 | yes |
| KDE Connect | yes, "No paired devices" | 1524,0 440x440 | yes |
| Clipboard | yes, the history, **only while it holds something** | 1524,0 440x440 | yes |
| Full tray | yes, "Status and Notifications" with the hidden items | 1524,0 440x440 | yes |
| Calendar | yes, events, holidays, week numbers | 1408,64 818x458 | yes |

- All five applets open inside the tray's one expanded window, on their own page. With the tray already open,
  invoking an applet switches the page.
- **Clipboard with an empty history opens nothing:** the applet sets itself hidden inside the tray, and the action
  is ignored. In the exploratory runs one more call, right after a plasmashell restart with one fresh entry, also
  opened nothing; the next ones worked. Two fallbacks work: `invokeShortcut("show-on-mouse-pos")`, klipper's own
  popup at the pointer, and **route 3, `plasmawindowed org.kde.plasma.clipboard`**, a separate decorated window
  "Clipboard" (863,357, 500x537) with the same history.
- **Route 2 is route 1:** an `activate widget <id>` action exists only for a widget that has a shortcut. The user's
  config had one, for the task manager, with no key.

### Placement

- **The tray's popup always opens at the top of the screen, at the tray's x + 6**, whatever the tray's y: a second
  tray added at 814,414 opened its popup at 820,0. So it covers 1524–1964 × 0–52 of the bar's strip.
- **It is stacked above the strip.** With `strip.qml` up (`dock` namespace, Top layer), the popup drew over it,
  tinted red through its translucent background. An exclusive zone on the strip moved neither the widgets nor
  either popup.
- **The calendar opens below the clock**, at 1408,64, 818x458: 12 px under the strip, right-aligned to the screen.
- Neither popup is anchored to anything the bar draws, so Task 7 can't line them up with bar items without moving
  the widgets themselves.

## Step 4: Meta, Meta+1…0, and memory

### Meta and Meta+1…9

In the exploratory runs, with the real sandboxed plasmashell and the panel (with Kickoff and the task manager) gone:

- **Removing the panel changes nothing in KGlobalAccel.** `globalShortcutsByKey` still listed plasmashell's
  `activate application launcher` for bare Meta (16777250) and `activate task manager entry N` for Meta+N. A
  stand-in ghost-shell registering Meta and Meta+1 (as ghostd does) got 0 of 3 presses each; plasmashell got 3 of 3.
- **Bare Meta:** `setForeignShortcutKeys(["plasmashell", "activate application launcher"], [[Alt+F1]])` on the
  private bus took Meta off the action at once, with no restart. Then Meta reached ghost-shell 3 of 3 and Alt+F1
  still opened Plasma's launcher action.
- **Meta+1, unregistered:** `unregister("plasmashell", "activate task manager entry 1")` worked until plasmashell
  restarted. **plasmashell registers `activate task manager entry 1…10` on every start, task manager or not**, so
  the action came back with its default Meta+1. Who then gets the key depends on registration order: with
  ghost-shell's action registered first it kept the presses (3 of 3), but re-registered after plasmashell's it got
  none (plasmashell 3 of 3).
- **Meta+1, cleared:** `setForeignShortcutKeys(["plasmashell", "activate task manager entry N"], [])` leaves the
  action registered with no key and writes `activate task manager entry 1=none,Meta+1,…` to `kglobalshortcutsrc`.
  plasmashell re-registers with `SET_PRESENT` and keeps the stored "none". **This is the method that survives a
  plasmashell restart**, and the one the prototype uses.
- **Editing `kglobalshortcutsrc` alone doesn't work while KWin runs.** KGlobalAccel keeps its registry in memory and
  rewrites the file from it: an edited `activate application launcher=Alt+F1,…` was back to `Meta\tAlt+F1` after a
  plasmashell restart, with Meta still held. The file route would need KWin restarted, a new login on the desktop.

The final run (`run.sh`), retire by `plasma-panel.sh`:

| | bare Meta | Meta+1…9 | Meta+0 | Alt+F1 |
|---|---|---|---|---|
| Holders after the retire | none | none | `kwin/view_actual_size` | – |
| Presses, ghost-shell registered after the retire | **ghost-shell 3/3** | **ghost-shell 1/1 each** | KWin 1/1 | plasmashell 1/1 |
| Holders after a plasmashell restart | ghost-shell only | ghost-shell only | `kwin/view_actual_size` | – |
| Presses after the restart | **ghost-shell 3/3** | **ghost-shell 1/1 each** | KWin 1/1 | plasmashell 1/1 |

So ghost-shell can take Meta+1…9 as well as Meta, once the retire step clears plasmashell's keys. Meta+0 is KWin's
"Zoom to actual size" in the user's shortcuts; taking it means changing a KWin shortcut, which this spike didn't.

### Memory

`smaps_rollup` of plasmashell, 60 s after it started, three samples 10 s apart (they agreed within 1 MB, so one
figure per start below). Every figure but "cold" is a restart in a sandbox that had already run plasmashell.

| Run | With the panel: Rss / Pss | Retired: Rss / Pss | Rss delta | Pss delta |
|---|---|---|---|---|
| Exploratory | 429.7 / 223.0 MB, 422.7 / 216.2 MB | 398.4 / 195.1 MB | −24 to −31 MB | −21 to −28 MB |
| `run.sh` | 422.7 / 215.1 MB (restored) | 413.7 / 207.9 MB | −9 MB | −7 MB |
| Repeat loop (`sandbox.sh` restarts) | 413.4 / 211.8 MB, 416.9 / 215.0 MB | 388.5 / 190.8, 389.0 / 191.5, 389.3 / 191.3 MB | −24 to −28 MB | −20 to −24 MB |
| Cold: the first starts of a fresh sandbox | 475.9–476.9 / 269.5–269.7 MB | – | – | – |

- **Removing the panel saves about 25 MB of Rss**, out of some 415 MB: Kickoff, the task manager and the panel's
  own view go, while the tray and clock move to the desktop. One run of three saved only 9 MB, for no reason the
  spike found.
- Measured in the same process right after the retire, Rss **grew** (a spot check: 472 → 522 MB), since the freed
  memory isn't returned and the new widgets load on top. The saving shows only after a plasmashell restart, which
  the retire does anyway (step 3).
- The first starts in a fresh sandbox are left out: about 60 MB higher every time (`up`'s own start, and in the
  loop also the restart right after it), most likely caches built on first use.

## Step 5: restore

`plasma-panel.sh restore` stopped plasmashell, put the three files back, unregistered the seven `activate widget
<id>` actions, gave the 11 actions their backed-up keys through `setForeignShortcutKeys`, and started plasmashell.

- **The panel came back as it was:** the capture after the restore differs from the one before the retire only in
  the clock's digits (a 312x34 physical px box).
- KGlobalAccel again listed plasmashell's launcher with Meta and Alt+F1 and `entry 1…9` with Meta+1…9.
- `plasmashellrc` and `kglobalshortcutsrc` were identical to the backup; the layout file differed in 1 line, the
  folder view's `screenMapping` written in another order.
- **The restore must undo the popups' shortcuts through KGlobalAccel too.** Putting `kglobalshortcutsrc` back isn't
  enough, since KGlobalAccel rewrites it from memory: before the prototype unregistered them, the 7 `activate widget
  <id>` entries survived the restore.

## Retire and restore, as Task 17 should do them

Files to back up, once, before the first retire (from `~/.config`):
- `plasma-org.kde.plasma.desktop-appletsrc` (the layout);
- `plasmashellrc` (panel geometry; the retire didn't change it, but it belongs to the panel);
- `kglobalshortcutsrc`;
- and, since KGlobalAccel's memory is the truth, **the keys of the 11 actions** as `allShortcutInfos` on
  `/component/plasmashell` reports them (`launcher`, `entry 1…10`).

Retire (`plasma-panel.sh retire`):
1. Back up the above; refuse if a backup is already in place.
2. `evaluateScript(retire.js)`: add `org.kde.plasma.systemtray` and `org.kde.plasma.digitalclock` to screen 0's
   desktop at its top right, copy the panel tray's `General` and the panel clock's `Appearance` (splitting list
   keys), remove every panel.
3. Stop plasmashell; give the seven popups' widgets a `[Shortcuts] global=` key in the layout file; start it.
   (On the desktop that is one visible plasmashell restart.)
4. `setForeignShortcutKeys`: the launcher action keeps its keys minus bare Meta; `entry 1…10` get no keys.
5. Write the popup name → widget id map for `plasma.popup` (`<backup>/popups`).

Restore (`plasma-panel.sh restore`):
1. Stop plasmashell (it writes its layout on the way out, so the files must go back after).
2. Put the three files back.
3. `unregister` the seven `activate widget <id>` actions; `setForeignShortcutKeys` each of the 11 actions to its
   backed-up keys.
4. Start plasmashell.

On the desktop the stop and start would be `systemctl --user stop|start plasma-plasmashell.service`; the spike
passes its own. The prototype refuses any bus but a private one; Task 17 drops that guard.

## Step 6: recent files

Read-only. `recent.py` took the 10 newest `file://` bookmarks of `~/.local/share/recently-used.xbel` (by
`modified`) and the 10 newest file resources (`ResourceScoreCache`, by `lastUpdate`) from a copy of
`~/.local/share/kactivitymanagerd/resources/database` with its `-wal` and `-shm`, opened read-only. The copy was
deleted afterwards.

- **The first 5 agree, 5 of 5, in the same order; all 10 appear in both lists.**
- Extensions, both lists, newest first: 9 × `.html`, then 1 × `.md`.

So Task 9 can read either; they told the same story here. kactivitymanagerd's database is SQLite in WAL mode and
the user's kactivitymanagerd holds it open, so a reader must open it read-only (or copy it with its `-wal`).

## Limits

- One headless output, no GPU timing, a virtual keyboard. The copied configs were the user's, but icons, fonts and
  colour schemes weren't copied, NetworkManager, PulseAudio, BlueZ and powerdevil were absent or stood in for, so the
  popups showed their "not running" pages. Their sizes may differ with real content (the tray's window is a fixed
  440x440 here).
- `strip.qml` stands in for the bar's stacking only: a `dock` surface on the Top layer. The real bar's namespace and
  layer decide the stacking against Plasma's popups and toasts, and should be checked again in Task 7.
- The user's live desktop was not touched: no call reached their buses, and nothing ran on their display.

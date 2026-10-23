# Bar bridge spike: the window list, Meta on its own, launch focus and popup fades

Date: 2026-10-23. Plan 3a Task 1, answering design §8 questions 1 and 3 and the plan's two added questions (does a
launched app get focus, does KWin fade `xdg_popup`s).

## Answers

| Question | Answer |
|---|---|
| Can the KWin script report the window list and act on commands? | **Yes.** Reports arrived in activation order 4/4; activate, minimize and close all worked |
| Click-to-act latency (target ≤ 50 ms) | **1–2 ms** median, 3 ms worst over three full runs, from the stub queuing a command to KWin's signal |
| Does `DBusCall` have `failed()`, and does it fire when ghostd is gone? | **Yes.** It fired 4–5 ms after the stub died, then once per 1 s retry |
| Idle D-Bus traffic | **One `Next` per 20 s**: 6 messages a minute on the whole bus, all of them the bridge's |
| **Does the `kwinrc` `[ModifierOnlyShortcuts]` binding reach a ghostd method?** | **No. KWin 6.7.5 no longer reads that group.** 5 Meta taps, 0 `ToggleLauncher` calls |
| Does Meta+S still reach KRunner's binding? | **Yes**, through KIO's D-Bus activation of `org.kde.krunner`. No `globalShortcutPressed` signal is emitted for it |
| Does Meta held with another key toggle? | No toggle (moot, since Meta alone doesn't toggle either) |
| **Does bare Meta as a KGlobalAccel shortcut of ghost-shell's own component reach ghostd?** | **Yes, once plasmashell's launcher entry no longer holds bare Meta:** 15/15 taps gave `globalShortcutPressed` for `ghost-shell/launcher`. With that entry kept, the key is shared and only plasmashell's action fires (0/20) |
| Does a launched app get focus? | **Yes, by every route** (`DesktopEntry.execute()`, `gtk-launch`, `kioclient exec`), 9/9 in each effect set |
| Does KWin fade an `xdg_popup` (`PopupWindow`)? | **Yes**, under both effect sets (`fadingpopups`) |
| Does KWin fade a layer-shell `dock` surface? | **Not with the stock effects; yes with the user's own effect set** (`kinetic_fadingpopups`, from opacity 0.2) |
| A surface that no effect fades | **Layer-shell namespace `utility`**, under both effect sets |

**The `kwinrc` route for Meta is dead, but a fallback works.** Design §8 says "Any 'no' on 1–3 without a working
fallback stops the plan". A follow-up in the same sandbox ("Meta through KGlobalAccel") showed that bare Meta reaches
ghostd as a KGlobalAccel shortcut of the `ghost-shell` component, registered the way ghostd registers its view
shortcuts. That works only once bare Meta is removed from plasmashell's `activate application launcher` entry. While
both hold the key, plasmashell's action fires and ghost-shell's never does, whichever registers first.

**Plan call 1 ("every surface uses `dock`") does not hold on this machine.** Under the user's effect set a `dock`
surface fades in from 20 % over the windows behind it, as the 2c spike also found. See "Popup fades".

## Setup

- Fedora 44, KWin 6.7.5, kglobalacceld 6.7.5, plasma-workspace 6.7.5, Quickshell 0.2.1. Everything ran in one
  headless KWin (`--virtual --no-lockscreen`, one 2560x1440 output) with its own session bus, a second private bus as
  its system bus, and sandboxed XDG dirs under `/tmp/gs-sbx-3a-t1`. The `qs` and `qml` clients ran at
  `QT_SCALE_FACTOR=1.15` with `GHOST_GREETING=off`. Nothing ran on the user's display or reached their buses.
- The user's `kwinrc` and `kglobalshortcutsrc` were copied into the sandbox with `cp`. Only the copies were edited,
  with `kwriteconfig6 --file <copy>`, to add `[ModifierOnlyShortcuts] Meta=…` and enable the spike script. That
  `kwinrc` has no `[Windows] FocusStealingPreventionLevel`, so KWin's default, 1 (Low), applied. It has no
  `[ModifierOnlyShortcuts]` group of its own either.
- Two effect sets:
  - **stock:** the copied `kwinrc` alone. Its `[Plugins]` turns on effects that live in the user's data dir, so
    without those files KWin loaded its built-in set minus `scale` (turned off there). That set includes
    `fadingpopups`.
  - **live** (`SPIKE_LIVE_EFFECTS=1`): the user's `~/.local/share/kwin/effects` was copied into the sandbox as well.
    KWin then also loaded `kinetic_fadingpopups`, `kinetic_scale`, `kinetic_squash`, `kinetic_maximize` and
    `kwin4_effect_geometry_change`, alongside the stock `fadingpopups`. The user's KWin scripts were not copied.
- `spikes/bar-bridge/`:
  - `kwin/bar-bridge-spike/`: a copy of `kwin/ghost-shell-desktop` (its cover and focus reports kept), extended:
    - `main.js` keeps a most-recently-activated list of `normalWindow || dialog` windows and reports it as JSON
      whenever the JSON changes (fields: `id` = `internalId`, `caption`, `appId` = `desktopFileName` falling back to
      `resourceClass`, `resourceClass`, `pid`, `active`, `minimized`, `fullScreen`, `skipTaskbar`,
      `demandsAttention`). It reports on `windowAdded`, `windowRemoved`, `windowActivated`, and each window's
      `captionChanged`, `minimizedChanged`, `fullScreenChanged`, `demandsAttentionChanged` and
      `skipTaskbarChanged`. `handleCommand(json)` runs `activate` (unminimize, then `workspace.activeWindow = w`),
      `minimize` (`w.minimized = true`) or `close` (`w.closeWindow()`). Each signal is logged with `Date.now()`.
    - `main.qml` has a `DBusCall` for `org.ghostshell.Windows1.Report(s)` and one for
      `org.ghostshell.WindowCommands1.Next()`, both on `org.ghostshell.Ghostd` at `/org/ghostshell/Windows`.
  - `stub_ghostd.py`: owns `org.ghostshell.Ghostd` on the private bus. `Next()` returns a queued command at once, or
    `""` after 20 s. `org.ghostshell.Desktop1.ToggleLauncher()` at `/org/ghostshell/Desktop` only logs. A control
    object, `org.ghostshell.Spike1` at `/org/ghostshell/Spike`, takes `Queue(s)` and answers `Windows()`.
  - `drive.py`: queues commands through the control object, checks the reports, and computes the latencies and
    traffic from the logs.
  - `windows.qml` (`qs`, two `FloatingWindow`s, app "A"), `plain.qml` (Qt's `qml` runtime, app "B", the 20-window
    close pool and the launched app). Quickshell fixes every window's app id to `org.quickshell`, so a second app id
    needed a second toolkit. The `qml` runtime's is `org.qt-project.qml`.
  - `fake_chord.py`: chords on top of `spikes/greeting-handoff/fake_key.py`'s client (press in order, release in
    reverse), since `fake_key.py` only taps one key.
  - `shell.qml`: the launch and popup checks (below).
  - `run.sh`: the whole spike. `SPIKE_LIVE_EFFECTS=1` selects the live set. Three full runs (stock, live, stock again) gave the
    figures below.
- KWin needs `QT_FORCE_STDERR_LOGGING=1` in its environment, or its log (with the script's timestamped lines) never
  reaches `kwin.log`. The script logs with `console.info`: Fedora's `qtlogging.ini` drops `console.log`'s debug level.

## Window list and commands

### Signal names (KWin 6.7.5)

- `DBusCall` (`src/scripting/dbuscall.h:135–149`): **`finished(QVariantList returnValue)`** and **`failed()`**. In QML
  these are `onFinished: returnValue => …` (the reply's arguments as a list; `Next`'s string is `returnValue[0]`) and
  `onFailed: …`. `call()` uses `QDBusConnection::sessionBus().asyncCall`, so Qt's default 25 s timeout applies and a
  20 s long poll fits under it (`dbuscall.cpp`).
- Workspace: `windowAdded`, `windowRemoved`, `windowActivated`. Window: `captionChanged`, `minimizedChanged`,
  `fullScreenChanged`, `demandsAttentionChanged`, `skipTaskbarChanged`.

### What the script must handle

Each of these broke the spike once:
- **KWin emits `windowActivated` for a new window before `windowAdded`** (`src/workspace.cpp:930–951`). An MRU list
  that pushes on `windowAdded` lists the window twice unless it checks first.
- **Minimizing emits `windowActivated` for the next window before `minimizedChanged`.** The report from the first
  signal already shows the window minimized, so the second report is identical and is skipped.
- **Activating the window that is already active emits nothing.**
- **`workspace.windowList()` does not exist in a declarative script.** It is on `QtScriptWorkspaceWrapper` only
  (`src/scripting/workspace_wrapper.h:634–647`). Use `workspace.stackingOrder`.
- **`onFinished` must call `Next()` again before handling the command.** When handling came first, a `TypeError`
  ended the loop for good.
- **A restarted ghostd has no window list** until a window changes, since the script reports only on change. The
  spike resends the list with each 1 s retry, so a restarted stub had the list as soon as its first `Next` arrived.

### Order and behaviour

- Starting from `b-1*, a-2, a-1`, activating `a-1`, `b-1`, `a-2`, `a-1` gave reports in exactly the expected order
  every time, **4/4 in both runs**. `*` marks the active window.
- **Activate** on a minimized window unminimizes and focuses it. **Minimize** moves focus to the next window in
  KWin's focus chain, here the previously active one. **Close** removes the window and focuses the next one. All of
  this is KWin's own policy, the same one Plasma's task manager gets, since that also only asks KWin to activate,
  minimize or close.
- `skipTaskbar` is reported but not filtered (the brief asked for `normalWindow || dialog`). Plan call 4 filters it.

### Latency, queue to signal

From the stub logging `queued` to the script logging the KWin signal, all on the epoch-ms clock. n = 24 activations
(4 order checks + 20 alternating), 20 minimizes, 20 activations of a minimized window, 20 closes.

| Action, signal | Stock: median / p90 / max | Live: median / p90 / max |
|---|---|---|
| activate → `windowActivated` | 1 / 1 / 2 ms | 1 / 2 / 2 ms |
| minimize → `minimizedChanged` | 1 / 1 / 1 ms | 1 / 1 / 2 ms |
| activate a minimized window → `windowActivated` | 1 / 1 / 2 ms | 1 / 2 / 2 ms |
| close → `windowRemoved` | 2 / 2 / 2 ms | 2 / 2 / 2 ms |

- The D-Bus hop alone (queued → `command` logged in the script) was 0–1 ms. The report that shows the change
  reached the stub 0–2 ms after queuing.
- Not included: the shell → ghostd hop (a socket message), and the client's own redraw. The spike measures the
  ghostd → KWin half that the long poll adds.

### `failed()` and the retry

- With the stub killed while a `Next` was pending, D-Bus answered `org.freedesktop.DBus.Error.NoReply` and **`failed()`
  fired 4–5 ms later**. It then fired again on each 1 s retry (`ServiceUnknown`, since the private bus has no service
  files): 1 005, 2 005 and 3 006 ms after the stop.
- With a new stub up 3.5 s after the stop, `Next` and the window list both reached it 4 006–4 007 ms after the stop,
  at the next retry.
- Before the stub first existed, the script's poll failed once a second in the same way, and recovered the same way.

### Idle traffic

- Over 60 s with no window changes, the bus carried **6 messages, all the bridge's: 3 `Next` calls and their
  replies** (2 replies fell inside the window, the third was still pending at its end). That is the expected one
  `Next` per 20 s, with no `Report` while nothing changes. The same in both runs.

## Meta on its own

- **Result, both runs:** 5 Meta taps gave **0** `ToggleLauncher` calls at the stub, with
  `[ModifierOnlyShortcuts] Meta=org.ghostshell.Ghostd,/org/ghostshell/Desktop,org.ghostshell.Desktop1,ToggleLauncher`
  in the sandbox `kwinrc` from the start.
- **Why:** KWin 6.7.5 has no reader for the group any more.
  - The string `ModifierOnlyShortcuts` does not occur anywhere in KWin's v6.7.5 source tree, nor in the installed
    `/usr/lib64/libkwin.so.6.7.5` (`strings`, both encodings). Other config keys from the same `kwinrc`, such as
    `FocusStealingPreventionLevel`, do occur there.
  - Modifier-only shortcuts are now ordinary KGlobalAccel shortcuts. kglobalacceld's
    `GlobalShortcutsRegistry::keyEvent` (`src/globalshortcutsregistry.cpp:479–528`) fires a shortcut whose key is a
    bare modifier when that modifier is released with nothing else pressed in between. KWin's plugin
    (`src/plugins/kglobalaccel/kglobalaccel_plugin.cpp`) only forwards keys to it.
  - In the copied `kglobalshortcutsrc`, Meta alone belongs to plasmashell's
    `activate application launcher=Meta\tAlt+F1`. In the sandbox that component was registered but inactive
    (`isActive` false, since no plasmashell ran), so a tap did nothing at all.
- **Meta+S** reached KRunner's binding in both runs. The binding is a *service* component (`[services][org.kde.krunner.desktop]`,
  backed by `/usr/share/kglobalaccel/org.kde.krunner.desktop`). kglobalacceld starts such components through
  `KIO::ApplicationLauncherJob` (`src/kserviceactioncomponent.cpp:56–96`), so the private bus carried one
  `org.freedesktop.Application.Activate` call to `org.kde.krunner` at `/org/kde/krunner` per press. It failed there,
  since that bus can't activate anything, so KRunner never started. **No `globalShortcutPressed` signal is emitted for a
  service component.** That signal (`src/component.cpp:180–195`) is only for components a running app registered.
- **Meta+K**, which nothing binds with Meta, toggled nothing.
- Service components whose desktop files live in the user's own data dir (9 of them) failed to load in the sandbox
  ("could not create a component"), so none of the user's custom launchers could fire.

The KGlobalAccel route was tried afterwards, at the coordinator's request; see the next section.

**Not covered:** design §8.3's "With the panel gone, are Meta+1…0 still held by plasmashell?" needs a running
plasmashell, which this brief did not include. The copied file shows plasmashell's task manager holding Meta+1…9.

## Meta through KGlobalAccel

A follow-up, `spikes/bar-bridge/meta.sh`, in a headless KWin `3a-t1-meta` with the same isolation as `run.sh`: own
session bus, private system bus, sandboxed XDG dirs, and the user's `kwinrc` and `kglobalshortcutsrc` copied in with
`cp`. Only the copies were edited. `meta_kglobalaccel.py` registers actions as ghostd's
`crates/ghostd/src/feeds/shortcuts.rs` does:
- `doRegister(["ghost-shell", "launcher", "ghost-shell", "Open the launcher"])`;
- then `setShortcutKeys(id, [[key, 0, 0, 0]], SET_PRESENT | NO_AUTOLOADING)`.

It logs every `globalShortcutPressed` from `org.kde.kglobalaccel.Component`. `fake_key.py` tapped Meta alone (5 taps,
0.5 s apart). `fake_chord.py` then pressed Meta+S, Meta+1 and Meta+K.

### The key

- The copy holds bare Meta in `[plasmashell] activate application launcher=Meta\tAlt+F1,Meta\tAlt+F1,…`. The `\t`
  there is a literal backslash and `t` joining the keys.
- KGlobalAccel (`allShortcutInfos` on `/component/plasmashell`) stores the action's keys as `150994992` (`0x09000030`,
  Alt+F1) and **`16777250` (`0x01000022`, `Qt::Key_Meta` with no modifier bit) for bare Meta**.
  - That differs from `Meta+<key>` combinations, which carry the modifier bit `0x10000000`, as in `keyseq.rs`'s
    `Meta+M` = `0x1000004D`.
  - `keyseq::parse` rejects a bare "Meta" today (`Unparsable`: "meta" is no key name), so Task 5 or 11 must special-case it.
- The registration asked for exactly `16777250`. KGlobalAccel granted `[[16777250, 0, 0, 0]]` in every run.

### Retired: bare Meta removed from plasmashell's entry in the copy

The retire step was taken in the copy: the first field became `Alt+F1`, and the defaults were left alone. KGlobalAccel
then held `keys=[150994992]` for plasmashell's action, and `globalShortcutsByKey(16777250, Equal)` listed only
`ghost-shell/launcher`.

| Input | `ghost-shell/launcher` pressed | Other effects |
|---|---|---|
| Meta tapped alone, 5 × 3 runs | **15/15** | none |
| Meta+S | 0 | KRunner's binding still fired: 1 `org.freedesktop.Application.Activate` to `org.kde.krunner` per press |
| Meta+1 | 0 | none (plasmashell's task manager, which holds it, wasn't running) |
| Meta+K | 0 | none |

- The signal fires on Meta's **release**, within 0–1 ms of `fake_key.py`'s press-and-release. This is kglobalacceld's
  modifier-only rule (`globalshortcutsregistry.cpp:479–528`), and the release is also why a held Meta followed by
  another key never fires it.
- `globalShortcutPressed` arrives on `/component/ghost_shell` with arguments (`"ghost-shell"`, `"launcher"`,
  timestamp): the path, interface and member ghostd already matches for its view shortcuts.

### Shared: plasmashell's entry kept

A stand-in plasmashell registered `["plasmashell", "activate application launcher", "plasmashell", "Activate
Application Launcher"]` with `SET_PRESENT` only, so it took its stored keys (`Alt+F1` and bare Meta), as a running
plasmashell would. KGlobalAccel granted ghost-shell's bare Meta anyway. `globalShortcutsByKey(16777250, Equal)` then
listed **two holders**, plasmashell's action and `ghost-shell/launcher`.

| Order | Meta taps | `plasmashell/activate application launcher` | `ghost-shell/launcher` |
|---|---|---|---|
| plasmashell registers first (as at login), 3 runs | 15 | **15** | **0** |
| ghost-shell registers first, 1 run | 5 | **5** | **0** |

Meta+S, Meta+1 and Meta+K fired neither action, and Meta+S still reached KRunner. So a shared bare Meta is a silent
loss for ghost-shell. ghostd's existing `warn_if_shared` check would log it, but the user would just get Plasma's
launcher.

### What it means for the plan

- The launcher's Meta works through KGlobalAccel, with no new mechanism. It is one more action on ghostd's existing
  `ghost-shell` component, listened for exactly as the view shortcuts are.
- It needs a retire step that **removes bare Meta from plasmashell's `activate application launcher`**, and
  only that key: Alt+F1 can stay. It can't be done by registering over it. The plan must decide how and when that step
  runs on the live session, and how it is undone. That is the user's call, and nothing here touched their files.
- Because the key is `0x01000022`, not a modifier bit plus a key, the config parser and the shared-key check need a
  bare-Meta case.

## Launch focus

- `shell.qml` shows a `dock` layer surface with exclusive keyboard focus, like the launcher, then waits 300 ms. It
  starts the test entry (in the sandbox's `XDG_DATA_HOME/applications`, `Exec=sh -c "sleep 1; exec qml …"`) and
  hides the surface at once. Before each trial a normal window (`a-1`) was made active.
- **`DesktopEntry.execute()` gave the launched window focus in 3/3 trials, in both runs.** `Quickshell.execDetached`
  with `gtk-launch <id>` and with `kioclient exec <path>` did too, 3/3 each, in both runs. The window opened
  1.10–1.15 s after the call, and was then active and first in the report, with `demandsAttention` false.
- **Why:** at focus stealing prevention Low, KWin activates every new window that wants input, token or not
  (`src/workspace.cpp:930–934`: "focus stealing prevention 'low' should always activate new windows"). So the route
  doesn't matter at the user's setting. At Medium or above, the window would need an `xdg_activation` token that
  `mayActivate` accepts (`src/activation.cpp:580–639`). Whether `execute()` passes one was not tested.
- `DesktopEntries` fills in after `qs` starts. A `byId` made 600 ms after start returned null in one run, so the
  launcher must not assume entries are there at load.

## Popup fades

`shell.qml` shows one 300 x 300 red surface at a time, at the same place: an xdg_popup (`PopupWindow` anchored to a
`dock` bar `PanelWindow`), a layer-shell `dock` surface, and a layer-shell `utility` surface. KWin placed all three at
460,46–804,390 (Quickshell scales layer-shell margins with `QT_SCALE_FACTOR`, as it does the popup's anchor).
`grab.py watch` sampled 64 x 64 at 500,100 over black, at **1 428–2 224 captures/s**. Each kind was
shown 3 times per run.

| Surface | Stock effects | Live effects |
|---|---|---|
| `PopupWindow` (xdg_popup) | **fades in** from 0 (first red 7–14), 880–1 420 captures over 430–650 ms (two runs) | **fades in** from 0 (first red 7), 900–1 260 captures over 500–650 ms |
| layer-shell `dock` | full red in the first capture, 6/6 (two runs) | **fades in** from 0.2 (first red 51), 1 090–1 390 captures over 590–650 ms |
| layer-shell `utility` | full red in the first capture, 6/6 (two runs) | full red in the first capture, 3/3 |
| `PopupWindow` with `fadingpopups` and `kinetic_fadingpopups` unloaded | full red in the first capture | full red in the first capture |

- The `PopupWindow` fade is the stock **`fadingpopups`** effect. It animates any window whose `popupWindow` is true,
  which every xdg_popup is (`src/plugins/fadingpopups/package/contents/code/main.js:23–58`). It is in the stock set
  and stays on in the user's `kwinrc`. The unloaded control above confirms it.
- The live `dock` fade is the user's **`kinetic_fadingpopups`**, which adds `window.dock` to the windows it fades,
  from opacity 0.2 (its `main.js:59`, `92`). This is the 2c finding again.
- **Durations:** both effects nominally fade in over 150–200 ms (`animationTime(150)`, `animationTime(200)`). The
  headless runs measured 430–650 ms, and a separate check outside the full run gave 250–350 ms. Read the presence
  of a fade as the result, and the length as inflated by the capture load.
- **Which surface kind for popups:** neither `PopupWindow` nor `dock` shows without a fade under the user's effects.
  Only namespace **`utility`** did, in every trial. The 2c note lists what Utility brings: Show Desktop (Meta+D) hides
  it, and ghost-shell-desktop ignores it for cover. Docks are not hidden by Show Desktop, so the bar itself is
  probably better kept `dock`. That trade, and whether its fade at login matters, is the user's decision. The
  surfaces that open on demand (launcher, hover lists, menus, tooltips) would flash under `dock` and not under
  `utility`.

## Limits

- Headless KWin, one output, a virtual input device, no GPU-specific timing. The effect sets were copies of the
  user's configuration and files, not their live compositor.
- No plasmashell ran, so plasmashell's KGlobalAccel shortcuts (Meta for the launcher, Meta+1…9) were inactive in
  `run.sh`. In `meta.sh`'s shared mode a stand-in registered the launcher action the way an app does, not the real
  plasmashell.
- Latencies were measured on one machine with an idle KWin. The command path is one D-Bus round trip on the
  session bus.

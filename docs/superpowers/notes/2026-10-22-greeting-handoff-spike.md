# Greeting hand-off spike: the overlay under the lock screen

Date: 2026-10-22. Plan 2c Task 1, answering lock design §11 spike 3 (§6.5): can a layer-shell Overlay surface,
mapped while kscreenlocker is up, be the first thing on screen when it unlocks?

## Answer

**Yes, on one condition: the overlay's window type must be one that no enabled open effect animates, measured with the
live effect set. On this machine that is `utility` (namespace `"utility"`, window type Utility).** KWin types a layer
surface by its namespace, and open effects choose the windows they fade in by type. An effect that fades the overlay
in starts its animation when the overlay maps under the lock, but the animation only advances when the overlay is
painted, so the whole fade plays after the unlock, over the windows. That is the flash §6.5 forbids.

With the user's effect set copied into the sandbox, `utility` showed the overlay's ground in the first capture after
the unlock in all three runs, with no frame of the window under it. One run with the stock effects agreed. The
other types failed:

- **Normal** (any namespace KWin does not know): the window showed at full strength at the unlock under the stock
  `scale`, and at 92 % under the user's `kinetic_scale`.
- **`dock`**, which passes with the stock effects, failed with the live set. The user's `kinetic_fadingpopups` fades
  Docks in from opacity 0.2, so the window showed at 80 % and faded out over 154–181 ms.

`desktop` passed too, but it brings KWin and ghost-shell behaviour that `utility` does not (see "Window types" below).

## Setup

- Fedora 44, KWin 6.7.5, kscreenlocker 6.7.5, plasma-workspace 6.7.5, Quickshell 0.2.1. All of it in a headless KWin
  (`--virtual`, one 2560x1440 output, the screen locker on), with `qs` at `QT_SCALE_FACTOR=1.15`. Nothing ran on the
  user's display, and nothing reached the user's session or their real system bus.
- `spikes/greeting-handoff/`:
  - `shell.qml`: a full-screen `WlrLayer.Overlay` `PanelWindow` in `#0a0f11`. It maps when the file in
    `SPIKE_ARM_FILE` appears and asks for `WlrKeyboardFocus.Exclusive` when the one in `SPIKE_FOCUS_FILE` appears. Its
    namespace is `SPIKE_NAMESPACE`. Every `frameSwapped` bumps a counter drawn in the corner, which asks for the next
    frame, and logs `spike overlay frame N at <epoch ms>`. So the log records every frame Qt renders.
  - `window.qml`: a normal toplevel (`FloatingWindow`) the size of the screen, filled `#ff0000`, standing in for the
    user's windows. It logs the keys it receives and its `active` changes.
  - `run.sh`: the whole spike. It is a spike-local copy of `scripts/headless-kwin.sh` without `--no-lockscreen`, on
    its own session bus, plus:
    - a second private bus (`dbus-daemon` with `scripts/private-bus.conf`) passed to KWin as `DBUS_SYSTEM_BUS_ADDRESS`
      (ruling R2). `dbus-monitor` records everything on it;
    - `KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1` and `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1`, set only in that KWin's
      environment.
  - `fake_logind.py`: an `org.freedesktop.login1` on the private system bus. `GetSession` returns
    `/org/freedesktop/login1/session/spike`, the session has `SetLockedHint` and `LockedHint`, and `SIGUSR1` emits the
    session's `Unlock` signal.
  - `grab.py`: `org.kde.KWin.ScreenShot2` on KWin's private session bus. `shot` captures the workspace to PNG.
    `watch` captures a 64 x 64 area at the screen's centre as fast as KWin answers (1 700–2 000 captures/s), and
    classes each capture as red, ground, black or other.
  - `fake_key.py`: a keyboard for the headless KWin through `org_kde_kwin_fake_input`, written in the Wayland wire
    format with only the standard library. A virtual KWin has no input devices, so without it no client gets a
    `wl_keyboard`.
- Sequence in `run.sh`:
  1. Start the red window and type a key, which it receives.
  2. Start the overlay, not yet mapped.
  3. `org.freedesktop.ScreenSaver.Lock` on KWin's own bus. Wait for `ActiveChanged(true)`.
  4. Arm the overlay under the lock, and wait 3 s.
  5. Start `watch`, and send `Unlock` from the fake logind. Wait for `ActiveChanged(false)`.
  6. Ask for exclusive focus, then type a key.
- Run with `spikes/greeting-handoff/run.sh`. Options:
  - `SPIKE_NAMESPACE` sets the overlay's namespace (default `utility`); `ghost-greeting-spike` reproduces the Normal
    fade.
  - `SPIKE_LIVE_EFFECTS=1` copies the `[Plugins]` and `[Effect-*]` sections of `~/.config/kwinrc`, and
    `~/.local/share/kwin/{effects,scripts}`, into the sandbox only. It reads the user's files and writes nothing back.
  - `results.txt` records KWin's `loadedEffects`, whether each user script is loaded, and the most red in any capture
    once the lock screen is gone.

## KWin's rule while locked (source, tag v6.7.5)

- `src/scene/windowitem.cpp:160–167`, `WindowItem::computeVisibility`: while `waylandServer()->isScreenLocked()`, only
  `isLockScreen()`, `isInputMethod()` and `isLockScreenOverlay()` windows are visible. `isScreenLocked()` covers both
  `Locked` and `AcquiringLock` (`src/wayland_server.cpp:806–816`).
- `windowitem.cpp:195–203`, `updateVisibility`: a window that is not visible is also `setSuspended(true)`, so KWin
  sends it no frame callbacks. Every `WindowItem` reruns this on `WaylandServer::lockStateChanged` (`windowitem.cpp:53`),
  which forwards `KSldApp::lockStateChanged` (`wayland_server.cpp:677`).
- A layer-shell Overlay surface is **not** a lock-screen overlay. That flag is set only through the
  `kde_lockscreen_overlay_v1` protocol (`wayland_server.cpp:497–504`, `Window::setLockScreenOverlay`), which KWin keeps
  from clients that do not declare it (`wayland_server.cpp:138–144`, `154–196`). Quickshell does not use it. So the overlay is
  hidden under the lock like any window, and stays in KWin's `OverlayLayer`, above normal windows
  (`src/layershellv1window.cpp:217–227`).
- `KSldApp::doUnlock` (kscreenlocker `ksldapp.cpp:584–620`) emits `unlocked()` and then `lockStateChanged()` in one call.
  `unlocked()` emits `ActiveChanged(false)` (`interface.cpp:96`, `218–221`) and `SetLockedHint(false)`. So the D-Bus
  signal, the end of the lock screen and every window's visibility change all happen in one pass of KWin's event loop.
  The overlay and the windows under it become visible together, and the overlay is on top.

## Results

### Mapping under the lock

- **The overlay maps while locked.** Its first frame renders 87–92 ms after it is armed, under the lock, in every run.
- **It stays hidden.** The captures after arming (`3-locked-armed.png`) match those before it (`2-locked.png`): the
  lock screen, with no ground anywhere.
- **Qt renders exactly one frame and then stops.** KWin suspends the hidden overlay, so no frame callback comes, and
  the counter stays at 1 for the whole 4 s under the lock. Frame 2 comes only after the unlock. So **whatever the
  overlay draws in its first frame under the lock is what appears at the unlock.** Nothing changed later under the
  lock (a recolour, dots that load late) reaches the screen before then.

### The unlock, stock effects (fresh sandbox config)

Times are from `dbus-monitor` (`ActiveChanged(false)`), the overlay's log and `watch`, all in epoch ms.

| Run | Namespace (window type) | `Unlock` → `ActiveChanged(false)` | Last lock-screen capture → first capture after | Red captures after the unlock | Overlay's next frame after `ActiveChanged(false)` |
|---|---|---|---|---|---|
| 1 | `ghost-greeting-spike` (Normal) | 87 ms | 2 ms, then **red** | 1–21 ms after it, then a fade to ground by +173 ms | 8 ms |
| 2 | `ghost-greeting-spike` (Normal) | 84 ms | 1 ms, then **red** | 1–23 ms after it, then a fade to ground by +158 ms | 9 ms |
| 3 | `dock` (Dock) | 87 ms | 3 ms, then ground | none | 17 ms |
| 4 | `dock` (Dock) | 67 ms | 1 ms, then ground | none | 14 ms |
| 5 | `dock` (Dock) | 82 ms | 1 ms, then ground | none | 5 ms |
| 6 | `dock` (Dock) | 69 ms | 2 ms, then ground | none | 15 ms |
| 7 | `utility` (Utility) | 86 ms | 1 ms, then ground | none | 9 ms |

### The unlock, the user's live effect set

These runs used `SPIKE_LIVE_EFFECTS=1`. KWin's `loadedEffects` then included `kinetic_scale`, `fadingpopups`,
`kinetic_fadingpopups`, `kinetic_maximize`, `kinetic_squash`, `kwin4_effect_geometry_change`, `sheet`,
`dialogparent`, `wobblywindows`, `translucency`, `blur`, `slidingpopups` and `slidingnotifications`, but not `scale`,
which the user turned off. Both user scripts, `ghost-shell-desktop` and `gridlock-keeper`, were loaded.

"First scene" is the first capture that differs from the lock screen. The ground's red is 10, so "peak red" above 10
is the window showing through.

| Run | Namespace (window type) | First scene after the lock screen | Peak red | Ground reached | Overlay's next frame |
|---|---|---|---|---|---|
| L1 | `dock` (Dock) | +2 ms, `206,3,3`: the window at 80 % | 206 | +181 ms | 16 ms |
| L2 | `dock` (Dock) | +1 ms, `206,3,3` | 206 | +154 ms | 7 ms |
| L3 | `ghost-greeting-spike` (Normal) | +2 ms, `235,1,1`: the window at 92 % | 235 | +243 ms | 13 ms |
| L4 | `utility` (Utility) | +0 ms, ground | 10 | +0 ms | 7 ms |
| L5 | `utility` (Utility) | +0 ms, ground | 10 | +0 ms | 6 ms |
| L6 | `utility` (Utility) | +0 ms, ground | 10 | +0 ms | 4 ms |
| L7 | `desktop` (Desktop) | +3 ms, ground | 10 | +3 ms | 16 ms |
| L8 | `desktop` (Desktop) | +0 ms, ground | 10 | +0 ms | 6 ms |

Two more runs (`utility` and `desktop`) had a layer-shell desktop stand-in under the red window for a Show Desktop
probe. That probe was inconclusive (see "Window types"), and both runs were also flash-free, at peak red 10.

`206` is `kinetic_fadingpopups`' starting opacity, 0.2, over the red window: `255 × 0.8 + 10 × 0.2`. `235` is
`kinetic_scale`'s, 1/12.

- **When no effect animates the overlay, the first scene after the unlock is the overlay**, drawn from the buffer it
  committed under the lock. `watch` never saw red, black or a blend: the lock screen gives way to ground between two
  captures under 3 ms apart.
- **The delay from `ActiveChanged(false)` to the overlay's next rendered frame is 4–17 ms**, about one refresh. It
  doesn't matter for the flash, because the overlay's last frame is already on screen at the unlock. It does bound how
  soon the greeting can start moving.
- **With a Normal-type namespace, the windows show.** The scale effect (`/usr/share/kwin-wayland/effects/scale/contents/code/main.js:42–80`,
  `92–121`) animates every managed Normal or Dialog window on `windowAdded`: opacity from 0 and scale from 0.8,
  OutCubic, 200 ms. A layer surface takes its window type from its namespace, and an unknown one is
  `WindowType::Normal` (`src/layershellv1window.cpp:22–35`, `45`). `windowAdded` fires when the overlay maps under
  the lock. `EffectWindow::isVisible` ignores the lock (`src/effect/effectwindow.cpp:193–199`), so the effect starts
  the animation then. But the timeline advances only in `AnimationEffect::prePaintWindow`
  (`src/effect/animationeffect.cpp:469–491`), which a hidden window never reaches. The fade therefore begins at the
  first paint after the unlock, from opacity 0: `watch-…-red.png` in runs 1 and 2 is `#ff0000`.
- **Which types each open effect animates** (by source, then confirmed by the runs above):

  | Effect | Enabled | Normal | Dock | Utility | Desktop |
  |---|---|---|---|---|---|
  | `scale` (`/usr/share/kwin-wayland/effects/scale/contents/code/main.js:42–80`) | stock: yes; live: no | yes | no | no | no |
  | `kinetic_scale` (`~/.local/share/kwin/effects/kinetic_scale/contents/code/main.js:44–81`) | live | yes | no | no | no |
  | `fadingpopups` (`/usr/share/kwin-wayland/effects/fadingpopups/contents/code/main.js:23–58`) | stock and live | no | no | no | no |
  | `kinetic_fadingpopups` (`~/.local/share/kwin/effects/kinetic_fadingpopups/contents/code/main.js:23–67`, dock at `:59`) | live | no | **yes** | no | no |

  - `scale` and `kinetic_scale` animate managed `normalWindow || dialog` windows.
  - Both `fadingpopups` effects animate popups, outlines, unmanaged non-utility windows, and the splash, toolbar, OSD
    and applet-popup types. The kinetic fork adds `dock`, `notification` and `criticalNotification` (`:59–63`).
  - A layer surface is managed, so `fadingpopups`' unmanaged branch, which is where it excludes `utility`, never
    applies to it.
  - The other enabled effects react to other events: maximize, minimize, geometry change, modal dialogs, window
    movement and opacity during move/resize (`[Effect-translucency] MoveResize=88`). Their `windowAdded` handlers
    were not read.
  - The measured runs above are the evidence that none of them fades a Utility overlay in.
- **The lock screen does not fade out.** It is gone in one step: `scale` skips `window.lockScreen` (`main.js:68–69`),
  and `fadingpopups` blacklists `kscreenlocker_greet` (`main.js:17–18`). Stock `LockScreenUi.qml:55–57` calls
  `Qt.quit()` at once on success, and the greeter then `_exit(0)`s (kscreenlocker `greeter/greeterapp.cpp:330–337`).
  So the overlay needs no state to match a half-faded lock screen. It only has to show what the lock screen's last
  frame showed: lock design §6.5's ground + dots.
- **No black gap.** The greeter's process ends before `doUnlock` runs, but `watch` caught no frame without the lock
  screen or the overlay in any run.

### Keyboard

- **`WlrKeyboardFocus.Exclusive`, asked for after the unlock, takes the keyboard,** whatever the type (Normal, Dock,
  Utility, Desktop) and effect set. In every run, the overlay became active 3–16 ms after asking, the red window went inactive within 1 ms of that, and the next key reached the
  overlay (`spike overlay key 65`), not the red window.
- **Before that, KWin gives the keyboard back to the last active window.** The red window became active 2–15 ms after
  `ActiveChanged(false)` and kept the keyboard until the overlay asked. In the spike that was 80–109 ms, because the
  request waits on a 50 ms file poll. A key typed in that window goes to the user's window, not the greeting. Whether
  asking for Exclusive while still locked is safe was not tested: KWin routes every key to the greeter while locked,
  but plan call 7 keeps the armed overlay at `None`.

### Window types for a layer-shell Overlay (KWin v6.7.5 source)

The type changes no stacking: `belongsToLayer` follows the layer-shell layer (`src/layershellv1window.cpp:217–227`).
It changes no placement, because `isPlaceable()` is false for layer surfaces (`:108–111`), so `Placement`'s Utility
branch (`src/placement.cpp:41`) never runs. It changes no focus rule either: `acceptsFocus` and `wantsInput` follow
keyboard interactivity (`:133–136`, `233–236`), and Exclusive was measured working for all four types. What does
differ:

- **Utility and Show Desktop.**
  - `Workspace::breaksShowingDesktop` (`src/workspace.cpp:1743–1746`) is true for Utility (and Normal), false for Dock
    and Desktop.
  - So adding a Utility overlay while Show Desktop is on ends Show Desktop (`addWaylandWindow`, `:911–915`).
  - And turning Show Desktop on while it is up hides it (`setShowingDesktop`, `:1759–1767`).
  - At the lock this is moot. kscreenlocker's greeter is itself a layer surface with LayerShellQt's default scope
    `"window"` (`greeter/greeterapp.cpp:421–427`), so it is Normal-typed and already ends Show Desktop when it maps.
    This is by source; the sandbox could not hold Show Desktop to measure it.
  - It is not moot during the greeting: Meta+D while the greeting plays would hide the overlay mid-sequence.
  - The Show Desktop probe was inconclusive and was removed from the spike. Even with a layer-shell desktop stand-in
    taking `OnDemand` focus, KWin dropped Show Desktop within 0.4 ms of `showDesktop(true)`.
- **Desktop.**
  - KWin special-cases Desktop windows in activation (`src/activation.cpp:455`, `524`), when adding windows
    (`src/workspace.cpp:929–934`) and in `findDesktop` for Show Desktop focus.
  - `setSkipSwitcher(!isDesktop())` (`src/layershellv1window.cpp:49`) leaves a Desktop-typed layer surface in the
    window switcher.
  - Both user scripts recognise the dashboard as `desktopWindow && resourceClass === "quickshell"`
    (`ghost-shell-desktop/contents/code/main.js:121–123`, `gridlock-keeper/contents/code/main.js:4–6`). A Desktop-typed
    greeting would be taken for the dashboard and restacked, and would make ghost-shell-desktop report "desktop
    active" while it holds the focus.
- **Utility in the user's scripts.** `ghost-shell-desktop` counts only Normal and Dialog windows as covering the
  dashboard (`main.js:79–87`), so a Utility overlay is ignored. While it holds the focus, the script reports the
  desktop inactive (`main.js:92`). `gridlock-keeper` ignores it.
- **Effects that single out Utility:** none of the enabled ones. `diminactive` (`src/plugins/diminactive/diminactive.cpp:196–204`)
  dims inactive Normal, Dialog, Utility, Dock and Desktop windows alike, but it is off by default and not enabled
  here.

### Unlock route

KSldApp finds its session by calling `org.freedesktop.login1.Manager.GetSession("auto")` on
`QDBusConnection::systemBus()` (kscreenlocker `logind.cpp:79`, `88`). It listens for that session's `Unlock` signal
(`logind.cpp:140`). On it, KSldApp sets `s_logindExit` and terminates the greeter (`ksldapp.cpp:304–313`). The greeter
exits 1 on SIGTERM (`greeter/main.cpp:44–50`), and the exit handler then unlocks anyway because of `s_logindExit`
(`ksldapp.cpp:251–264`) through the same `doUnlock` a password ends in. Qt's `systemBus()` honours
`DBUS_SYSTEM_BUS_ADDRESS`, so the headless KWin asked only `fake_logind.py`. It logged `GetSession('auto')`,
`SetLockedHint(True)` at the lock and `SetLockedHint(False)` at the unlock. Nothing reached the user's session 2: there
was no `loginctl` call, no password and no signal on the real system bus. The greeter's PAM stack (`/etc/pam.d/kde`:
`pam_unix`, no `pam_faillock`) only waited for a password and never got one.

## What the greeting needs (Tasks 4 and 5)

1. **Namespace `utility` on every greeting overlay:** a type that no enabled open effect animates, measured with the
   live effect set. If the user changes their KWin effects, rerun `SPIKE_LIVE_EFFECTS=1 spikes/greeting-handoff/run.sh`.
   `dock` and Normal types flash on this machine.
   - Meta+D while the greeting plays hides a Utility overlay (Show Desktop above). Tasks 4 and 5 should treat that as a
     skip, or accept it.
   - If no type survived some future effect set, the fallback is a project KWin effect that grabs
     `Effect.WindowAddedGrabRole` on the greeting's window, so that no other effect animates it. Nothing like that was
     built or installed.
2. **The armed rest must be complete in the first frame after mapping.** Ground + dots, the dots included, must be in
   the overlay's first frame under the lock, since no later frame renders until the unlock. Anything drawn
   asynchronously (a `ShaderEffect` whose `.qsb` loads late) has to be ready before the overlay maps. The other way is
   to map it only once it is ready, still before the unlock.
3. **Ask for `Exclusive` as early as the unlock is known.** Until then the user's last window has the keyboard.
4. **Project 2's side of the hand-off:** the overlay has to be up, resting on ground + dots, before the lock screen
   exits. ghostd arms it on `ActiveChanged(true)` or `PrepareForSleep(true)`, seconds before any unlock. The lock
   screen's exit scan should end on the same ground and dot grid, at the same pixel positions, so that the step from
   its last frame to the overlay's buffer is invisible. KWin adds no fade between them.

## Limits

- Headless KWin, one output and no GPU-specific timing. The effect set was the stock one or a copy of the user's, not
  the live compositor itself. The live session also differs in having a real display.
- ScreenShot2 renders the scene at the moment of the call, not the frame KWin presented. At 1 700–2 000 captures/s,
  every pass of KWin's event loop around the unlock was sampled. A presented frame is always one of those states.
- Only the centre 64 x 64 was sampled densely. The overlay's opacity is uniform across the surface, and the full
  captures before and after match.
- Multiple screens and the sleep path (`PrepareForSleep`, `LockOnResume`) were not exercised. The visibility rule
  above does not depend on why the screen locked.

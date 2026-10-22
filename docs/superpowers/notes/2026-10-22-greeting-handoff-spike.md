# Greeting hand-off spike: the overlay under the lock screen

Date: 2026-10-22. Plan 2c Task 1, answering lock design §11 spike 3 (§6.5): can a layer-shell Overlay surface,
mapped while kscreenlocker is up, be the first thing on screen when it unlocks?

## Answer

**Yes, on one condition: the overlay's layer-shell namespace must give it a window type that no open effect animates.
`dock` works.** With `dock`, the first scene after the unlock is the overlay's ground in all four runs, with no frame
of the window under it. With a namespace KWin does not know (window type Normal), KWin's scale effect fades the overlay
in from opacity 0, and that fade starts at the unlock: the red window under it shows at full strength in the first
frame and fades out over about 170 ms. That is the flash §6.5 forbids, so the namespace is part of the design, not a
detail.

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
- Run with `spikes/greeting-handoff/run.sh`; `SPIKE_NAMESPACE=ghost-greeting-spike` reproduces the fade.

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

### The unlock

Times are from `dbus-monitor` (`ActiveChanged(false)`), the overlay's log and `watch`, all in epoch ms.

| Run | Namespace (window type) | `Unlock` → `ActiveChanged(false)` | Last lock-screen capture → first capture after | Red captures after the unlock | Overlay's next frame after `ActiveChanged(false)` |
|---|---|---|---|---|---|
| 1 | `ghost-greeting-spike` (Normal) | 87 ms | 2 ms, then **red** | 1–21 ms after it, then a fade to ground by +173 ms | 8 ms |
| 2 | `ghost-greeting-spike` (Normal) | 84 ms | 1 ms, then **red** | 1–23 ms after it, then a fade to ground by +158 ms | 9 ms |
| 3 | `dock` (Dock) | 87 ms | 3 ms, then ground | none | 17 ms |
| 4 | `dock` (Dock) | 67 ms | 1 ms, then ground | none | 14 ms |
| 5 | `dock` (Dock) | 82 ms | 1 ms, then ground | none | 5 ms |
| 6 | `dock` (Dock), final committed script | 69 ms | 2 ms, then ground | none | 15 ms |

- **With `dock`, the first scene after the unlock is the overlay**, drawn from the buffer it committed under the lock.
  `watch` never saw red, black or a blend: the lock screen gives way to ground between two captures under 3 ms apart.
- **The delay from `ActiveChanged(false)` to the overlay's next rendered frame is 5–17 ms**, about one refresh. It
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
- **Why `dock` escapes:** `scale` animates only `normalWindow || dialog`. `fadingpopups`
  (`/usr/share/kwin-wayland/effects/fadingpopups/contents/code/main.js:23–58`) animates only popups, outlines, unmanaged
  windows and splash, toolbar, OSD and applet-popup types. A Dock is none of these. (`desktop` would escape too, but
  KWin treats Desktop-type windows specially, and the dashboard already uses that namespace.)
- **The lock screen does not fade out.** It is gone in one step: `scale` skips `window.lockScreen` (`main.js:68–69`),
  and `fadingpopups` blacklists `kscreenlocker_greet` (`main.js:17–18`). Stock `LockScreenUi.qml:55–57` calls
  `Qt.quit()` at once on success, and the greeter then `_exit(0)`s (kscreenlocker `greeter/greeterapp.cpp:330–337`).
  So the overlay needs no state to match a half-faded lock screen. It only has to show what the lock screen's last
  frame showed: lock design §6.5's ground + dots.
- **No black gap.** The greeter's process ends before `doUnlock` runs, but `watch` caught no frame without the lock
  screen or the overlay in any run.

### Keyboard

- **`WlrKeyboardFocus.Exclusive`, asked for after the unlock, takes the keyboard.** In every run, the overlay became
  active 3–16 ms after asking, the red window went inactive within 1 ms of that, and the next key reached the
  overlay (`spike overlay key 65`), not the red window.
- **Before that, KWin gives the keyboard back to the last active window.** The red window became active 2–15 ms after
  `ActiveChanged(false)` and kept the keyboard until the overlay asked. In the spike that was 80–109 ms, because the
  request waits on a 50 ms file poll. A key typed in that window goes to the user's window, not the greeting. Whether
  asking for Exclusive while still locked is safe was not tested: KWin routes every key to the greeter while locked,
  but plan call 7 keeps the armed overlay at `None`.

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

1. **Namespace `dock`** (or another type that the open effects in use skip) on every greeting overlay. The user's live
   KWin runs `kinetic_scale` and both `fadingpopups` and `kinetic_fadingpopups`, with `scale` off. By their source
   (`~/.local/share/kwin/effects/kinetic_scale/contents/code/main.js:44–81`, `kinetic_fadingpopups/contents/code/main.js:23–67`),
   they choose windows by the same rules, so a Dock escapes them too. This was read, not measured live.
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

- Headless KWin with its default effects (a fresh config), one output and no GPU-specific timing. The live session
  differs in its effects (point 1 above) and in having a real display.
- ScreenShot2 renders the scene at the moment of the call, not the frame KWin presented. At 1 700–2 000 captures/s,
  every pass of KWin's event loop around the unlock was sampled. A presented frame is always one of those states.
- Only the centre 64 x 64 was sampled densely. The overlay's opacity is uniform across the surface, and the full
  captures before and after match.
- Multiple screens and the sleep path (`PrepareForSleep`, `LockOnResume`) were not exercised. The visibility rule
  above does not depend on why the screen locked.

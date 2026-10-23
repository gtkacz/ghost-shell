# ghost-shell — Lock and Login Design (Project 2 of 3)

- **Date:** 2026-10-22
- **Status:** design written after brainstorming; implementation plan not yet written
- **Parent design:** [`2026-10-20-ghost-shell-design.md`](2026-10-20-ghost-shell-design.md) (project 1). Its visual language (§5), greeting (§7), configuration (§14) and performance rules (§15) apply here unless this document says otherwise.
- **Reference mockup:** [`mockups/2026-10-22-ghost-shell-lock-directions.html`](mockups/2026-10-22-ghost-shell-lock-directions.html). Open it in a browser. It shows three styles, each in lock and login mode. You can type into it: Enter with anything but `ghost` fails, and `ghost` plays the exit pass.

Items marked **[call]** were decided by the designer (Claude), not the user. The user can override them. Everything else was decided by the user during brainstorming.

---

## 1. Intent

The lock screen and the login screen should look like part of ghost-shell, and should look like **one thing** to each other. Today the login screen is the SDDM theme `glyph`. The user kept it as the best of about twenty themes they tried, but they don't like it. They love how the ghost-shell desktop looks, and they want the same look here.

For the user this is **a password box they get past in two seconds**. It isn't a screen they look at while away. That sets the bar: fast to read, nothing competing with the field, and nothing that costs anything while it sits there.

**Success looks like this:**
- Lock and login both look like the mockup, in the configured style, on the real 2560×1440 screen.
- Typing a password and pressing Enter is as fast as it is today.
- On success, the screen hands over to the §7 greeting with no flash of the desktop or windows in between.
- Switching style is one config setting.

## 2. Scope

- **In:**
  - the KDE lock screen (shown on Meta+L and on wake from sleep);
  - the SDDM login screen (shown after boot and after logging out);
  - three styles, switched by config:
    - **A, Prompt**;
    - **B, Terminal**;
    - **C, ID check**.
- **Build order (user decision):**
  1. Build **A** first, on parts that are shared. B and C then reuse them, so each is mostly a layout.
  2. Then B.
  3. Then C.
- **Out:**
  - project 3 (terminal theme, file manager, GRUB, boot splash);
  - the greeting itself, which is project 1 §7;
  - a lock screen that shows things while you're away: media controls, notification content, or the dashboard.

## 3. The machine (facts the builder needs)

These are on top of project 1 §3.

- **Lock screen:** kscreenlocker 6.7.5.
  - The greeter is `/usr/libexec/kscreenlocker_greet`. It loads its QML (`lockscreenmainscript`) from a Plasma *shell* package. The stock QML lives in `/usr/share/plasma/shells/org.kde.plasma.desktop/contents/lockscreen/` (`LockScreen.qml`, `LockScreenUi.qml`, `MainBlock.qml`, …), not in the look-and-feel package as it did in older Plasma.
  - If the QML fails to load, or is "outdated", the greeter falls back to KDE's built-in lock screen. It never unlocks.
  - `plasmashell` still runs (`--no-respawn`), alongside ghost-shell.
  - **Auto-lock is off** (`kscreenlockerrc`: `Autolock=false`, `Timeout=0`), so locking only happens on Meta+L and on sleep.
- **Login:** SDDM 0.21.0.
  - The theme is set in `/etc/sddm.conf.d/kde_settings.conf` (`[Theme] Current=glyph`). Themes live in `/usr/share/sddm/themes/`.
  - The greeter runs as the `sddm` user, so it **cannot read anything in the user's home**: not the config, and not the fonts.
- **Fonts:** Kode Mono is installed only per-user (`~/.local/share/fonts/ghost-shell/`). Noto Sans CJK JP is installed system-wide.
- **Authentication:**
  - one user, `gtkacz` (UID 1000);
  - `fprintd` is installed, but there's no fingerprint device, so it's password only today;
  - keyboard layouts `us` and `br` are both configured.

## 4. What the user decided

- Restyle **both** lock and login, and make them look like one thing.
- It's a **password box**, not something to look at while away.
- Build **all three** mocked styles, chosen with a config setting. A comes first.
- Do it in a modular way: the styles share their parts.

They saw the mockup and chose to build all three as shown. Where this document describes behaviour from the mockup, the mockup is the source of truth for look and motion.

## 5. Visual language

Exactly project 1 §5:
- palette tokens;
- Kode Mono (or the configured font) with the CJK fallback;
- the dot grid on `ground`;
- faint global scanlines;
- chamfered panels with Grid brackets;
- segmented cells;
- colour only for attention;
- the Japanese setting;
- drawn icons only, with chevrons for arrows.

The design scale is the same too: the mockup is at 1280×720, so native is ×2 physical px, derived from the screen at runtime.

**Anti-goals carried over:** glitch effects, bloom or glow, smooth fades on alert text, and Unicode glyphs as icons.

There's **no wallpaper**: the ground and the dot grid are the background, as on the desktop.

## 6. Shared behaviour (all styles)

These come from the mockup unless marked.

### 6.1 The password field

- **One lit cell per character**, in a row of segmented cells (mockup: 9×14 cells, 3 px gaps, 28 cells, unlit cells at ~6 %). The length shows; the characters never do. Nothing else echoes the password, not even a reveal toggle **[call]**.
- **[call]** Past 28 characters, every cell stays lit and the cursor stays on the last cell.
- A **block cursor** in `signal`. It blinks exactly like the boot cursor: 1 s, held steps of full → half → off → half.
- The field always has keyboard focus. Clicking anywhere focuses it again.
- **Keys:**
  - **Enter** submits;
  - **Esc** clears;
  - **Backspace** deletes.
  - **[call]** There's no on-screen Unlock button. Enter is the only way to submit, as in the mockup.
- **Busy:** while authentication runs, the cursor is hidden and typing is ignored.

### 6.2 Failure

- The field clears.
- A **boxed crit alert** appears under the field: `ACCESS DENIED · ATTEMPT n`.
  - It's the §5.4 alert box: a 3 px crit left edge, stepped scrolling stripes, the stepped arrival flicker, then the cursor blink.
  - **[call]** Unlike the dashboard's alerts, it blinks only for its arrival plus about 8 s, then holds still.
- Typing again hides the alert.
- **[call]** No shake and no other motion on failure.
- If PAM reports a message (for example a lockout after too many attempts, or "account expired"), it replaces the alert text, still in crit.

### 6.3 Chips

- **Keyboard layout:** a chip with a drawn keyboard icon and the current layout (`US` / `BR`). Clicking it cycles through the configured layouts. It always shows, because a password typed in the wrong layout is the commonest way to fail with two layouts.
  - **At login:** system-wide, only `us` is set today (`localectl`). `br` exists only in the user's `~/.config/kxkbrc`. The SDDM greeter runs under its own KWin and gets only the system layout. **[call]** The installer offers to set the system layouts to `us,br` once (`localectl set-x11-keymap us,br`, with sudo), and asks first like the other system changes (§10). How the login screen then switches layouts on Wayland (SDDM's `keyboard` object, or its KWin) is spike 5. If the user declines, or switching can't work, the login chip shows the one layout and doesn't cycle.
- **Caps Lock:** a `CAPS LOCK` chip in `warn`, shown only while Caps Lock is on.
- **Session** (login only): a chip showing the session that will start, e.g. `PLASMA (WAYLAND)`, with a chevron. Clicking it opens a small list in the panel style. **[call]** The default is the last session used, as SDDM does.

### 6.4 Actions

These are a row of drawn icon + label buttons in `dim`; hovering turns them `hi`.

| Mode | Actions |
|---|---|
| Lock | Switch user, Sleep |
| Login | Sleep, Restart, Shut down |

**[call]** An action only shows if the backend says it's possible (e.g. Switch user needs a display manager that supports it). There's no confirmation dialog for login-screen Restart or Shut down, which matches SDDM's own themes.

### 6.5 Success and the hand-off

- On success, **one downward scan pass** (≈0.8 s, 30 held steps, a 2 px light line) erases the screen to the bare dot grid.
- Then the lock screen exits, or the session starts.
- Project 1 §7 then takes over:
  - after **unlock**, the greeting plays full-screen over the open windows;
  - after **login**, it plays when the session starts.
- **No frame of the desktop or windows may show between the lock screen and the greeting.** Plan 2c has built the project 1 side; see the hand-off section of `docs/superpowers/notes/2026-10-22-plan2c-acceptance.md`.
  - **What the lock screen can rely on:**
    - from the moment of locking, the greeting's overlay is mapped on every screen, under the lock, resting on `ground` + the full dot grid;
    - the overlay takes no keyboard while locked;
    - KWin shows the overlay's frame first on unlock.
  - **What it must do:**
    - its exit ends on that same ground and dots, in the same place, so the switch to the overlay can't be seen;
    - the lock has to keep `org.freedesktop.ScreenSaver`'s `ActiveChanged` true while it's up and false on exit, as kscreenlocker already does. If it doesn't, the greeting neither arms nor plays.
- **[call]** The scan pass ends on bare `ground` + dots, and the greeting's tube power-on starts from that. Together they read as one sequence.
- **[call] At login, there's no pass.** SDDM closes its greeter as soon as login succeeds, and holding login for 0.8 s would break "as fast as today". The greeting's own power-on opens the session.
- **[call] KDE's login splash (KSplash):** it's still active (`~/.config/ksplashrc` is empty) and sits between SDDM and the greeting. At acceptance, log in once and check whether it shows. If it does, **ask the user** before switching it off. That's the same rule Plan 2c set.

### 6.6 Screens

- **[call]** The prompt appears only on the primary screen (on this machine, the M27Q, matched by model as in project 1).
- Any other screen shows the bare dot grid.

### 6.7 Japanese

Panel tags follow the Japanese setting: `認証` for A's AUTHENTICATE and `照合` for C's IDENTIFY. With `off`, they're hidden.

## 7. The three styles

Every style shows the same parts from §6. Only the arrangement differs.

### 7.1 A, Prompt (build first)

- **Just the box.** It's one chamfered panel, 380 wide at mockup scale, centred on the dot grid. Nothing else is on screen except the action row near the bottom centre.
- **Panel header:**
  - the label `AUTHENTICATE` with its tag `認証`;
  - in lock mode, the right-aligned meta reads `LOCKED HH:MM`, with the live clock and no seconds;
  - in login mode, the meta is the clock alone **[call]**.
- **Body, in order:**
  1. the user name in `signal`, bold, at about 18 px, followed by a dim small tag: `GHOST-SHELL` (the host name, **[call]**) when locked, or `PLASMA · WAYLAND` (the session) at login;
  2. the password field;
  3. the failure alert, when there is one;
  4. a chip row with the layout, Caps Lock and, at login, the session chip on the right.
- There's no big clock.

### 7.2 B, Terminal

- **No panel.** It looks like a TTY, left-aligned at about x=108, y=178 (mockup scale).
- **Content, in order:**
  1. the dashboard's 118 px hero clock with teal superscript seconds;
  2. a date line `DAY · DD.MM.YY · CITY UTC±N`;
  3. two terminal lines:
     - `ghost-shell locked · gtkacz` or `ghost-shell login: gtkacz`;
     - `password:` followed by the cells;
  4. the alert, below the lines.
- A foot row along the bottom holds the chips on the left and the actions on the right.

### 7.3 C, ID check

- **The clock**, at 74 px, and the date line sit centred above the card.
- **The card** is a 620×214 chamfered panel split into two columns:
  - **Identity column (196 wide):**
    - a chamfered avatar drawn as a **dot halftone** in `signal`: the user's face image (`~/.face.icon`, or the AccountsService icon) rendered as dots, or a generic silhouette if there isn't one **[call]**;
    - the name in caps;
    - a key/value list: `UID`, `HOST`, and while locked `SESSION`.
  - **Auth column:**
    - the header `IDENTIFY` with its tag `照合`, and the meta `LOCKED` or `NEW SESSION`;
    - a `PASSPHRASE` label;
    - the field;
    - the alert;
    - the chips.
    - While locked, a bottom line also reads `AWAY HH:MM:SS · SIGNALS n NEW · MEDIA <state>`.
- **The "away" line shows counts and states only, never content**, because the lock screen is visible to anyone at the desk.
  - `AWAY` counts up live from the moment of locking.
  - `SIGNALS` is the number of unread notifications that arrived while locked.
  - `MEDIA` is the MPRIS state.
  - How the lock screen gets these is left to C's own plan. One option is a small file ghostd keeps current, with counts only. If a value isn't available, its item is hidden.
  - These items never appear on the login screen.

## 8. Architecture and modularity

### 8.1 Keep KDE's locker and SDDM [call]

- Security stays with the systems built for it:
  - kscreenlocker still owns locking, and it keeps the screen locked if the greeter crashes;
  - SDDM still owns login;
  - PAM still does authentication.
- ghost-shell supplies only the **QML that draws them**. It never implements its own locker. On this KWin, a Quickshell session lock isn't the route.
- If ghost-shell's QML fails, the fallback must be "KDE's stock screen" or "SDDM's default", never "unlocked".

### 8.2 One shared module, two thin adapters, one file per style

- **`GhostGate`** (working name) is a QML module holding the gate's parts:
  - the password cells and cursor;
  - the alert box;
  - the chips: layout, Caps Lock, session;
  - the action row and its drawn icons;
  - the clock and date line;
  - the scan pass;
  - screen setup: ground, dots, scanlines, scale.
- **Styles** are one QML file each: `Prompt.qml`, `Terminal.qml`, `IdCheck.qml`.
  - Each only arranges GhostGate parts and reads the backend through the adapter.
  - Adding B or C later shouldn't touch A's parts beyond moving genuinely shared things into the module.
  - **[call]** Styles are a closed set for now. Third-party lock styles aren't part of the API, but nothing should prevent adding them later, in the spirit of project 1 §13.
- **One adapter per backend** maps that backend's API onto a small common interface the styles use. That interface has:
  - **properties:** user name, mode (lock/login), busy, attempt count, last message, Caps Lock, layouts plus the current layout, sessions plus the selected session, and which actions are allowed;
  - **calls:** submit(password), next layout, choose session, and each action;
  - **events:** succeeded and failed.

  The two adapters:
  - **kscreenlocker adapter:** the greeter's `authenticator` and its succeeded/failed signals, the switch-user and suspend calls, and keyboard state;
  - **SDDM adapter:** `sddm.login(user, password, session)`, `loginSucceeded`/`loginFailed`, `sddm.suspend`/`reboot`/`powerOff`, `userModel`, `sessionModel`, and `keyboard.layouts`.
- **Styles never talk to a backend directly**, so each style works in both modes for free. The style can be tested with a fake adapter, headlessly.

### 8.3 Sharing with GhostKit

The greeters are plain Qt Quick processes, not Quickshell. Today some of GhostKit's pieces import `Quickshell`:
- `Theme`, `DotGrid`, `Scanlines`, `AlertBox` and `FrameClock` do;
- `Panel`, `Chip`, `Digits`, `Glyph`, `SegMeter` and `lib/*.mjs` don't.

**Rules:**
- **Nothing a greeter loads may import Quickshell.**
- **The visual primitives have one source of truth.** Panel shape, chips, alert stripes and blink, dot grid, scanlines and palette must not be forked into a second copy that drifts. For example, split GhostKit's Quickshell-free core into a module both sides import.
- Project 1's desktop must keep working unchanged, and its tests must still pass.

The builder picks the mechanics.

### 8.4 Performance

The idle lock screen animates only:
- the cursor blink (4 steps/s);
- for a few seconds after a failure, the alert's stripes and blink;
- in C, the 1 Hz clock and away timer.

**It must not follow the display's 170 Hz.** Use stepped timers, not continuous animations, as in project 1 §15. **[call]** Target: the idle greeter costs ≤ 1 % of one core averaged over 60 s, measured the same way as project 1.

## 9. Configuration

- **One new setting**, in project 1's config file:

  ```toml
  [lock]
  style = "prompt"   # prompt | terminal | idcheck
  ```

  - The default is `prompt`. An unknown value falls back to `prompt`, and ghostd logs a warning, as for any other invalid config.
  - **[call]** No other lock-specific knobs until a need appears.
- The lock and login screens also follow these existing settings:
  - the font and CJK fallback;
  - every colour token;
  - panel fill %;
  - the dot grid and scanline settings;
  - the Japanese setting.
- **Getting the config to the greeters [call]:**
  - ghostd already parses and validates the config. On every successful (re)load, it writes a **resolved snapshot** containing only what the gates need: style, appearance and Japanese mode. It holds no secrets and no notification content.
  - The lock greeter runs as the user, so it reads the snapshot from the user's state or runtime directory. It picks up changes the next time it locks.
  - SDDM can't read the user's home, so ghostd also writes the snapshot to a **system path that the installer creates once, with sudo**, owned by the user and world-readable (e.g. `/var/lib/ghost-shell/gate.json`). The login screen picks it up the next time it's shown. **Changing style never needs sudo after the first install.**
  - If the snapshot is missing or unreadable, the gates use the built-in defaults (style A, default palette). The gates must never fail to show because of it.

## 10. Installation and migration [call]

- **Optional for other ghost-shell users (user decision, 2026-10-23).** Lock and login are two separate **opt-in** parts of the install, for example `install.sh --lock --login`.
  - A plain install of ghost-shell leaves the user's lock screen, SDDM theme, system fonts and keymap untouched.
  - Either part can be installed or removed without the other, and without affecting the desktop shell.
  - Removing a part restores what was there before it.
  - This machine's install opts into both.

- Install Kode Mono, and Chakra Petch Slashed if it's configured, **system-wide** so the `sddm` user can use them (e.g. `/usr/local/share/fonts/ghost-shell/`).
- **SDDM:**
  - install the theme to `/usr/share/sddm/themes/ghost-shell/` with the Qt 6 greeter declared in its metadata;
  - switch to it with a drop-in in `/etc/sddm.conf.d/`, leaving `kde_settings.conf` untouched, so removing the drop-in returns to `glyph`.
- **Lock screen:**
  - point kscreenlocker at ghost-shell's lock QML **without editing files under `/usr/share/plasma`**;
  - an uninstall must restore the stock lock screen.
- **Ask the user before switching either one**, because these are the screens they need to get back in. Before switching, prove each works:
  - test the lock greeter in its testing mode;
  - test the SDDM theme in `sddm-greeter-qt6 --test-mode`.

  Both tests run in a headless KWin, never on the user's display (the project's standing rule).
- Don't delete `glyph` or the other installed SDDM themes.

## 11. Spikes first

These answers change the plan, so get them before building:

1. **How kscreenlocker 6.7 picks its lock QML,** and the cleanest supported way to point it at ours: a custom shell package, a config key, or something else. Also what makes a lock QML "outdated" (the version check it falls back on).
2. **What the lock greeter's sandbox allows:** reading the snapshot file, and which QML imports are available.
3. **That ghost-shell's lock QML ends on the overlay's frame (§6.5)** with no visible change at unlock, on the real lock path.
4. **Rendering A headlessly in both greeters** with the real backends (a wrong password shows the alert; the right one succeeds).
5. **Switching layouts in SDDM 0.21 on Wayland** (§6.3).

## 12. Non-goals and anti-goals

- **Non-goals:**
  - media controls, notification content, or widgets on the lock screen. These are things stock Plasma's lock screen has (it shows MPRIS controls), and they're dropped on purpose **[call]**; C's away line only shows counts.
  - a virtual-keyboard button;
  - user avatars at login beyond C's halftone;
  - a user list. **[call]** With one user, the name is preselected. If more users exist, the name becomes a chip that cycles through them.
  - fingerprint or smartcard UI. There's no reader today. **[call]** If PAM prompts for something other than a password, show its message in the alert line, so it's at least not silent.
- **Anti-goals (failure even if it works):**
  - any path where a crash or a bad config leaves the session unlocked;
  - a flash of the desktop or windows between lock and greeting;
  - echoing the password's characters;
  - visible idle CPU/GPU cost;
  - a style that only works in one mode (lock or login);
  - forked copies of GhostKit's primitives;
  - everything in project 1 §16: glitch, glow, smooth alert fades, glyph icons, overflowing text.

## 13. Left to the builder

- How kscreenlocker is pointed at the QML (after spike 1).
- Splitting GhostKit (§8.3) and naming the module.
- The adapter interface's exact shape.
- The snapshot format and paths.
- Packaging and install steps.
- Test strategy, including headless greeter tests.

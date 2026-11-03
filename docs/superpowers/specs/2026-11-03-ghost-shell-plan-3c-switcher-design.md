# ghost-shell — Plan 3c Design: the Alt+Tab Switcher

- **Date:** 2026-11-03
- **Status:** design written after brainstorming; implementation plan not yet written
- **Parent design:** [`2026-10-20-ghost-shell-design.md`](2026-10-20-ghost-shell-design.md), §4 (stack), §10 (switcher), §15 (performance), §16 (anti-goals). This document replaces §10's grid with the layout below; the rest of §10 still holds.
- **Reference mockup:** [`mockups/2026-11-03-ghost-shell-switcher-layouts.html`](mockups/2026-11-03-ghost-shell-switcher-layouts.html). Open it in a browser and pick `C READOUT`. Hold `Z` as Alt and tap `X` as Tab. Its window contents are drawn stand-ins and its timing is the browser's.
- **Machine at writing:** Fedora 44, Plasma and KWin 6.8.2 on Wayland, one 2560×1440 display, NVIDIA.

Items marked **[call]** were decided by the designer, not the user, and the user can override them. Everything else was decided by the user, here or in the parent design.

---

## 1. Intent

Alt+Tab is the last stock KDE surface the user sees many times a day. The user switches windows from the bar and with Alt+Tab about equally, so this has to be first-class, not a reskin they tolerate.

They use Alt+Tab two ways, almost equally, roughly 60/40:

1. **Flicking** back to the last window. Nothing should appear.
2. **Holding Alt** and looking through the windows for one. This should be quick to read and should look like the rest of ghost-shell.

## 2. Scope

Plan 3c was scoped as the switcher plus three audio items parked by Plan 2b. They are unrelated, so they are split:

- **This design:** the switcher.
- **After it, as a separate small change:** split `widgets/media/Widget.qml` (788 lines), level meters beyond the default mic, and capture streams that count as "recording" forever (ledger items M9, M4 and C24).

## 3. Behaviour

- **Flick:** an Alt+Tab released before KWin's show delay goes straight to the last window and draws nothing. **[call]** The delay stays at KWin's default of 90 ms, so `[TabBox] DelayTime` is left unset. The mockup's `DRAW AFTER` control exists for trying other values, and it does not ship.
- **Hold:** the switcher appears after the delay and stays until Alt is released, which focuses the selected window. Tab steps forward and Shift+Tab steps back.
- **Selection on opening:** the previous window, which KWin already does.
- **Which windows, in what order:** whatever KWin's switcher gives, most recently used first, including minimised windows.
- **Other shortcuts:** `Meta+Tab` and the current-application walk (`Alt+\``, bound on this machine) read `[TabBox]`, so they get this layout too, the walk with a shorter list. **[call]** `[TabBoxAlternative]` is left alone: its shortcuts are unbound here and `kwinrc` has no such group.
- **[call] Esc** cancels and stays on the current window. KWin already does this.
- **[call] Clicking a row** focuses that window, through `tabBox.model.activate(index)`.
- **The desktop stays still while browsing.** KWin's "Show selected window" (`[TabBox] HighlightWindows`) is turned off, so the windows behind the panel do not change and the preview is the only picture of the selection. This is the user's decision. The setting defaults to on, and the user's config does not set it, so the install has to write it.

## 4. Layout

Layout C, "Readout", as in the mockup. Sizes below are the mockup's, where 1 mockup px is 2 physical px at 2560×1440. They scale with the design unit as the shell's other surfaces do.

- **Backdrop:** the whole screen, bar included, dims to about 60 % (`rgba(5, 8, 9, .6)` in the October reference).
- **Panel:** centred and chamfered, with Grid corner brackets, about 950 × 390.
- **Header:** `SWITCH WINDOW` and `窓切替` on the left, `n / N` on the right. The tag follows the `japanese` setting and disappears when it is off. Only CJK characters use the CJK font.
- **Left, the list:** about 366 wide, with rows about 30 high, showing number (`01`), app icon, app name and window title. Long titles elide on the right.
- **Right, the preview:** about 528 wide, one live thumbnail of the selected window, with its app name and title beneath. The window keeps its proportions inside the box.
- **Selected row:** signal-colour border, Grid brackets, faint signal fill, bright icon and bright text, as §10 asked of the selected card.
- **[call] More than 10 windows:** the list scrolls to keep the selection visible, with a drawn chevron at the end that has more. The panel does not resize.
- **[call] Fewer than 10 windows:** rows stay at the top and the panel keeps its size, so the preview never moves.
- **[call] Opening:** a top-down print of about 110 ms in 6 steps, as in the mockup. Closing is instant. There are no smooth fades.
- **[call] No text scramble:** the shell scrambles a text on its first write in each showing, but here the names are being read at that very moment, so they appear plain.
- **Colours and fonts:** every colour, the font and the panel look come from the user's config (§5). The mockup's scanlines also run over the preview; **[call]** the builder may keep them off the real window if that costs less.

## 5. How it is made

### 5.1 A KWin layout

As the parent design's §4 says, this is a KWin TabBox layout in QML, not a Quickshell surface. It renders inside KWin and gets live thumbnails from `KWin.WindowThumbnail`. It also keeps working when `ghostd` or the shell is down.

Checked on the machine and in KWin's `Plasma/6.8` source:

- A layout is a package of type `KWin/WindowSwitcher` in `~/.local/share/kwin/tabbox/`, with `KWin.TabBoxSwitcher` as its root.
- The stock `coverswitch` layout uses a plain full-screen `Window` with `Qt.ToolTip | Qt.BypassWindowManagerHint | Qt.FramelessWindowHint`. That is the route for the backdrop, with no `PlasmaCore.Dialog` and no Plasma styling.
- KWin caches the loaded layout object for the session, so anything the layout creates must exist only while `tabBox.visible` is true.
- If a package cannot be found, KWin logs it and falls back to its default layout.

### 5.2 Colours and fonts: the lock screen's route

KWin cannot reach `ghostd`, exactly like the lock screen, so **[call]** the layout reads the same snapshot, `gate.ini` in `$XDG_STATE_HOME/ghost-shell/`, through `QtCore`'s `Settings` and `GateLib.readSnapshot`, as `packaging/lock/org.ghostshell.lock/contents/lockscreen/LockScreen.qml` does.

- It reuses the gate's `Theme` and shared files through `scripts/assemble-gate.sh` instead of keeping another copy of the palette.
- It re-reads the snapshot on each opening, so a config change shows at the next Alt+Tab with no KWin restart.
- Sizes come from the screen, as on the lock screen: `Theme.fit()` with `tabBox.screenGeometry.width`, which needs no snapshot key.
- A missing or unreadable snapshot means the gate's built-in defaults (`GateLib.parseSnapshot({})`), with no second set of defaults in the layout. The switcher must always open.
- **Palette presets are in progress** (`2026-11-03-color-palettes-design.md`, uncommitted at writing, from other work). They resolve to full colour tokens in the same snapshot, so the switcher follows them with no extra work. Build on them if they have landed.

### 5.3 Icons

- **[call]** The layout follows the bar's `bar.icons` setting, mono or colour. The user's is mono.
- `gate.ini` does not carry that setting yet, so `ghostd` adds one key to the snapshot. Follow the file's existing versioning.
- `model.icon` is a `QIcon`, so it needs `Kirigami.Icon` as the stock layouts use, with the tint applied as `shell/kit/GhostKit/AppIcon.qml` does through `Duotone`.

### 5.4 The app name: not given by KWin

KWin's switcher model offers `caption`, `desktopName`, `minimized`, `windowId`, `closeable` and `icon`. There is **no app name**, and the list needs one.

`windowId` is a window's `internalId`, and KWin's `Workspace` exposes windows with `desktopFileName` and `resourceClass`, so the window can be looked up. How to reach a friendly name from there is left to the builder. Raw classes such as `com.mitchellh.ghostty` or `thorium-browser` must not reach the screen as they are, and the column must never be empty.

**[call]** When a caption ends with the app's name (`Toast.qml — ghost-shell — Visual Studio Code`), the title drops that ending, so the row does not say it twice.

### 5.5 Install

**[call]** An opt-in `scripts/install.sh --switcher` and `--remove-switcher`, following `--lock` and `--remove-lock`, because it changes KWin's settings:

- `--switcher` assembles and installs the package, records the previous `[TabBox] LayoutName` and `HighlightWindows` (including "not set"), writes the new values to `kwinrc`, and asks KWin to reconfigure.
- `--remove-switcher` puts both back exactly, deletes keys that were not set before, removes the package and reconfigures.
- Stage the package before replacing it, so a failed build keeps the working one. The audit found this hazard in `--lock` (finding 5), and this code must not copy it.

The package source goes in `packaging/switcher/`, beside `packaging/lock/`.

### 5.6 If it goes wrong

The layout runs inside KWin's process, so unlike the bar or the dashboard, a hang or crash there can stop the whole screen. KDE's own layouts run the same way, but this one adds ghost-shell's shaders and panel code. KWin's fallback (§5.1) only covers a missing package.

Facts this rests on, from this machine and KWin's `Plasma/6.8` source:

- KWin creates a layout at the first `show()`, so a session is safe until the first hold (proof 9 in §6 confirms it).
- KWin itself handles the switch to a text console (`VirtualTerminalFilter`), so a hung KWin blocks `Ctrl+Alt+F3` too. A console only helps when KWin is still responding.
- `kernel.sysrq` is 1 here, so `Alt+SysRq` then `S`, `U`, `B` still reboots cleanly. `sshd` is not running.

The way back:

1. **KWin hung:** reboot with `Alt+SysRq` then `S`, `U`, `B`, or hold the power button.
2. **Then:** log in as usual, do not hold Alt+Tab, and run `scripts/install.sh --remove-switcher`.
3. **KWin still responding:** the same command from a text console.

**[call]** So that this holds when it is needed:

- `--remove-switcher` works with no display and no session bus, and a hung KWin cannot hang it: the `reconfigure` call has a timeout, and failing only warns.
- The README states this way back beside the install.
- Before the first opening on the real GPU, the user rehearses `--remove-switcher` from a real text console. A headless KWin cannot stand in for NVIDIA, so that first opening is the risky moment, and the user saves their work first.

## 6. Prove first

Each of these rests on reading, not running. Prove them in a headless KWin 6.8.2 (`scripts/headless-kwin.sh`) before building the panel. **[call]** The plan makes this its first, standalone task, as Plans 3a and 3b did with their spikes, and no later task starts until it passes. **If one fails, stop and bring it back to the user instead of redesigning around it.**

1. A layout with a full-screen `Window` and one `WindowThumbnail` opens over a layer-shell bar and shows a live window.
2. A flick draws nothing: no frame of the layout reaches the screen when Alt is released inside the delay. `scripts/fake-keys.py` presses and releases one key per line, so it needs a way to hold Alt.
3. The user's `kinetic_fadingpopups`, `kinetic_scale` and `kinetic_squash` effects do not fade or scale the switcher. They have touched ghost-shell's surfaces before (see the Plan 3a ledger on namespaces). If they do, the opening stops being the print.
4. `Settings` reads `gate.ini` inside KWin's QML engine and can re-read it on each opening.
5. The gate's `Theme`, its shaders and `Duotone` load in KWin's engine, which is shared with every KWin script.
6. `windowId` finds its window through `Workspace`, and a friendly name is reachable (§5.4).
7. What reinstalling a layout takes when KWin has cached it: a reconfigure, or the next login.
8. A broken layout (a QML error at load, an error when it opens) leaves KWin running and Alt+Tab still switching windows.
9. KWin does not load the layout until the first hold: not at start, and not on a flick.

## 7. Performance

Parent §15 applies. Measure on the real machine and record every reading in the README's "Performance" section.

- **Closed:** no measurable KWin CPU over the stock layout, and nothing created. Record KWin's memory before and after a first opening.
- **Open:** repaints only on a selection change, apart from the live thumbnail. Compare KWin's CPU with the stock layout held open on the same windows.
- **First and later openings:** time from key press to first frame for each. The first loads the QML, as stock layouts do. If it is noticeably slower than stock, bring that back to the user.

No numeric targets are set, because no baseline exists yet. Stock is the baseline.

## 8. Testing

- Logic that can live in `lib/*.mjs` (app-name cleanup, title trimming, which rows show) gets node tests, as the rest of the shell does.
- A headless smoke in the style of `scripts/smoke-lock.sh`: install into a sandbox, open the switcher with fake keys, step, release, and assert the focused window, plus the flick from §6.2.
- A sandbox test that `--switcher` then `--remove-switcher` restores `kwinrc` exactly, both when the keys were set and when they were not.
- Agents never run this on the user's display. The live install happens with the user present, with these acceptance checks: flick, browse, Shift back, Esc, click, more than 10 windows, a minimised window, a palette change, and `--remove-switcher`.

## 9. Non-goals and anti-goals

**Non-goals, each left out on purpose:**

- Layouts A (grid) and B (strip), and a setting to choose between styles. They are noted for later in `docs/IDEAS.md`. **[call]** The setting arrives with the second style, because a setting nothing reads is what the audit flagged in `launcher.style` (finding 20).
- Live window previews on hovering a bar task (`docs/IDEAS.md`). KWin 6.8.2 still has no `ext-image-copy-capture`, so that remains its own project.
- **[call]** Close buttons on rows, search, filters, arrow keys and drag.
- Any setting for the show delay, or for "Show selected window".
- Virtual desktops and secondary monitors, as in the parent §16.

**Anti-goals, a failure even if everything else works:**

- Any flash on a quick flick.
- Feeling slower than KDE's own switcher.
- Alt+Tab failing to work because of ghost-shell. A broken layout must leave window switching working.
- Smooth fades, bloom, glow or glitch effects, Unicode glyphs as icons, and Latin text in the CJK face.

## 10. Left to the builder

- Exact sizes, spacing and the design-unit mapping, staying close to the mockup.
- Whether the layout imports the gate's module or a smaller assembled set.
- The route to the friendly app name, within §5.4.
- What the preview shows when a thumbnail is unavailable, such as the app's large icon.
- The snapshot key's name and the package's id.

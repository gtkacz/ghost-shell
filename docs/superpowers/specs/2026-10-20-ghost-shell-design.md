# ghost-shell — Desktop Shell Design (Project 1 of 3)

- **Date:** 2026-10-20
- **Status:** design written after brainstorming; implementation plan not yet written
- **Reference mockup:** [`mockups/2026-10-20-ghost-shell-reference.html`](mockups/2026-10-20-ghost-shell-reference.html). It's an interactive, self-contained page: open it in a browser. All values in it are simulated, but every sensor, drive, interface, app and process name is real and comes from this machine.

Items marked **[call]** were decided by the designer (Claude), not the user. The user can override them. Everything else was decided by the user during brainstorming.

---

## 1. Intent

The user wants their desktop to feel like a terminal from *Tron: Legacy*, *Cyberpunk 2077* or *Ghost in the Shell*. Visually it draws on [eDEX-UI](https://github.com/GitSquared/edex-ui) and [wrayth](https://github.com/bowenbride/wrayth), but it is built from scratch and performance comes first. The desktop itself becomes a live dashboard of the machine (usage, temperatures, fans, I/O, network) and of the outside world (time, weather and INMET alerts, media, notifications), with some purely decorative pieces (an uplink globe, a process tree, a silicon schematic).

The user arrived without a concrete picture and shaped everything by reacting to mockups. When in doubt, the mockup is the source of truth for look and behaviour, and this document is the source of truth for intent and rules.

**Success looks like this:**
- It looks like the reference mockup on the real 2560×1440 screen.
- It feels instant.
- It costs close to nothing while idle or covered by windows.
- Everything the user might want to tweak lives in one config file.

## 2. Scope and order

| # | Project | Status |
|---|---------|--------|
| **1** | **Desktop shell** (this document): dashboard on the desktop layer with three views, boot/unlock/wake greeting, top bar (+ optional dock), launcher, Alt+Tab switcher, notification server, popups and history, media, config file | designing now |
| 2 | Lock screen restyle | next, immediately after 1 |
| 3 | Terminal theme, file manager, GRUB theme, boot splash / boot video | after 2 |

Projects 2 and 3 each get their own brainstorm and design. They are **out of scope** here, but nothing in project 1 should make them harder.

## 3. The machine (facts the builder needs)

- **OS:** Fedora 44, kernel 7.2.11, **KDE Plasma 6 on Wayland**.
- **Display:** one Gigabyte M27Q, 2560×1440 on DP-1. The connector name has drifted before (DP-4 → DP-1), so match outputs by model, not connector.
- **CPU:** AMD Ryzen 9 9950X3D, 16 cores / 32 threads. **CCD0 has the 3D V-Cache** and owns logical CPUs 0–7 and their SMT siblings 16–23. **CCD1** ("frequency" die) owns 8–15 and 24–31. Each CCD has 32 MB L3; CCD0 adds 64 MB of V-Cache. There's an I/O die with a Radeon 610M iGPU.
- **GPU:** NVIDIA RTX 4080 SUPER, 16 GB, proprietary driver 615.78.08 (NVML available).
- **Memory:** 96 GB DDR5 (91.9 GiB usable), 8 GiB swap.
- **Drives** (identify by model/serial; **kernel `nvmeX` numbering changes between boots**):
  - XPG GAMMIX S70 BLADE 3.7T: btrfs, `/` and `/home`.
  - Kingston SKC2500 250G: Windows NTFS, automounted under `/run/media/gtkacz/…`.
  - WD Green WDS960G2G0C 960G: **controller state `dead`**, reports 0 B. It must show as a critical drive state, not crash the panel.
  - WD Blue 1T (`sda`) and Seagate SkyHawk 3T (`sdb`): usually unmounted.
- **Sensors (hwmon):**
  - `k10temp`: Tctl, Tccd1, Tccd2.
  - `nct6799`: fan2 = CPU fan (~1670 rpm), fan3 = chassis fan (~450 rpm), SYSTIN/CPUTIN/AUXTIN temperatures.
  - NVMe composite temperatures.
  - `spd5118` ×2: DIMM temperatures.
  - `r8169`: NIC temperature.
  - `ps_controller_battery_*`: DualSense battery.
  - Reading one NVMe temperature currently returns an I/O error (the dead drive); this must be handled.
- **Network:**
  - `enp14s0`: 1 GbE, 192.168.15.5/24.
  - `tailscale0`: 100.122.159.93, 1 peer.
  - NetworkManager OpenVPN connection "OpenVPN Flxpoint" (`tun0`).
- **Location:** São Paulo (`America/Sao_Paulo`, IBGE geocode `3550308`). Units: °C, 24-hour clock, dates as `DD.MM.YY`.
- **Today's desktop:**
  - A floating bottom Plasma panel: Kickoff, an icon-only task manager with 11 pinned launchers (§9), system tray, digital clock, and a pager the user doesn't use.
  - Plasma notifications.
  - KRunner bound to Search, Alt+Space, Alt+F2 and the user's custom **Meta+S**.
- **Existing pieces that overlap this project** (throwaway proof-of-concepts; don't build on them):
  - `~/.config/quickshell/gridlock`: a Quickshell HUD on Plasma's desktop layer, plus the KWin script `gridlock-keeper`.
  - `~/.config/conky*`.
  - `~/.local/bin/dash` and the autostart entry `~/.config/autostart/dash-restore.desktop`, which switch between them.
  - `~/scripts/storm_alert.py` plus `storm-alert.timer`, which polls INMET every 10 min and calls `notify-send`. This is the user's own INMET integration (see §12).

  Installing ghost-shell must cleanly disable these. **Ask the user before deleting any of them.**

## 4. Platform and stack

- **[call] Stay on KDE Plasma 6 / KWin.** KWin stays the compositor and window manager. ghost-shell takes over these roles from `plasmashell`: **desktop/wallpaper, panel, notification server, and Kickoff**. KRunner and all of its shortcuts stay exactly as they are.
- **Nothing may silently regress when Plasma's shell stops providing it.** These are the functions in use today:
  - clipboard history and its **Meta+V** popup (Plasma 6's clipboard lives in plasmashell);
  - the volume, brightness, media and **keyboard-layout OSDs** (layouts `us` and `br` are both configured);
  - system-tray hosting (the StatusNotifierItem watcher and host, used by KDE Connect, Steam, Discord, etc.);
  - the tray applets: network, Bluetooth, audio, keyboard layout, Disks & Devices, Brightness & Night Light, camera-in-use, device batteries (DualSense, phone), printers, vaults, input method, display configuration;
  - from Kickoff: power and session actions, and recent files (§9);
  - from notifications: Do Not Disturb, suppression while a fullscreen app is running, and file-transfer progress (§11);
  - from the task manager: Meta+1…0, unread badges and progress on icons, and right-click jump lists (§8).

  Polkit and the screen locker run outside plasmashell and should be unaffected; verify that. The builder chooses how to keep each function: re-implement it, keep a stripped plasmashell, or run a standalone helper.
- **Tray menus aren't restyled in project 1.** The icons in the bar are restyled. The menus and popups behind them may be KDE's standard ones for now. Restyling them is planned later work (§17), so don't make choices that would make it hard.
- **[call] Suggested stack**, open to change if the builder finds better:
  - **UI:** [Quickshell](https://quickshell.org) (Qt 6 Quick: C++ with QML, GPU scene graph, wlr-layer-shell; already proven to run on this KWin by the gridlock POC).
    - Heavy renderers (block graphs, globe, silicon schematic, process tree) as C++ scene-graph items or shaders, not JS canvas.
    - QML holds layout and binding only.
  - **Data:** a small **Rust daemon** (`ghostd`) reading `/proc`, `/sys` (hwmon, cpufreq, block stats), NVML, D-Bus (NetworkManager, MPRIS, logind, UPower), INMET and the forecast API, and the connection table. It pushes compact, versioned snapshots to the UI over a Unix socket (or shared memory) at fixed rates. **No shell-script polling and no process forks in steady state.**
  - **Alt+Tab:** a **KWin TabBox (window switcher) layout** written in QML. It renders inside KWin and gets live window thumbnails for free.
  - **Task list:** KWin supports `org_kde_plasma_window_management`, not `wlr-foreign-toplevel`. **Spike this first**: confirm how the shell lists and focuses windows on KWin (Quickshell support, a C++ plugin, or a KWin script bridge).
- **[call] Packaging:** a local install (script or RPM) plus systemd user units. ghost-shell starts with the Plasma session.

## 5. Visual language

**Scale:** the mockup renders at 1280×720, exactly half the native 2560×1440. Every pixel value in this document is at mockup scale. **Native = ×2 in physical pixels**, via one global scale constant. The output runs at a **1.15 display scale** and **170 Hz**, so in Qt's logical units the factor is ≈ ×1.74 (×2 / 1.15). Derive it from the output's scale at runtime; don't hard-code it. Reading the doubled numbers as logical units would overflow the screen by 15 %. Re-check spacing at native resolution: the mockups had small sizing slips the user has seen and accepted as "just a prototype".

### 5.1 Lineage (user-decided)

- **From Netrunner (Cyberpunk 2077), the base:**
  - the font (§5.3);
  - **45° chamfer cuts** on the **top-right and bottom-left** corners of every panel (12 px);
  - the solid brand block in the bar, with a slanted right edge;
  - katakana;
  - segmented meters.
  - **No glitch effects. Ever.**
- **From Section 9 (Ghost in the Shell):**
  - **the palette** (the user's favourite);
  - CRT feel, meaning faint full-screen scanlines (a 1 px dark line every 3 px at 15 % black);
  - kanji;
  - **top-to-bottom "printing" as the signature motion.**
- **From Grid (Tron: Legacy):** **corner brackets** (1.5 px, signal colour) on the two *square* corners of every panel (top-left, bottom-right). **No bloom or glow anywhere.**

### 5.2 Colour: used only to call attention

Default palette. Every colour is overridable in config (§14).

| Token | Default | Use |
|---|---|---|
| `ground` | `#0a0f11` | screen background |
| `panel` | `#0a0f11` at **55 %** fill | panel background; translucent so the dot grid shows through (fill % configurable) |
| `hair` / `hair2` | `#2a3a3d` / `#1a2527` | panel edges, separators |
| `text` / `hi` / `dim` | `#b7d1cd` / `#e4f1ef` / `#587170` | body, emphasis, labels |
| `signal` | `#a3d4cc` | the only "colour" in normal state (teal) |
| `warn` | `#e9a84c` | warnings |
| `crit` | `#ff3b45` | critical |
| `dot` | `#a3d4cc` at 16 % | 1 px dots on a 24 px grid |

Everything stays teal-grey until it needs the user. Default thresholds: loads ≥ 75 % turn **warn** and ≥ 90 % **crit**; temperatures ≥ 80 °C warn and ≥ 90 °C crit (per sensor, configurable). Dead drives and INMET *Perigo* / *Grande Perigo* are **crit**, and *Perigo Potencial* is **warn**. Device batteries ≤ 20 % warn and ≤ 10 % crit. Air quality (US AQI) > 100 warns and > 150 is crit. A microphone in use is **warn**, as a privacy cue. That severity mapping follows the user's own `storm_alert.py`.

### 5.3 Type and text

- **Font:** the user's pick is **Kode Mono** "for now", everywhere: bold for display, medium for UI. It's SIL OFL, on Google Fonts, with variable weights 400–700. It has the cut-corner shapes of the Netrunner font (Chakra Petch) plus a **slashed zero**, which the user wanted so 0 can be told from O.
  - The font family is a **config setting** (§14). Kode Mono is the default.
  - Kode Mono is monospace and wider than Chakra Petch. Check every label at native size; text truncates with "…" and never overflows.
  - **[call]** Because the user said "for now", also ship **"Chakra Petch Slashed"** as a ready alternative: Chakra Petch with a diagonal added to the zero in every weight, made during brainstorming. Chakra Petch is SIL OFL 1.1 with no reserved font name, so a modified build under a new name is allowed.
    - Generate it reproducibly: a small script in the repo (fontTools) takes the upstream Chakra Petch release and adds the slash. The slash is 0.8 of the stroke width, runs from the inner bottom-left to the inner top-right, and is sunk 0.45 of a stroke into the walls.
    - Ship it with the OFL licence, installed to the user's font directory so other apps (and projects 2 and 3) can use it.
  - Other candidates the user saw: Share Tech Mono, Martian Mono (condensed), Orbitron.
  - Japanese falls back to a CJK font (e.g. Noto Sans CJK JP). The fallback is configurable too.
- **Digits that change** (clocks, rates, percentages, countdowns) must never shift. Monospace fonts like Kode Mono handle this natively. With a proportional font set in config, each digit sits in a fixed-width `1ch` cell. **Known issue:** with Chakra Petch, a leading narrow digit (the "1" in `19:04`) got too much margin; use real tabular figures or metric-aware centring.
- **Clocks:** the big bold display font with the seconds as a teal superscript. **No extra scanline or row texture on the clock**; the user rejected both thick and thin variants. Only the global faint scanlines apply.
- **Japanese:** setting `kanji | katakana | mixed | off`, default **mixed**: kanji on panel labels (演算, 描画, 記憶, 記録, 温度, 通信, 衛星, 回路, 系譜, 時刻, 気象, 音声, 通知…) and katakana on small status chips (アラート, クリティカル, ライブ, オンライン, プレイ…).
- **Text scramble:** used **only the first time a text is written**: panel labels at startup, a newly arrived notification, a new track title. **Never** on numbers, the clock, or values that update.
- **Icons:** drawn vector icons only. **Never use Unicode glyphs as icons** (→ ▲ ⇄ × ↕). **Every arrow-type icon is a chevron**: traffic up/down, the main-view swap, scroll hints, previous/next track.
  - Chevrons are small and fairly flat: about 6.8 wide by 3 tall in a 12-unit box, drawn at 0.8 em with a 1.6-unit square-capped stroke (as in the reference mockup).
  - Play and pause stay as the standard triangle and bars.

### 5.4 Shapes, bars, motion

- **Panel header:** a 5 px signal-colour square, an UPPERCASE label (tracking .16 em), the Japanese tag, and right-aligned dim metadata.
- **Everything bar-like is segmented.**
  - Meters are rows of 4 px cells with 2 px gaps.
  - History graphs are columns of small cells, like an LED matrix.
  - The media spectrum is stacked cells.
  - Song progress is segmented.
  - Unlit cells show at ~6 % so the matrix reads even when idle.
- **Boxed alerts** (the INMET box, a dead-drive line):
  - a 3 px warn/crit left edge;
  - diagonal stripes (16 px tile) scrolling horizontally at one tile per 1.6 s, or per `appearance.alert_stripe_period_s` (user decision, 2026-10-21);
  - the text inside does a **stepped arrival flicker** (≈1.6 s, held frames) when the alert first appears, then **blinks exactly like the boot cursor**: a 1 s loop in held steps (full → half → off → half). **Never smooth fades.** After 10 s, or `appearance.alert_attention_s` (user decision, 2026-10-21), the text holds still at full colour and the stripes step once a second, or every `appearance.alert_stripe_step_s` (user decision, 2026-10-21); the same alert doesn't blink again in that session, and a new level or message is a new alert (user decision, 2026-10-21: a lasting condition such as a dead drive shouldn't cost frames forever).
  - Coloured text that isn't boxed (e.g. `WX 18° · STORM` in the bar, warn/crit rows in the notification list) does **not** animate.
- **Countdowns:** anything with a time (e.g. an INMET alert window) shows **only a live `HH:MM:SS` countdown**: no icon, no label, no clock time. It counts to the start, then to the end once active.
- **Spacing rule:** each panel has exactly one element that stretches to absorb spare height. Text never overflows; it truncates with an ellipsis.

## 6. The desktop dashboard

- It **is** the desktop: it lives on the desktop/background layer, windows draw over it, and the user sees it on an empty screen, after Show Desktop, and after the greeting.
- **Interactive parts:** notification search and scroll, media controls, view tabs, the process tree.
- **[call]** When it's fully covered by windows, it **stops rendering** and the daemon drops to what the bar needs.

### 6.1 Views

By default there are three views: **V1** (main), **V2** (main) and **MONITOR**. Views are just named layouts (§13), so users can add more.

- The **monitor shortcut** toggles MONITOR from either main view and returns to the main view you came from. It's one step from either.
- The **swap shortcut** moves to the next main view: V1 and V2 by default, or any list set in `dashboard.main_views`. This is the secondary action.
- **Shortcuts only act when the user is already on the desktop** (no focused window). They never summon the dashboard over windows.
- The bar's view chip reads **`MAIN`** or **`MONITOR`** (no V1/V2, no Japanese). Clicking it toggles MONITOR, and a chevron button next to it swaps V1 and V2.
- **[call]** Default keys: `Meta+M` (monitor) and `Meta+Shift+M` (swap), configurable.
- **Transition:** a CRT print top-to-bottom. The incoming view is revealed in 24 discrete steps with a 2 px light scanline at the edge while the outgoing one is erased by the same line. It takes 640 ms to or from MONITOR and 380 ms between V1 and V2.

### 6.2 Layouts

All values at mockup scale: 12 px margins and gutters, a 30 px bar on top.

- **V1:**
  - Left column (300): Processor 250, Graphics 204, Memory (stretches).
  - Centre:
    - Time hero, 262 high: 118 px clock, then a `DAY · DATE · UPTIME · SÃO PAULO UTC−3` line, a weather line, and the INMET alert box.
    - Below it, Media (290 wide, album art stacked) and Notifications (stretches) side by side.
  - Right column (300):
    - **Uplink scope**, 300 high: tabs `UPLINK | SILICON | TREE`, a traffic block graph, up/down/ping, and a VPN line. **It shows endpoint labels at its small size.**
    - Storage (stretches).
    - Sensors 184.
- **V2:**
  - Left column: the same as V1.
  - Centre:
    - Time band, 150 high: 74 px clock, date/uptime, weather and the alert box.
    - **Centrepiece** (stretches): tabs `UPLINK (default) | SILICON | PROCESS TREE`.
    - Media strip, 120 high.
  - Right column: Notifications 290, Network 164 (traffic + links: LAN, OpenVPN, Tailscale), Storage (stretches).
- **MONITOR:**
  - Header band, 104 high: clock, then weather (alert plus 6-hour forecast), then a compact player.
  - Six full-width **telemetry lanes**, each with a label block (name + kanji, current value, legend):
    - CPU heat grid: 16 rows, one per core (SMT pairs averaged), columns = samples;
    - GPU load;
    - memory;
    - network (download below the axis, upload above);
    - disk I/O (mirrored the same way);
    - thermals (CPU, GPU and NVMe as three stacked strips).
  - **[call]** History length: 5 min by default, configurable.
  - Right rail: Process tree (stretches) and Sensors 184.
  - **[call]** No notifications in monitor mode; it's about the machine.
- With **bar layout B** (§8), every view loses ~60 px of height to the dock and must re-flow without clipping.

### 6.3 Panel contents

| Panel | Shows |
|---|---|
| Processor | per-CCD thread block graphs (CCD0 · V-CACHE, CCD1 · FREQ, with temperatures), load history, load %, Tctl, average frequency, **CPU package power (W)**, CPU fan rpm |
| Graphics | load %, VRAM, temperature, power, fan, load history |
| Memory | used GiB, RAM and swap meters, top processes by CPU (name, CPU %, MEM %) |
| Storage | every drive: model, mounts, used %, temperature, **wear % and power-on hours**; **dead** and **unmounted** states; read/write rates |
| Sensors | every temperature and fan listed in §3, plus **device batteries** (DualSense, phone via KDE Connect) |
| Network | traffic block graph, up/down rates, ping, links (LAN + each VPN with state) |
| Uplink | dotted globe with **accurate geography**, home marker in São Paulo, **arcs to the endpoints of live connections** with moving packets and hostname labels; endpoint location from an **offline** GeoIP database **[call]** |
| Silicon | schematic of the real package: CCD0 and CCD1, each with 8 core tiles (two segmented thread bars each), the L3 strip with the V-Cache hatch on CCD0, Infinity Fabric links with packets whose speed follows load, and I/O-die blocks (DDR5 usage, PCIe ×16 to the GPU with GPU load, NVMe throughput, iGPU). Tile outlines go warn/crit with load. The header adds the CPU package power and an approximate system draw (`SYS ≈` CPU package + GPU board power). |
| Process tree | pscircle-style radial tree of the real process table (kernel threads collapsed into one node), dots sized by RSS and coloured by CPU. **Must be traversable: nodes collapse and expand.** |
| Media | MPRIS: art (the player's art URL, including `https` covers such as Spotify's, loaded once per track through Qt's cache; user decision, 2026-10-21), title, artist, segmented spectrum, segmented progress, previous / play-pause / next. **Idle states:** *paused* keeps the track and progress, shows a PAUSED chip, and the spectrum bars fall from their last values at the spectrum's own fall rate (each frame a band keeps 0.8 of its value) until all cells are unlit, then stay dark with no motion (user decision, 2026-10-21); *stopped or no player* shows the last track dimmed with `STOPPED · LAST PLAYED 12M AGO`, and play resumes it or opens the player. **Audio rows**, always visible: `OUT` (default output device, segmented volume, mute) and `IN` (default input device, live mic level, mute). While any app records, `IN` reads LIVE in warn and a boxed alert says `MIC LIVE · <APP> IS RECORDING`. |
| Notifications | searchable (live filter), scrollable history: app, message, age; warn/crit rows get a coloured left edge; new items scramble in |
| Weather | temperature, condition, high/low, humidity, wind, rain chance, 6-hour forecast, **sunrise, sunset, moon phase and illumination, air quality (US AQI)**, INMET alert box with countdown |
| Clock | `HH:MM` plus superscript seconds, `DD.MM.YY`, weekday, uptime, city / UTC offset |

**Uplink geography:** the mockup's continents come from a coarse 10° placeholder mask and look like a "bizarro Earth". The real globe must be geographically right. Sample a proper land dataset, e.g. Natural Earth land polygons (public domain), onto the dot sphere at a density that keeps coastlines recognisable. Check that the São Paulo marker sits on Brazil's south-east coast and that the major continents are unmistakable while the globe rotates.

**Rejected, do not build:**
- a circular "CORE" reactor (radial data didn't fit the style; the process tree is the one radial exception, as requested);
- ASCII earth;
- a terminal/shell panel on the desktop ("a waste of a panel");
- a virtual-desktop indicator (the user doesn't use them).
- **[call]** desktop icons: today's Folder View shows four items from `~/Desktop`. The dashboard replaces it, and `~/Desktop` stays an ordinary folder you can reach in Dolphin.

## 7. Greeting

The greeting plays on **login, unlock, and wake from sleep**. Timings are from the mockup; the sequence takes about 5.5 s.

1. **Tube power-on:** a bright 2 px line expands across the centre (120 ms), then opens vertically (300 ms) while a faint teal glow fills and fades (900 ms).
2. The **dot grid prints top to bottom** in 24 steps (~0.9 s).
3. **"Welcome back, gtkacz" types itself like a terminal:**
   - about 62 ms per character with ±40 % jitter, a little longer on spaces;
   - a block cursor in the signal colour;
   - each character **burns in** bright white and settles over 0.55 s;
   - the name is in the signal colour.
4. The subtitle types: `おかえりなさい · SESSION nnnn · N NEW SIGNALS`.
5. While it holds (~1.5 s), a faint refresh band rolls down the screen.
6. **One downward scan pass** (~0.8 s, 30 steps) erases the greeting and prints the bar and desktop behind the line.

Rules:
- The greeting uses typing, not text scramble.
- **[call]** Waking from sleep and the unlock that follows count as one event, so the greeting plays once.
- **[call]** Any key or click skips straight to the end. The key that skips is consumed and doesn't reach the app underneath.
- **Over windows:** on unlock and after a login that restores windows, the greeting plays **full-screen over the open windows**, and the final scan pass **reveals them as they were**. It doesn't clear them away. If no windows are open, the pass reveals the dashboard. It has to be on screen before the lock screen goes away, so the windows never flash in first.
- **[call]** The name is configurable and defaults to the login name.
- The cursor's blink defines the alert blink in §5.4.

## 8. Top bar and optional dock

The bar is **always on top** and reserves its height. From left to right:

- **Brand block** `GS//01 攻殻`: solid signal colour with a chamfered edge. Click opens the launcher.
  - The text (default `GS//01`) and the small subtitle (default `攻殻`) are config settings. The subtitle is hidden when Japanese is `off`.
  - The whole block can be **turned off** in config. The space goes to the next bar item, and the launcher stays on the Meta key (and the dock tile in layout B).
- **Tasks**, depending on layout (below).
- **Ticker:** a slowly scrolling status line: failed systemd units, firewalld, kernel, NVIDIA driver, IPs, VPN, critical items, pending updates. It also serves as the flexible spacer.
- **Status:** CPU %, GPU %, memory, traffic up/down (chevrons), VPN count, and the weather summary (warn colour, never animated).
- **Tray:** Bluetooth, network, volume, KDE Connect, clipboard, and a notification bell with a count (crit colour while anything critical is unread). Also any StatusNotifierItem apps.
- **View chip:** `MAIN`/`MONITOR` plus the swap chevrons.
- **Clock:** `HH:MM`, superscript seconds, and the date.

**Bar position setting:** `top` (default) or `bottom`.
- **bottom** moves the bar to the bottom edge in every layout.
- In layout **B**, the status bar goes to the bottom and the dock goes to the top.
- The dashboard re-flows into the remaining space.
- Tooltips, the launcher, the notification centre and popups open towards the middle of the screen.
- It works with every bar layout and icon style.

**Bar layout setting**, with every combination built and switchable in config:

- **A, one bar:**
  - pinned and running app icons sit in the top bar;
  - small marks under each icon count its windows;
  - the focused app gets a long bright mark;
  - pinned apps that aren't running are dimmed;
  - GPU and memory status are hidden to make room.
- **B, bar + dock:**
  - the top bar shows status only;
  - a floating chamfered **dock** at the bottom centre holds a launcher tile, the pinned and running apps, and a separator;
  - the dashboard re-flows into the remaining height.
- **C, window tabs:**
  - each open window is a chamfered tab (index, icon, app name), newest first, with `+N` overflow;
  - system status and VPN are hidden;
  - pinned apps that aren't running are reached through the launcher.

**Icon style setting:**
- `mono`: the app's real icon turned into a silhouette (alpha × luminance) tinted in the signal colour; the focused app goes `hi`.
- `colour`: the original icon.
- Badges (three-letter codes) were rejected.

**Pinned apps:** imported from today's panel, in this order: Dolphin, Steam, Tixati, Discord, Firefox, Thorium, Spotify, WhatsApp (Thorium PWA), VS Code, Slack, Konsole. They're configurable.

**[call] Task behaviour** follows Plasma's defaults so habits carry over:
- click focuses the app, or launches it if it isn't running;
- clicking the focused app minimises it;
- middle-click opens a new instance;
- hovering lists the app's window titles;
- Meta+1…0 activate the first ten pinned apps;
- unread badges and progress from apps show on their icons (a signal-colour count; progress as a segmented mark);
- right-click opens a menu with the app's jump-list actions (`.desktop` Actions), pin/unpin, and close.

The user switches apps with the taskbar slightly more than with Alt+Tab, and uses both heavily, so both must be first-class.

## 9. Launcher

- It opens with the **Meta key on its own** (KWin's modifier-only Meta shortcut, currently bound to Kickoff, must be re-pointed) or by clicking the brand block. **Meta+S stays KRunner** (the user's custom bind), as do Alt+Space, Alt+F2 and Search.
- **[call] Design, not mocked:**
  - a panel dropping from the brand block, in the panel style;
  - a search field that filters installed apps by name, generic name and keywords;
  - a results list with icon, name and category, plus a pinned/favourites section;
  - keyboard navigation; Enter launches and Esc closes;
  - **power and session actions** (lock, log out, sleep, restart, shut down) and **recent files**, so nothing from Kickoff is lost.

## 10. Alt+Tab window switcher

- The user's own switcher replaces KWin's default. It shows a grid of window cards in most-recently-used order:
  - each card has a **live window thumbnail** and a caption: index, app name, title;
  - the selected card gets a signal-colour border, Grid brackets and a bright icon;
  - the header reads `SWITCH WINDOW 窓切替` with `n / N`;
  - the backdrop dims behind it.
- Releasing Alt focuses the selection. Shift+Alt+Tab steps backwards.
- This is the KWin TabBox layout from §4.

## 11. Notifications

- ghost-shell **is** the `org.freedesktop.Notifications` server; Plasma's must not run. It supports actions, urgency, replacement, app icons and images.
- **History:** the dashboard list (§6.3) and the bell count in the bar. **[call]** History survives restarts, with a configurable limit (default 200).
- **[call] Notification centre:** the dashboard is usually covered, so clicking the bell opens a drop-down centre under the bar. It's the same searchable list in the panel style, with dismiss and clear-all, and a Do Not Disturb toggle. Clicking an entry activates its app or action.
- **[call] Read rules:** opening the centre, or the dashboard list becoming visible, marks everything shown as read. The bell shows the unread count and stays in the crit colour while any unread critical item remains. Critical items stay in history until dismissed.
- **[call]** Popups are suppressed while a fullscreen app is focused or Do Not Disturb is on; they still go to history. File-transfer and job progress (e.g. Dolphin copies) appear as a live, segmented-progress notification.
- **[call] Popups:** the dashboard is usually covered, so new notifications also appear as toasts:
  - placed top-right under the bar, in the panel style;
  - the text scrambles in;
  - normal ones dismiss after 6 s; critical ones stay until dismissed;
  - boxed critical/warning toasts use the §5.4 alert animation.
- **Built-in sources** from the daemon:
  - INMET alerts (§12), replacing `storm_alert.py` and its timer once the user agrees;
  - drive failures (e.g. the dead WD) as critical.

## 12. Data sources

| Data | Source | Rate **[call]** |
|---|---|---|
| CPU per thread, load, frequency | `/proc/stat`, `cpufreq` | 1 Hz (the bar can show 1 Hz too) |
| Temperatures, fans | hwmon (`k10temp`, `nct6799`, `nvme`, `spd5118`, `r8169`) | 1 Hz |
| GPU | NVML | 1 Hz |
| Memory, processes, process tree | `/proc` | 1 Hz values; tree rebuilt every 5–20 s |
| Disk usage, I/O, drive health | `statvfs`, `/proc/diskstats`, sysfs `nvme*/state`, SMART **[call]** | I/O 1 Hz; usage 60 s; health 10 min |
| Network rates, links, VPNs, ping | netlink / `/proc/net/dev`, NetworkManager D-Bus, Tailscale local API, ICMP to 1.1.1.1 | 1 Hz; ping 5 s |
| Live connections for the uplink | netlink sock_diag + an offline GeoIP DB **[call]** | 5 s |
| Media, spectrum | MPRIS (D-Bus); spectrum from the PipeWire monitor (cava-style FFT) **[call]** | spectrum drawn at 15 fps by default, 30 with `appearance.spectrum_fps = 30` (user decision, 2026-10-21), only while playing and visible |
| Weather alerts | INMET `https://apiprevmet3.inmet.gov.br/avisos/ativos`: `hoje` + `futuro`, filtered by geocode `3550308`; fields `descricao`, `severidade`, `data_inicio`/`hora_inicio`, `data_fim`/`hora_fim`, `riscos` (as in the user's script). The event (`descricao`) and severity (`severidade`) are translated per `[i18n]`, with INMET's own words kept beside them; `riscos` stay in Portuguese. Translations come from a built-in glossary, then from MyMemory (`https://api.mymemory.translated.net/get`, sent only the term), asked once per term and kept in `$XDG_STATE_HOME/ghost-shell/translations.json` (user decision, 2026-10-21) | 10 min; MyMemory only for a term not yet known |
| Forecast, current weather | Open-Meteo for São Paulo **[call]** | 15 min |
| Session events | logind (PrepareForSleep, session lock/unlock), ScreenSaver D-Bus | event-driven |
| Audio devices, volume, mute, mic level, recording apps | PipeWire (native API or WirePlumber) | event-driven; mic level ~15 Hz, only while visible |
| Device batteries | `power_supply` / UPower (DualSense), KDE Connect D-Bus (phone) | event-driven / 60 s |
| Drive wear %, power-on hours | UDisks2 D-Bus NVMe SMART (no root needed for the active session) | 10 min |
| CPU package power | RAPL `/sys/class/powercap/intel-rapl:0/energy_uj`. It's **root-only on this kernel**, so install a one-time permission rule (udev or tmpfiles, with sudo). Without it, the value is hidden, not shown as an error. | 1 Hz |
| Air quality | Open-Meteo air-quality API (US AQI) | 30 min |
| Sunrise, sunset, moon | calculated locally from the configured lat/lon | once per minute |

Missing or broken sources (the dead NVMe, an offline API) degrade a panel row; they never break the panel.

## 13. Extensibility: widgets and providers

The user plans to open-source ghost-shell, so **every component must be pluggable**: anyone can write a new one and put it anywhere. The user approved this approach; the details are **[call]**.

- **One contract for built-ins and community components.** Every built-in panel and bar item is itself a widget, written exactly as a third party would write one. No built-in gets private hooks.
- **Widgets** are the UI pieces.
  - A widget is a folder holding `manifest.toml` and `Widget.qml`.
  - The manifest declares:
    - `id`, `name`, `version`, and `api` (the widget API version);
    - `kinds`, meaning where it can live: `panel` (dashboard), `bar` (bar item), or both;
    - size hints in design px;
    - `requires`, the data topics it reads;
    - whether it `animates`;
    - its **settings schema**: each key's type, from `bool | int | float | string | color | enum | list`, and its default.
  - **Discovery order:** built-ins (install dir), then `~/.config/ghost-shell/widgets/`, then any extra `widgets.paths`. A user widget with a built-in's id replaces it.
  - The daemon reads the manifests and validates the config's layout against them, so a typo produces one clear error, e.g. `layout.v1[0][1]: widget "graphics" has no setting "foo"`.
- **Layout lives in the config file.**
  - **Views are named layouts** under `layout.views.<name>`. There can be any number.
    - `dashboard.main_views` (default `["v1", "v2"]`) is the order the swap shortcut and chevron cycle through.
    - `dashboard.monitor_view` (default `"monitor"`) is what the monitor shortcut opens.
    - `dashboard.default_view` is the view shown at start.
  - **A layout is a tree** of three node types:
    - **row/column splits:** children with fixed or flex sizes;
    - **slots:** one widget plus its settings;
    - **tabs:** a tabbed container of slots. Each tab shows the widget's name or a `title` override, the default tab is configurable, and only the active tab's widget is loaded.

    For example, the UPLINK / SILICON / PROCESS TREE centrepiece is a `tabs` node of three separate widgets, so any widget can be put in a tab, and those three can be placed on their own.
  - **[call]** New node types (e.g. a grid) can be added later without breaking existing configs. Third-party containers aren't part of the API yet.
  - The bar is a layout too: left, centre and right lists of bar widgets (§8).
  - The defaults reproduce the layouts in §6 and §8. The same widget can appear in several slots and several views.
  - If fixed sizes overflow the screen, they shrink proportionally with a logged warning. Panels never overlap.
- **The widget kit** is a QML module, `GhostKit`. It contains:
  - the building blocks: Panel, SegMeter, BlockGraph, AlertBox, Digits, Kv;
  - the singletons: Theme, FrameClock, Data.

  Community widgets look native and follow the §15 performance rules without extra work. Widgets may also use Quickshell's own services (MPRIS, PipeWire, system tray, UPower) directly.
- **Data providers:**
  - Built-ins live in `ghostd`.
  - **External providers** can be any program, in any language. They connect to ghostd's socket and declare the topics they publish (namespaced names like `myplugin.thing`). Then they push JSON, which ghostd relays to subscribers along with the last value.
  - Providers can be listed in config, in which case ghostd starts each one once and restarts it if it exits. Built-in topic names can't be claimed by a provider.
  - Weather alerts become a provider: INMET for Brazil, and others can add their own country.
- **Isolation:**
  - A widget that fails to load, or targets an unsupported API version, shows an **error card** in its own slot (`widget "x" targets API 2; this ghost-shell supports API 1`). The rest of the desktop is unaffected.
  - A misbehaving provider is disconnected without affecting others.
- **For authors:** an author's guide, an example widget, an example provider (Python), and JSON schemas for every built-in topic, generated from the daemon's types.
- **Generic defaults:**
  - The project defaults are machine-neutral:
    - the first screen;
    - no location, so weather widgets stay hidden until one is set;
    - pinned apps imported from the Plasma panel on first run;
    - built-in label rules for common sensor chips.
  - This machine's specifics live in the user's own config file:
    - the M27Q display;
    - São Paulo / INMET geocode `3550308`;
    - the 11 pinned apps;
    - any sensor label overrides.

## 14. Configuration

- **One config file**, e.g. `~/.config/ghost-shell/config.toml` **[call]**. There is **no settings panel**.
- **[call]** It reloads live when saved. If the file is invalid, the last good config is kept and a warning notification says what's wrong.
- **Must cover at least:**
  - bar layout `a | b | c` and bar position `top | bottom`;
  - icon style `mono | colour`;
  - Japanese `kanji | katakana | mixed | off`;
  - **font family** (default Kode Mono) and the CJK fallback;
  - **every colour token** (§5.2, plus dot grid and scanlines);
  - **panel fill %**;
  - warn/crit thresholds (including batteries and AQI);
  - pinned apps;
  - main views, monitor view and default view;
  - shortcuts;
  - the greeting name (default: the login name);
  - the brand block: on/off, text (default `GS//01`), subtitle (default `攻殻`);
  - the **layouts** for V1, V2, MONITOR and the bar (§13), extra widget paths, and external providers;
  - location (city, lat/lon, weather-alert provider and its area code, e.g. INMET `3550308`);
  - the alerts' language: `[i18n]` `from` and `to` (e.g. `pt-BR` → `en-US`; off by default);
  - history length;
  - notification limits and popup timeout.
- The user wants it to cover "other simple stuff" too. The builder should expose obvious knobs, but not invent features.

## 15. Performance requirements [call]

These are measurable targets; the user only said "highly performant".

- **Frame rate:** the default views are never fully still (the uplink globe, packets, alert stripes and blink), and the monitor runs at 170 Hz, so the dashboard must **never follow the display's refresh rate**.
  - Decorative motion (globe, packets, stripes, ticker, process-tree pulses) runs at a **capped, stepped 15 fps**. Held frames suit the CRT style.
  - The alert blink runs at its own 4 steps per second.
  - The media spectrum runs at 15 fps by default, or 30 with `appearance.spectrum_fps = 30` (user decision, 2026-10-21), only while music is playing and the spectrum is visible.
  - Data panels repaint only when their data changes.
  - The builder chooses how to get there (a frame-paced render loop, separate surfaces for the animated regions, or similar), **before** building panels, because it's expensive to change later.
- **Desktop visible, default view animating:** shell + daemon ≤ 2 % of one CPU core combined (averaged over 60 s) and ≤ 300 MB RSS combined, each resident page counted once: `Rss` in `/proc/<pid>/smaps_rollup`, since VmRSS counts NVIDIA's `/dev/nvidiactl` mappings twice (user decision, 2026-10-21).
- **Desktop visible, nothing animating** (e.g. the SILICON tab with no alerts): ≤ 1 %.
- **Covered by windows:** zero dashboard frames, ≤ 0.5 %, and the daemon collects only what the bar shows.
- **Heavy drawing** lives in native scene-graph nodes or shaders.
- **No forks, shell scripts or busy polling in steady state.**
- The first greeting frame appears within 500 ms of shell start.
- Ship a way to measure these, such as a debug overlay or a log of the shell's and daemon's own CPU and memory.

## 16. Non-goals and anti-goals

- **Non-goals (this project):**
  - lock screen (project 2);
  - terminal, file manager, GRUB, boot splash/video (project 3);
  - virtual desktops;
  - **[call]** secondary monitors beyond showing a plain dot-grid desktop.
- **Anti-goals (failure even if it works):**
  - glitch effects;
  - bloom/glow;
  - smooth fades on alert text;
  - Unicode glyphs as icons;
  - circular data layouts (other than the process tree);
  - colour used for decoration;
  - visible idle CPU/GPU cost;
  - text that overflows or clips;
  - losing any Plasma function the user relies on (§4).
  - hard-coding where a component goes, or giving built-in components hooks that third-party widgets can't use (§13).

## 17. Later work (keep in mind, not in project 1)

- **Restyle the tray popups and menus** (network, Bluetooth, audio, keyboard layout, Disks & Devices, Brightness & Night Light, camera indicator, device batteries, printers, Vaults, input method, display configuration). In project 1 they may be KDE's standard popups.
- **Projects 2 and 3** (§2). The lock screen leads straight into the greeting, so project 2 must hand over to §7 without a flash.

## 18. Left to the builder

- The exact implementation of the stack in §4 (or a better one that meets §15).
- IPC format.
- Libraries, crates and the GeoIP database.
- How Plasma's lost functions are covered.
- Sensor discovery and naming.
- How "on the desktop" is detected for the shortcuts.
- Packaging and installation.
- Migrating off gridlock, conky, `dash` and `storm_alert.py` (ask before deleting).
- Test strategy.

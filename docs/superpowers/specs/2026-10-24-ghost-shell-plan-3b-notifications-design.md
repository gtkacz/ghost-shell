# ghost-shell — Plan 3b Design: Notifications and the Bell

- **Date:** 2026-10-24
- **Status:** design approved by the user on 2026-10-24 (density A, jobs stay with Plasma, server in ghostd); implementation plan not yet written. Written while Plan 3a finishes in another session: nothing here is built or installed yet.
- **Parent design:** [`2026-10-20-ghost-shell-design.md`](2026-10-20-ghost-shell-design.md). Its notifications (§11), bar (§8), stack (§4), visual language (§5), extensibility (§13), configuration (§14) and performance rules (§15) apply unless this document says otherwise.
- **Plan 3a design:** [`2026-10-22-ghost-shell-plan-3a-bar-design.md`](2026-10-22-ghost-shell-plan-3a-bar-design.md): the bar, the tray, `BarPopup`, and the reversible retire/restore of `scripts/plasma-panel.sh`. Its rulings that matter here are repeated in §2.
- **Reference mockups:**
  - [`mockups/2026-10-24-ghost-shell-notifications.html`](mockups/2026-10-24-ghost-shell-notifications.html): the bell's four states, the toast stack (two densities, A cards and B compact rows), the centre, and the dashboard widget. **Density A is the one to build** (user's decision, 2026-10-24).
  - the dashboard widget is the Notifications panel of [`mockups/2026-10-20-ghost-shell-reference.html`](mockups/2026-10-20-ghost-shell-reference.html), unchanged.

Items marked **[call]** were decided by the designer (Claude), not the user. The user can override them. Everything else was decided by the user, in the parent design, in 3a, or in the 3a ledger.

---

## 1. Intent

Plan 3b makes ghost-shell the desktop's notification server. After it, a notification from any app lands in ghost-shell's history, shows as a ghost-shell toast under the bar, counts on a bell in the bar, and can be read back in a drop-down centre and in the dashboard's Notifications panel. Plasma's own notification popups stop appearing. The greeting gets its `N NEW SIGNALS`, and the lock screen its `SIGNALS n NEW`.

**Success looks like this:**
- Every notification the user got from Plasma yesterday arrives today in the panel style: Discord, Slack, Firefox downloads, KDE Connect (with the phone's pictures and actions), Spotify, Steam, powerdevil's battery warnings, ghostd's own config warnings.
- The bell's count is right, and it is red exactly while something critical is unread.
- The user can find a notification from this morning by typing three letters.
- Nothing is lost from Plasma: Do Not Disturb, no popups over a fullscreen game, and file-copy progress still shows (§4.4).
- It is undone by one command, like the panel.

## 2. Where this sits

3a's rulings that shape 3b (ledger `.superpowers/sdd/2026-10-22-ghost-shell-plan-3a-bar/progress.md`):
- **R21/R22:** popups from the bar are **panel-sized surfaces that close on focus loss**, not full-screen surfaces catching outside clicks: a full-screen surface cost the launcher about +35 MiB open. Clicks on the empty dashboard or on inert bar areas leave such a popup open; that is accepted.
- **R9:** KDE's own tray popups open at the top of the screen over the bar's strip; accepted for project 1 as KDE-stock.
- **R28/R29:** the tray's brightness icon has **ghost-shell's own popup** (`widgets/tray/BrightnessPopup.qml`, a `Panel` inside a `BarPopup`). "Tray popups stay KDE-stock" no longer holds for every icon; the bell follows the brightness precedent, not the KDE one.
- `BarPopup` (`shell/kit/GhostKit/BarPopup.qml`) is the transient surface beside a bar item: Overlay layer, namespace `utility`, below a top bar and above a bottom one, sized to its content after a settle delay.
- The lock screen (`2026-10-22-ghost-shell-lock-design.md` §7) wants `SIGNALS n NEW`, the number of unread notifications that arrived while locked, by way of the counts-only snapshot ghostd keeps for the gate.
- The greeting (`2026-10-22-ghost-shell-plan-2c-greeting.md`, call 4) left `· N NEW SIGNALS` out of the subtitle until the server exists.

**3c** (Alt+Tab, parked audio items) stays independent of 3b.

## 3. Goals, non-goals, anti-goals

**Goals:**
- the `org.freedesktop.Notifications` server, owned by ghostd (§4, §5);
- history that survives restarts, with read state and the unread counts (§6);
- the bell in the bar (§7), toasts (§8), the centre (§9), the dashboard widget replacing its placeholder in V1 and V2 (§10);
- the built-in sources: INMET alerts and dead drives (§11);
- Do Not Disturb, app inhibitions, and suppression while a fullscreen app is focused (§12);
- `· N NEW SIGNALS` in the greeting, and the gate's `signals` count (§6.3);
- a reversible takeover from Plasma, with the same guarantees as `plasma-panel.sh` (§4.3).

**Non-goals for 3b:**
- **inline replies** (KDE Connect's SMS reply box). The server does not advertise `inline-reply`, so apps offer their ordinary actions instead. Backlog (§17).
- **job progress** as a ghost-shell toast. KIO's file-transfer progress keeps going to Plasma's `org.kde.JobViewServer`, and Plasma keeps showing it in its own popup, moved to the other corner (§4.4). Backlog.
- per-app rules such as KDE's "show popups for this app" switches, notification sounds (apps play their own), `body-images`, `action-icons`, and the `x-kde-*` extras (URL thumbnails, origin names).
- the history on the lock screen, or any notification content there (lock design §7: counts only).
- secondary monitors: toasts and the centre appear on the bar's screen only **[call]**.

**Anti-goals** (failure even if it works): those of parent §16, plus:
- a notification that an app sent and nobody can find afterwards;
- a window of time after login in which Plasma shows popups instead of ghost-shell;
- a toast over a fullscreen game, or over the lock screen;
- a bell that costs CPU while nothing arrives;
- a critical item that goes away on its own.

## 4. The open question: take over the server, or restyle around Plasma's?

**Decision [call]: ghost-shell takes over `org.freedesktop.Notifications`.** "Restyling around Plasma's server" has no workable form, and the parent design already says ghost-shell is the server (parent §11). What follows is the evidence, read from the sources on 2026-10-24 (plasma-workspace `libnotificationmanager/server_p.cpp`, `jobsmodel_p.cpp`, `notificationsmodel.cpp`, `notifications.cpp`, `utils.cpp`; Quickshell 0.2.1 `src/services/notifications/server.cpp`).

### 4.1 Why "around Plasma's server" is dead

- Plasma's notification data lives inside `plasmashell`'s process (`libnotificationmanager`). A second process linking the same library is a "non-master": it tries to register the bus name itself, fails because plasmashell holds it, and its model stays empty. There is no D-Bus API that reads Plasma's history, unread state or dismissals.
- The only way to see what Plasma receives from outside is to eavesdrop on the bus (`BecomeMonitor`). That gives a shadow copy with no dismiss, no read state, no actions, and leaves Plasma's KDE-styled popups on screen: the opposite of the rice.

### 4.2 How the name behaves, and what it means

- **Plasma as master** registers with `ReplaceExistingService` and `DontAllowReplacement`: it takes the name from anyone who allowed replacement, and nobody can take it from a running plasmashell. The master flag is a Qt application property plasmashell sets on itself.
- **Plasma registers lazily**: `Server::init()` runs when the first `NotificationsModel` is created, which the tray's notifications applet does at plasmashell's start. **When registration fails, Plasma does not retry**: the model stays invalid for plasmashell's lifetime, and its (hidden) applet shows "Notification service not available".
- **Quickshell's built-in server** registers with no flags and never replaces an owner; it only retries when the watched name is unregistered. It also advertises no `Inhibit`, no `ActivationToken`.

So the name cannot be *taken* from a running plasmashell, but it can be *inherited*: a process waiting in the bus queue becomes the owner the instant plasmashell's connection drops, and the new plasmashell's registration then fails and stays failed.

### 4.3 The mechanism **[call]**

- **ghostd requests the name** with `ReplaceExisting` set (harmless against Plasma; takes it from a stray `dunst`), `AllowReplacement` **unset** (so plasmashell can never take it back), and queueing **on**. If plasmashell holds the name, ghostd waits in the queue.
- **The retire step** (`scripts/plasma-panel.sh`, new subcommands beside `retire`/`restore`, names left to the builder; it reuses the script's plasmashell stop/start, backups, marker and recovery code) does, once, after asking the user:
  1. writes a marker in `$XDG_STATE_HOME/ghost-shell/` that tells ghostd to request the name (ghostd re-reads it on config reload and at start, like the popup map);
  2. installs a systemd user drop-in for `plasma-plasmashell.service` with `After=ghostd.service`, and switches `ghostd.service` to `Type=notify`: ghostd sends `READY=1` only once it owns the name, so at login plasmashell starts after the name is ghost-shell's. (`nix` is already a dependency; the notify datagram needs no new crate.)
  3. backs up and sets Plasma's popup corner (§4.4);
  4. restarts plasmashell. The name passes to ghostd as the old plasmashell exits; the new one fails to register and gives up.
- **Restore** removes the marker and the drop-in, puts the popup corner back, restarts ghostd (which no longer requests the name) and then plasmashell, whose registration now succeeds. Restore works with ghostd stopped, too: with no owner, plasmashell's next start registers.
- **READY never waits for the name** **[call]**: ghostd sends `READY=1` as soon as its request has been answered, owned *or* queued, so a login can never hang behind it. The guarantee at login comes from the ordering alone: nothing holds the name before plasmashell starts, so a queued request is only ever seen when plasmashell was already up, which is the state between the marker's creation and the retire's plasmashell restart (the retire script writes the marker, restarts ghostd, then restarts plasmashell, in that order). While queued, ghostd logs a warning and the topic says `server = "plasma"`.
- **Before the retire has run** (and after a restore), ghostd never requests the name, the topic carries `server = "none"` with an empty history, the bell draws dim with no badge and the tooltip `Notifications · Plasma serving`, the centre reads `PLASMA IS SERVING NOTIFICATIONS`, the greeting leaves `N NEW SIGNALS` out, and the gate's `signals` is absent, which the lock screen already hides **[call]**. So 3b's per-task reinstalls on the live desktop can show every piece while Plasma still serves, and the headless tests assert both states.
- **Independence from the panel:** notifications retire and restore on their own, so 3b's per-task reinstalls leave Plasma serving notifications until acceptance, as 3a did with the panel. `status` reports both.
- **If ghostd dies** the name is free and Plasma does not retake it until its next start (§4.2): notifications sent in that window get a bus error, as they would with any crashed server. systemd restarts ghostd on failure and the queue rule gives it the name back at once.

### 4.4 What Plasma keeps: job progress

KIO reports file transfers to `org.kde.JobViewServer` and `org.kde.kuiserver`, registered by plasmashell with the same master flags, in the same lazy path but **independently of the notification server's validity**. Taking them over means implementing `JobViewServer`, `JobViewServerV2`, `JobViewV2` and `JobViewV3`; that is a plan of its own.

**In 3b, jobs stay with Plasma** (user's decision, 2026-10-24). Plasma's applet keeps showing copy progress in its own popup. Because that popup would otherwise sit where ghost-shell's toasts go, **the retire step moves Plasma's popup corner to the opposite one** (`plasmanotifyrc`, `[Notifications] PopupPosition`; bottom-right with a top bar, top-right with a bottom bar), backed up and restored. The spike (§15) confirms that the job popup still appears once the notification name is ghost-shell's; if it does not, that is a stop point, because KIO shows no fallback dialog while the JobViewServer name exists.

## 5. The server, in ghostd

The server lives in ghostd, not in the shell (user's decision, 2026-10-24, on these grounds):
- the gate and the greeting need the unread counts from a process that is always there, and ghostd already writes the gate's snapshot;
- history must survive the shell's restarts, which happen on every install, and ghostd already has `write_atomic` and the snapshot code;
- the built-in sources are ghostd's;
- QML holds layout and binding only (parent §4), and the shell's CPU budget is already spent (3a R25);
- ghostd's zbus connection chooses the bus flags §4.3 needs; Quickshell's server cannot.

The cost is the interface itself, which is small and well specified, and decoding `image-data`.

### 5.1 Interface

`org.freedesktop.Notifications` at `/org/freedesktop/Notifications`, spec 1.2:
- `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation` (`ghost-shell`, `ghost-shell`, the crate version, `1.2`);
- `Inhibit(desktop_entry, reason, hints) → cookie`, `UnInhibit(cookie)`, property `Inhibited`, as Plasma offers them; an inhibition dies with the caller's connection;
- signals `NotificationClosed(id, reason)` with the spec's reasons (expired 1, dismissed 2, closed by call 3), and `ActionInvoked(id, key)`.

**Capabilities:** `body`, `body-markup`, `body-hyperlinks`, `actions`, `persistence`, `icon-static`, `inhibitions`. Not `inline-reply`, `body-images`, `action-icons`, `sound`.

### 5.2 What a notification becomes

Each `Notify` produces a record:
- `id` (server-assigned, monotonic within a session), `app` (the `app_name`, or the desktop entry's name when `app_name` is empty), `desktop_entry` hint, `app_icon` (the icon name, or the desktop entry's icon when empty), `summary`, `body`, `urgency` (low 0, normal 1, critical 2), `actions` as `[{key, label}]` with the `default` key kept apart, `arrived` (wall clock, ms), `read: bool`, `resident` and `transient` hints, `image` (a file path, §5.3), and `level`: `crit` for critical urgency, `warn` for the private hint `x-ghost-shell-level = "warn"` that the built-in sources send, else `none`.
- **Body markup** is kept as the subset the spec names (`b`, `i`, `u`, `a href`); anything else is stripped. The shell renders it as styled text; a link opens with the user's default handler. Plain text is escaped by the server, so a `<` in a Discord message cannot inject markup. The search index uses the plain text.
- **`replaces_id`** updates the record in place: same id, new content, `arrived` kept, `read` reset to unread, the toast's timer restarted. A level that rises to `crit` re-arms the alert attention; nothing else re-scrambles.
- **`transient`** shows a toast but is not kept in history **[call]**.
- **Expiry:** the app's `expire_timeout` is ignored, as Plasma ignores it; `notifications.popup_timeout_s` (6) rules, and critical items never expire **[call]**.
- **Dedupe:** a `Notify` identical in app, summary, body and hints to the previous one within 1 s is refused with Plasma's `ExcessNotificationGeneration` error **[call]**, so a looping client cannot fill the history.

### 5.3 Icons and images

- `image-data` / `image_data` / `icon_data` (raw rows) are decoded and written as PNG to `$XDG_RUNTIME_DIR/ghost-shell/notify/<id>.png`; the record carries the path. `image-path` / `image_path` are kept as given (a file, or an icon name). The directory is pruned with the history. The `png` crate is the one new dependency **[call]** (pure Rust, encoding only).
- App icons are looked up in the shell, by name, through the same path the tasks use, and follow the bar's icon style: mono silhouette or colour (parent §8). `notifications.icons = false` hides them on toasts and rows (closes the `docs/IDEAS.md` line "Toggleable icons on notifications") **[call]**.

### 5.4 Activation

Clicking a toast or a row, or an action chip, calls `notifications.invoke { id, key }`. ghostd emits `ActionInvoked(id, key)` and, unless the record is `resident`, closes it (reason 2). Then, because ghostd has no Wayland connection and cannot mint an `xdg_activation` token, **it raises the sender's window through the existing KWin bridge** (`WindowCommands1`): the window whose `app_id` matches the desktop entry, else whose `pid` matches the sender's, if one exists **[call]**. A click with no `default` action and no action key does only the raising. The spike checks that Discord, Firefox and KDE Connect come to the front this way.

## 6. History, read state, counts

### 6.1 Persistence

- History is one JSON file, `$XDG_STATE_HOME/ghost-shell/notifications.json`, written atomically and debounced (at most once a second while things change), holding the records, their read flags, the DND state and the next id **[call]**.
- `notifications.history_limit` (200) caps it, oldest first. **Critical items are exempt from the cap and stay until dismissed** (parent §11).
- Loaded at ghostd's start, so the bell and the greeting have their counts before the first `Notify`.

### 6.2 Read rules (parent §11, made concrete)

- Opening the centre marks every record read when it closes **[call: on close, so the `hi` rows are still visible while it is open]**.
- The dashboard widget marks everything read once it has been **visible for 2 s** **[call]**: its view is shown, the desktop is not covered (the shell's existing covered state), no greeting is playing, and the session is not locked. The dwell keeps a flicker of the desktop from clearing the count. A notification that arrives while the widget is visible is read when its toast ends. **The greeting snapshots `unread` at the moment it starts typing**, before any of this can run, so the unlock greeting and the lock screen's count agree.
- Dismissing a record, invoking it, or `MARK ALL READ` marks it read.
- `unread` is the count of unread records, `unread_crit` the unread ones at level `crit`. The bell shows `unread` and is crit while `unread_crit > 0`.

### 6.3 Who reads the counts

- The shell, through the `notifications` topic (§6.4): the bell, the centre, the widget, the toasts, and the greeting, whose subtitle gains `· N NEW SIGNALS` when `unread > 0` at the moment it starts typing (closes 2c's call 4). The Japanese rule is unchanged; the item is left out at zero.
- The gate, through ghostd's counts-only snapshot: a `signals` field, the number of unread records that arrived after the session locked, reset when it unlocks **[call]**. No content goes there (lock design §7).

### 6.4 Topic and calls **[call]**

- Topic `notifications`: `{ server, items: [record…] newest first, unread, unread_crit, dnd, inhibited, suppressed }`. It is wanted by the bar for its whole life, so it is published on every change, whole, as every topic is. `server` is `"ghost-shell"`, `"plasma"` or `"none"` (§4.3). `suppressed` says why toasts are held now: `"dnd"`, `"inhibited"`, `"fullscreen"`, `"locked"`, or `""`.
- **Who toasts what:** ghostd stamps each record at arrival with `held: bool`, true when a suppression applied then, and the stamp never changes. The shell toasts a record when `held` is false and `arrived` is later than the shell's own start, keeping an in-memory set of what it has shown; so a shell restart replays nothing, and a suppression that has ended replays nothing either **[call]**.
- Calls: `notifications.invoke { id, key }`, `notifications.dismiss { id }`, `notifications.clear` (everything but unread criticals **[call]**), `notifications.read { ids }` or `{ all: true }`, `notifications.dnd { on }`.
- The shell's rules (what to toast, what the bell shows, the filter) live in `lib/notifications.mjs` with node tests, like `tray.mjs` and `launcher.mjs`.

## 7. The bell

- A drawn bell (the reference mockup's path), the tray's **last built-in icon, after the brightness sun and the apps' status items, before the overflow chevron** **[call]**: nearest the view chip, where its wide popup has the most room, matching the mockup's order.
- States: dim with no badge when nothing is unread; signal with a small solid badge showing `unread` (two digits at most, `99` cap **[call]**) when something is; crit icon and crit badge while `unread_crit > 0`; the slashed variant, dim, while DND is on, keeping the badge in dim.
- Left click toggles the centre; middle click toggles DND; the wheel does nothing. Tooltip `Notifications · 3 unread`.
- `Meta+N` toggles the centre from the keyboard, registered through ghostd's shortcuts like the others, configurable **[call]**. The retire step checks that the key is free, as it checks the popup keys, and names the holder if not.
- It repaints only when a count or DND changes.

## 8. Toasts

- One surface at the screen's **top-right under the bar**, 12 design px from the right edge and 8 below the bar, 360 wide, holding a column of toasts 8 apart, **newest nearest the bar**. With a bottom bar the column sits above the bar and grows upwards. The surface exists only while a toast is shown (LazyLoader), takes no keyboard focus, and is on the Overlay layer with namespace `utility` so nothing fades it.
- **At most `notifications.max_popups` (3) at a time**; further ones wait in arrival order and a last dim line reads `+N WAITING` **[call]**. A critical toast is never pushed out by a normal one.
- **Normal toast** (density A): a `Panel` with the app name as its label and the age (`NOW`, then `2M`) as its meta; a row of the app icon (20 px) or the sender's image (40 px), the summary in `hi`, the body in text clamped to two lines, and action chips; a drawn cross at the right dismisses. A segmented line along the bottom counts the 6 s down in held steps; hovering holds it. Text scrambles in once.
- **Boxed toast:** a record at level `warn` or `crit` is an `AlertBox` inside the panel: the arrival flicker, the cursor blink for its attention window, then held stripes. A countdown (INMET's) is its trailing text. Critical toasts have no life line and stay until dismissed or until the record is dismissed elsewhere.
- **Clicks:** the body invokes `default` (or just raises the app), a chip invokes its action, the cross dismisses (reason 2). Middle click on a toast dismisses it without invoking anything **[call]**.
- **Density B** (compact rows, in the mockup) is the alternative the user may choose: one line per toast, app column, summary in `hi` joined to the body, chips underneath only when there are actions.
- Suppressed toasts (§12) go straight to history, unread, and are not replayed when the suppression ends.

## 9. The centre

- A `BarPopup` from the bell, **392 × 560 design px like the launcher**, right-aligned to the bell so it never leaves the screen; `keyboardFocus: OnDemand`, closing on focus loss, on Esc, on a second click on the bell, and after it invokes something (R21/R22). It is not full-screen and catches no outside clicks.
- **Header:** `NOTIFICATIONS` with the `notif` tag (通知 / ログ), meta `18 TOTAL · 1 CRIT · 1 WARN`.
- **Search field:** the launcher's prompt chevron, the query, the block cursor, placeholder `FILTER`, an `ESC` chip. Typing goes straight to it. The filter is a case-insensitive substring over app, summary and plain body; no ranking.
- **Tool chips:** `MARK ALL READ`, `CLEAR`, and `DND` at the right, lit in warn while on.
- **The list** stretches: rows of app (76 px column, signal), `summary — body` on one line elided, age at the right; warn/crit rows get the 3 px coloured edge and coloured text, and never animate; unread rows show their summary in `hi`. Newest first, no grouping **[call]**. The selected row swaps its age for a drawn dismiss cross.
- **Keys:** Up/Down select, Enter invokes, Delete dismisses, Esc closes. The mouse does the same by click and by the cross.
- **Footer:** `16 SHOWN · 3 NEW` and `ENTER OPEN · DEL DISMISS`. Empty: `NO SIGNALS` centred and `QUIET SINCE HH:MM` **[call]**.
- Opening scrambles the header (a new showing); rows do not scramble while you type.

## 10. The dashboard widget

- `widgets/notifications` (`kinds = ["panel"]`, `requires = ["notifications"]`) replaces `placeholder("Notifications")` in the default V1 and V2 layouts; MONITOR has none (parent §6.2).
- The reference mockup's panel: header `NOTIFICATIONS 通知` with meta `3 NEW`, the search field with its `/` chip, the list, and the footer `18 TOTAL · 1 CRIT · 1 WARN` with `SCROLL` and a chevron. Rows and clicks are the centre's.
- **Search focus on the desktop layer** is unproven: `Desktop.qml` takes no keyboard. The spike (§15) tries `OnDemand` focus granted on click for the background surface. If KWin refuses, clicking the field opens the centre with its field focused, and the widget's field stays as the visible entry point **[call]**.
- New rows scramble in (parent §6.3), which costs frames only on arrival.

## 11. Built-in sources

Both go through `Notify` on the bus like any app, so they work with Plasma's server too, and carry the private `x-ghost-shell-level` hint for the warn colour:
- **INMET:** when `weather.alerts` gains an alert id, a notification from app `WEATHER`: summary `<EVENT> · <SEVERITY>` in the configured language with INMET's words kept, body the risks, urgency critical for *Perigo* and *Grande Perigo*, normal with level `warn` for *Perigo Potencial*; `replaces_id` is reused while the same alert id is updated. The dashboard's alert box keeps showing the live alert; the notification is the arrival. This replaces `~/scripts/storm_alert.py` and its timer (already inactive on this machine); **ask before deleting them** (parent §18).
- **Dead drive:** a drive whose state reads `dead` raises one critical notification per drive per boot from app `STORAGE`: `<MODEL> · CONTROLLER STATE DEAD`. It stays until dismissed (§6.1).
- ghostd's existing config warnings (`notify.rs`) are unchanged; they now land in ghost-shell's own history.

## 12. Do Not Disturb and suppression

- **DND** is a switch in the history file, toggled by the bell's middle click, the centre's chip, or `notifications.dnd`. It survives ghostd restarts and has no schedule **[call]**. While on, nothing toasts; records still arrive unread.
- **Inhibitions** from apps (`Inhibit`) suppress toasts like DND while any cookie is alive; the bell does not change for them.
- **Fullscreen:** while the active window is fullscreen (`windows` topic, already in ghostd), toasts are held. A critical notification is held too, unread and red on the bell, so a game is never covered **[call]**.
- **Locked:** while the session is locked, toasts are held and counted for the gate (§6.3).
- Held toasts are not shown later; the bell carries the count.

## 13. Configuration

```toml
[notifications]
history_limit = 200      # exists; critical items are exempt
popup_timeout_s = 6      # exists
max_popups = 3           # new [call]
icons = true             # new [call]: app icons on toasts and rows

[shortcuts]
notifications = "Meta+N" # new [call]
```

Every colour, the font and the Japanese mode already apply through `Theme`.

## 14. Performance

Parent §15 and 3a §7 rule; 3a's margin rule (+0.25 % CPU and +10 MiB Rss accepted and flagged; a budget check that goes over stops the task and the numbers go to the user) carries over.
- **Idle:** nothing. The topic changes only when a notification does; the bell repaints on count changes; the toast surface and the centre exist only while shown.
- **A toast** costs: the surface's creation, the scramble (held frames for under a second), and for boxed ones the 10 s attention window on the FrameClock, then one step a second, exactly as the dashboard's alert boxes. The first opening is expected to retain Rss, as the launcher's did (R22: +17–20 MiB one-time); measure it.
- **Covered:** toasts appear over windows by design and are the only frames the shell draws then. A normal toast must cost under 0.1 % of a core averaged over its 6 s **[call]**.
- **ghostd:** the server adds no polling; images are decoded once on arrival.
- Measurements go in README.md's Performance section as with every plan.

## 15. The spike that comes first **[call]**

In a headless KWin with a private bus and a sandbox plasmashell, as 3a's Task 2 did, with nothing on the user's display:
1. **Inheritance:** ghostd requests the name with §4.3's flags while plasmashell owns it; plasmashell is restarted; the name is ghostd's; plasmashell's applet reads invalid and does not retake it within two minutes nor when its tray popup is opened; `notify-send` lands in ghostd. Then ghostd releases and plasmashell restarts: Plasma serves again.
2. **Jobs:** with ghostd owning the name, a KIO copy still shows Plasma's job popup, and `PopupPosition` moves it to the other corner (§4.4). A "no" stops the plan.
3. **Login order:** with the drop-in and `Type=notify`, `systemctl --user show -p After plasma-plasmashell.service` lists ghostd, and `systemd-analyze --user verify` is clean. The real login is checked at acceptance.
4. **Activation:** after `invoke`, a Qt test window and a Firefox window come to the front through the bridge (§5.4).
5. **Desktop-layer focus:** can the background surface take `OnDemand` keyboard focus on click (§10)?
6. **Real payloads:** capture, with the user's yes, a read-only bus trace of a KDE Connect, a Discord, a Firefox and a Spotify notification from the live session, and render each in the sandbox: images, markup, actions, desktop entries. Snapshots go in the spike note.

Any "no" on 1 or 2 stops the plan and the findings go to the user.

## 16. Acceptance

- **Headless snapshots:** the bell's four states; the toast stack with a normal, a warn boxed, a crit boxed, an image, actions, and `+N WAITING`; the centre empty, filled, filtered, with DND on; the widget in V1 and V2; top and bottom bar. Budget runs: idle bar, one toast over a covered desktop, the centre open.
- **On the user's desktop, after asking:**
  1. run the notifications retire (§4.3);
  2. the user sends `notify-send` normal, critical, with an action and with an image; gets a phone notification through KDE Connect and a Discord message; copies a large file in Dolphin (Plasma's job popup, other corner); raises an INMET alert through the fake provider; toggles DND; plays a fullscreen video; locks with a notification pending (`SIGNALS 1 NEW`), unlocks (`1 NEW SIGNALS`); restarts ghostd (history intact);
  3. the live budgets are read;
  4. the restore is checked against copies of the configs, byte for byte, as 3a did, and the user decides whether to run it live once.
- A logout and login after acceptance confirms that ghostd owns the name from the start (§4.3).

## 17. Backlog (to add to `docs/IDEAS.md`, which another session is editing now)

- Inline replies (`inline-reply` capability, a reply field on the toast and the row; KDE Connect SMS).
- Take over `org.kde.JobViewServer` / `kuiserver` for segmented-progress toasts, and drop the popup-corner move.
- Per-app rules (mute an app, keep its toasts out) in config.
- Grouping rows by app in the centre, collapsible like Plasma's.
- A DND schedule, and DND while screen sharing (PipeWire screencast streams are already visible to ghostd).

## 18. Left to the builder

- The exact zbus layout of the interface and the record type in `ghost-proto`.
- How the retire subcommands share `plasma-panel.sh`'s code, and their names.
- The toast surface's split (one surface for the stack, or one per toast) within §14.
- Icon lookup reuse with the tasks' path, and the mono tint of app icons on toasts.
- The age buckets (`NOW`, `2M`, `1H`, `1D`) reuse Plan 2a's `ago()`.
- Test strategy: Rust tests for the server rules (replace, dedupe, cap, crit exemption, read rules, counts), node tests for `notifications.mjs`, smokes for the retire/restore against a sandbox, snapshots as above.

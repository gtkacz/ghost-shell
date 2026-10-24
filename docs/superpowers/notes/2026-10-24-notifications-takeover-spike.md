# Notifications takeover spike: inheriting the name from plasmashell, and Plasma's jobs

Date: 2026-10-24. Plan 3b Task 1, answering design §15 questions 1–3 (inheritance, jobs, login order). §15's
questions 4 (activation, already proved by 3a) and 5–6 (live payloads) are not part of it: nothing was read from the
user's session bus.

## Answers

| Question | Answer |
|---|---|
| 1. Can a process inherit `org.freedesktop.Notifications` from plasmashell? | **Yes.** Requested with `REPLACE_EXISTING` (2) only, it got `IN_QUEUE` (2) while plasmashell held the name, owned it the moment plasmashell restarted, and kept it for 123 s, through the tray popup's notification page and two further plasmashell restarts |
| 2. Do Plasma's job popups keep working? | **Yes.** With the claimant owning the notification name, a `JobViewV2` session shows Plasma's job popup, and `PopupPosition=BottomRight` plus a plasmashell restart moves it from the top-right to the bottom-right |
| 3. Is the login order expressible? | **Yes.** A drop-in with `After=ghostd.service` and `Wants=ghostd.service` on `plasma-plasmashell.service`, with `Type=notify` on `ghostd.service`, verifies clean, and a control with the opposite order is reported as an ordering cycle |

## Setup

- Fedora 44, plasmashell 6.7.5, KWin 6.7.5. `spikes/plasma-hidden/sandbox.sh` with `SANDBOX_NAME=3b-t1` (the script's
  only change: `name=${SANDBOX_NAME:-3a-t2}`), so the root is `/tmp/gs-sbx-3b-t1`. Private session and system buses
  only; the shell's own bus (`/run/user/…`) was never named in any call.
- `spikes/notifications-takeover/run.sh` runs all of it, `claim.py` is the claimant (it serves a `Notify` that logs its
  arguments, and releases the name on SIGUSR1), `job.py` opens a `JobViewV2`, `units.sh` is question 3.
  `run.sh` was run once end to end after a first manual pass; the numbers below are from that run.
- **plasmashell does register the name in the sandbox**: `org.freedesktop.Notifications`, `org.kde.JobViewServer` and
  `org.kde.kuiserver` all belonged to its pid before anything else started, so the "no" case did not arise.

## Question 1: inheritance

| Step | Evidence |
|---|---|
| Claimant requests the name while plasmashell owns it | `RequestName flags=2 -> 2 IN_QUEUE`; `GetNameOwner` `:1.13` (plasmashell); `ListQueuedOwners` `:1.13`, `:1.41` |
| plasmashell restarted | the claimant gets `NameAcquired`; `GetNameOwner` = `:1.41`, its own unique name |
| New plasmashell | its log has `org.kde.plasma.notificationmanager: Failed to register Notification service on DBus`; it holds `org.kde.JobViewServer` regardless |
| `notify-send` | arrives at the claimant: `Notify app='notify-send' summary='to claimant' … timeout=-1` |
| Tray popup, notifications page | `sandbox.sh popup tray`, then Up, Down, Enter: the page reads **"Notification service not available. Notifications are currently provided by 'ghost-shell spike claim.py'"**, where the quoted text is the claimant's `GetServerInformation` vendor and name; see `notifications-takeover-shots/01-tray-notifications-page.png` |
| Hold | 123 s after the acquisition: still `:1.41`, through the retire's plasmashell restart, the popup and a further restart |
| Release | `ReleaseName -> 1`; 30 s later no owner. plasmashell does not retake it |
| plasmashell restarted | it owns the name again (`GetConnectionUnixProcessID` = plasmashell) and `notify-send "to plasma again"` shows its toast at the top-right, 332x85 logical (`02-plasma-serves-again.png`) |

Seven plasmashell starts in the run (the first, then one after each of the claimant queuing, the retire, the hold, the release,
the second claimant and `PopupPosition`); the claimant held the name through every one that followed its request.

- **The claimant's flags:** `REPLACE_EXISTING` only. It never needed `ALLOW_REPLACEMENT` or `DO_NOT_QUEUE`.
- **Sandbox note:** the tray popup is the hidden tray's page; there is no `overflow` popup name in `popups`, and
  `tray` (the retire step's widget id 115) is what holds it. The popup window opens at the top-left of the sandbox
  output, an artefact of the headless sandbox, not of the name.

## Question 2: jobs

- **`kioclient copy` cannot show a job here**: it has no UI delegate and sends nothing to `org.kde.JobViewServer` or
  `org.kde.kuiserver` (a monitor on both saw no call; a 16 GiB copy stopped midway showed no popup). `job.py` instead
  does what KIO's job tracker does: `org.kde.JobViewServer.requestView("ghost-shell spike", "edit-copy", 0)` on
  `/JobViewServer`, then `org.kde.JobViewV2` on the returned path (`setInfoMessage`, `setDescriptionField`,
  `setTotalAmount`, `setProcessedAmount`, `setPercent`, `setSpeed`, `setDestUrl`), held open and terminated on SIGTERM.
  Whether a real KIO client does the same is a live-acceptance check.
- **Default placement:** the job popup opens as a plasmashell window at logical **1858,36, 332x143**, top-right,
  with "Copying", the destination, a 50 % bar and "5 s remaining" (`03-job-default.png`).

- **`PopupPosition`:** `kwriteconfig6 --file plasmanotifyrc --group Notifications --key PopupPosition BottomRight`
  writes `[Notifications]` / `PopupPosition=BottomRight` (a name, not a number); after a plasmashell restart, with
  the claimant still the owner, the same job's popup is at **1858,1073, 332x143**: the bottom-right
  (`04-job-bottom-right.png`). The key was absent from the copied config before.
- The two popups are Plasma's: with ghost-shell owning the name nothing else draws a job, as the design assumed.

## Question 3: login order

Run on copies of both units in a temp dir, `SYSTEMD_UNIT_PATH=<tmp>:/usr/lib/systemd/user`, with the offline
`systemd-analyze --user verify`; nothing touched the user's manager. (`systemd-analyze cat-config` has no `--user`
and `systemctl --user show -p After` needs the live manager, so neither was used.)

```ini
# plasma-plasmashell.service.d/ghost-shell.conf
[Unit]
After=ghostd.service
Wants=ghostd.service

# ghostd.service, the packaged unit with one line added to [Service]
Type=notify
```

- `systemd-analyze --user verify ghostd.service plasma-plasmashell.service`: exit 0, no output.
- Control, to show the `After=` edge is live offline: with `After=plasma-plasmashell.service` also on ghostd, verify
  exits 1: `Found ordering cycle: ghostd-cycle.service/start after plasma-plasmashell.service/start - after
  ghostd-cycle.service`.
- `Wants=` makes plasmashell pull ghostd in even if ghostd's own `WantedBy=graphical-session.target` were not enabled.

## Things worth knowing

- **Without an owner, `notify-send` waits 25 s and fails**: `/usr/share/dbus-1/services/org.kde.plasma.Notifications.service`
  activates `plasma_waitforname org.freedesktop.Notifications`, which never takes the name. A client sending in the
  window after ghostd dies and before plasmashell restarts would stall, then get an error.
- **The claimant is the server name the user sees**: the applet prints whatever `GetServerInformation` returns, so
  ghostd's name there is what "Notification service not available" names.
- **JobViewServer and kuiserver stay with plasmashell throughout**, owned by whichever plasmashell is current.
- Not covered: a real KIO client's job, `Type=notify` with a real ghostd (it sends no `READY=1` yet), the real login.

## Desktop-layer keyboard focus (Task 2)

Plan 3b Task 2, for the dashboard Notifications widget's search field. `spikes/desktop-focus/run.sh switch|hover|permanent`
runs a surface set up like `shell/host/Desktop.qml` (`PanelWindow`, `WlrLayer.Bottom`, namespace `desktop`, anchored to
all edges, `ExclusionMode.Ignore`, an input mask of only its claiming areas) next to a second `qs` config with an
ordinary `FloatingWindow` that holds the keyboard first. Both run in a headless KWin 6.7.5 of their own at
`QT_SCALE_FACTOR=1.15`, with no session bus. Input is KWin's `org_kde_kwin_fake_input`, driven by
`spikes/desktop-focus/fake_input.py` (it reuses the wire client of `scripts/fake-keys.py` and adds pointer motion and
button, which that script lacks); every key, click, activation and `keyboardFocus` change is logged from the QML.

| Question | Answer |
|---|---|
| Does `keyboardFocus` change at runtime, without a remap? | **Yes.** The new value takes effect on the live surface (None to OnDemand 2 and back) |
| Does a click on the field then give it key events? | **Yes, but not on the click that changes the value.** Set in the click handler, the surface was still not active: the next keys went to the other window, and only a second click activated it. Set while the pointer hovers the field, the first click activates it and the keys arrive |
| Does None on Esc hand focus back to the other window? | **No.** The surface stays the active window and keeps the keyboard (no deactivation event), so the next key reaches nobody. Only a click on the other window moves focus |
| Does unmapping the surface for 200 ms on Esc (`SPIKE_REMAP=1`) hand it back? | **Yes**, in both switch and permanent modes: the other window became active at once and received the next key. Not looked at for a flash of the dashboard |
| Variant, `OnDemand` permanently: does it steal focus on map? | **No**: with the other window active, mapping the surface left the keyboard with it |
| Same variant: does a click elsewhere on the dashboard steal focus? | **Yes.** A click on a plain claiming area (no field) made the surface active and the other window inactive, and the next key went to the surface. With None, the same click left the other window active |

What a click on another window does with the mode left at OnDemand: the other window becomes active, the surface's
window goes inactive and its input loses active focus, but `keyboardFocus` stays at OnDemand until the QML sets it
back, so the surface must reset it to None on `Window.active` turning false, on Esc, and on the field losing focus.

For Task 13: the field can take focus on demand when `OnDemand` is set before the click lands, so arm it on hover
(the widget's field hover handler sets OnDemand on enter, None on exit unless the field is focused) and on activation
of the field. Leaving the field with Esc needs the surface unmapped and mapped again (or another route to activate the
previous window, for instance the `ghost-shell-desktop` KWin script) to give the keyboard back; None alone does not.
Permanent `OnDemand` is the worse choice: the whole dashboard then takes the keyboard from the active window on any
click. The fallback in the plan (a click on the field opens the centre with its field focused) is still open if the
remap flash is unacceptable.

Not covered: a real KWin on the user's session, several outputs, a fullscreen window, and a `kdialog` as the other
window (a Quickshell `FloatingWindow` stood in, as the controller allowed).

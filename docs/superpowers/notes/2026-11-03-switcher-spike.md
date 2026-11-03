# Switcher spike: a KWin layout on 6.8.2

Date: 2026-11-03. Plan 3c Task 1, the proofs in spec §6. KWin 6.8.2, headless, `--virtual`, one 2560x1440 output, on a private bus. Nothing ran on the user's display. `spikes/switcher/run.sh` reproduces each row; "live effects" copies the user's KWin effects and scripts into the sandbox.

## Answers

| # | Proof | Result |
|---|---|---|
| 1 | Opens over the bar, live | **Pass.** The bar strip goes from 0,255,0 to 6,111,10, which is the 60 % backdrop. The preview's mean colour changes each second with the stand-in window, keeping its proportions |
| 2 | A flick draws nothing | **Pass.** 20 flicks, 0 openings, 20 focus changes, with Alt held for about 1 ms. A 300 ms hold opens it |
| 3 | Effects leave it alone | **Closing passes, opening does not.** See below |
| 4 | `Settings` reads `gate.ini` | **Pass.** Inside the `Instantiator`'s delegate it reads the file anew on each opening: `#112233`, then `#445566` after a rewrite, with no KWin restart |
| 5 | The module loads | **Pass.** `Panel`, `Theme`, `Scanlines` and `Duotone` render in KWin's engine, with the user's scripts loaded too. The only log line of note is headless PipeWire's |
| 6 | Windows can be looked up | **Pass.** `KWin.Workspace.stackingOrder.find(w => String(w.internalId) === String(windowId))` finds every row, with `desktopFileName` and `resourceClass` |
| 7 | Reinstalling | **Needs a new KWin.** After replacing the package's files and a `reconfigure`, the next opening still ran the old QML. A layout update takes effect at the next login |
| 8 | A broken layout is survivable | **Pass.** With an unknown type at load, and with a throw when the delegate completes, KWin stays up and Alt+Tab keeps moving focus. An unknown type shows nothing; KWin logs `Component failed to load` and tries to send a notification |
| 9 | Nothing loads before the first hold | **Pass.** The layout's root is not created at KWin's start or by 5 flicks, only by the first hold |

## Proof 3: the effects

An internal window with `Qt.ToolTip` counts as a popup (`InternalWindow::isPopupWindow`), so `fadingpopups` and the user's `kinetic_fadingpopups` animate it. Without `Qt.ToolTip` it counts as a normal window, and the user's `kinetic_scale` animates it for 334 ms instead, because KWin hard-codes internal windows as `WindowType::Normal` and never takes the `utility` route that solved this for 3a's surfaces.

**Closing is solved.** KWin's `Window.skipsCloseAnimation` is writable from QML and both fade effects honour it. The layout finds its own window by caption in `stackingOrder` on its first frame and sets it. The panel is gone on the next capture after release, 3 to 6 ms, instead of the stock effect's 600 ms fade.

**Opening is not.** Nothing a layout can reach stops the open animation: the effects run on KWin's `windowAdded`, before the layout's first frame. Measured on the panel's spot, with still stand-ins:

| Effects | Press to first change, later openings | First change to settled |
|---|---|---|
| KWin's defaults | 111, 112 ms | 135, 139, 141 ms |
| The user's | 100, 105 ms | 143, 158, 173 ms |

The user's effect fades from 20 % to 100 % over 200 ms with an `OutCubic` curve, so the window is at about 76 % at 66 ms and 93 % at the end of a 110 ms print. The print would run on top of this quick ramp, not instead of it. Layouts and stock switchers all get this ramp today.

Options. The user chose **A** on 2026-11-03, and Task 6 checks how it looks on the real desktop:

- **A, print over the ramp:** no added delay, and the panel is complete at about 200 ms. The ramp may not be visible under the print.
- **B, hold the panel back about 65 ms:** the steepest part of the ramp runs on an empty window, and the panel is complete about 65 ms later.
- **Not offered:** editing the user's effect, which an update would overwrite and which helps nobody else, or keeping a window mapped between openings, which would hold a full-screen surface over games and break "nothing exists while closed".

## Opening time against stock

Key press to first changed capture at the screen's centre, with the user's effects and still stand-ins. Headless, so the numbers compare layouts and say nothing about NVIDIA.

| Layout | First opening | Later openings |
|---|---|---|
| The spike's | 148 ms | 99, 106, 108 ms |
| `thumbnail_grid` | 210 ms | 120, 126, 127 ms |
| `org.kde.breeze.desktop`, the user's today | 207 ms | 123, 123, 124 ms |

The spike's layout is not slower than stock. It has no list yet, so Task 6 measures the real one.

## For the later tasks

- The window sets `title`, and `main.qml` sets `skipsCloseAnimation` on its first frame.
- A `Repeater`'s `itemAt()` does not notify, which left the thumbnail blank at first. The real panel uses `ListView.currentItem`.
- Install text: an update to an installed layout takes effect at the next login, while `LayoutName` and `HighlightWindows` take effect at a `reconfigure`.
- Proof 2 held Alt for about 1 ms. A person's flick is far slower, and one longer than 90 ms opens the switcher for an instant. Only the real desktop can show how often, so Task 6 checks it by hand.

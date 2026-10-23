# Plan 2c acceptance: the greeting

Installed build: HEAD `f5ad921`, installed 2026-10-23 03:13. The login greeting played once as the shell restarted (`greeting playing login` → `first frame` → `greeting revealed` → `greeting done`, no errors).

## Checks with the user (2026-10-23)

The user reported every check as passing. The journal (`journalctl --user -u ghost-shell -u ghostd`) since the install shows three `greeting playing unlock`, one `greeting skipped`, no warnings or errors, and no suspend or second login.

| Check | User | Journal |
|---|---|---|
| Meta+L, unlock with windows open: no frame of windows before the greeting; the pass reveals them as they were | pass | `armed` at lock, `playing unlock`, `revealed`, `done` |
| Unlock with no windows: the pass reveals the dashboard, whose texts scramble | pass | as above |
| Suspend, resume, unlock: one greeting, `greeting playing wake` | pass | no `playing wake` and no suspend since the install; not seen on this build |
| A key during a greeting ends it, and the key does not reach the window | pass | `greeting skipped` at 10:50:03 |
| Log out and in: the Plasma splash hides none of it | pass | no second login since the install; not seen on this build |

The subtitle's Latin text was first drawn in the CJK face; the user disliked it, and `f5ad921` draws only the kana in that face.

## Cost

See README, "Plan 2c: the greeting". While locked, the shell used 0.02 % and `ghostd` 0.03 % of one core over 60 s.

## Hand-off to project 2 (the lock screen)

What the lock screen can rely on:

- From the moment the session locks, the greeting's overlay is mapped on every screen, on the `overlay` layer with namespace `utility`, and rests on the ground colour and the full dot grid. Nothing in it animates, so it costs nothing measurable.
- The overlay takes no keyboard while the session is locked, so it never competes with the lock screen for input.
- On the unlock, KWin renders the overlay's frame first. The greeting then plays from that frame, 5–17 ms after the unlock.

What the lock screen must do:

- Its exit must end on the same ground and dots, in the same place, so that the change to the overlay's resting frame cannot be seen. The current lock screen exits without a fade, which also works.
- `ghostd`'s `session` topic reads the lock from `org.freedesktop.ScreenSaver` on the session bus (KWin's) and sleep from logind's `PrepareForSleep`. A replacement lock screen must keep `ScreenSaver`'s `ActiveChanged` true for as long as it is up and false on its exit, or the greeting neither arms nor plays.

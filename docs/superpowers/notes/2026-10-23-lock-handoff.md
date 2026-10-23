# Lock hand-off: from the gate's last frame to the greeting overlay

Date: 2026-10-23. Lock plan A Task 5, Step 5 (lock design spike 3, §6.5), measured by the method of
`2026-10-22-greeting-handoff-spike.md`.

## Answer

**Pass.** In four runs (two with the stock effects, two with the user's live effect set) the gate's last frame and
the overlay's resting frame are identical: 0 of 3 686 400 pixels differ, scanlines included, because the overlay draws
the same scanlines over the same ground and dots. Around the unlock, no capture showed the red stand-in window, a
blend with it, or black.

## Setup

- Fedora 44, KWin 6.7.5, kscreenlocker 6.7.5, Qt 6.11.2. Headless KWin, `--virtual`, one 2560x1440 output, scale 1,
  the screen locker on. Nothing ran on the user's display or reached the user's buses.
- `spikes/lock-handoff/run.sh`, on `spikes/greeting-handoff/`'s `fake_logind.py`, `grab.py`, `fake_key.py` and
  `window.qml`:
  - The package is installed by `scripts/install.sh --lock` into the sandbox, and KWin gets
    `PLASMA_DEFAULT_SHELL=org.ghostshell.lock` in its own environment, the route the drop-in gives the live KWin.
    So `ScreenSaver.Lock` starts the real greeter on our package.
  - **One change to the installed copy:** `LockAdapter` is swapped for `FakeAdapter` and `done` only logs, since the
    real success path needs the user's password. The fake logind's `Unlock` then ends the greeter (SIGTERM, exit 1),
    and ksld unlocks through the same `doUnlock` a password's `Qt.quit()` (exit 0) reaches. No password reached PAM.
  - ghost-shell's real shell runs on `ghostd run --fake`. The session values ghostd would publish for the lock and the
    unlock go in through the shell's debug IPC (as `scripts/smoke-greeting.sh` does), right after KWin's
    `ActiveChanged`. The snapshot is absent, so both sides use their defaults; the Task 3 tests tie those together.
- Sequence: arm the overlay while unlocked and capture its resting frame (`1-overlay-resting.png`), disarm it; lock;
  arm the overlay under the lock; type `ghost` and Enter into the gate; after its pass, capture the gate's last frame
  (`4-gate-last.png`); watch the centre 64x64 as fast as KWin answers while the fake logind unlocks; send the unlock
  greeting.
- `spikes/lock-handoff/compare.py` compares the two full frames. A pixel may differ only as the gate's scanline
  darkening the overlay's pixel, on the scanline rows (2 of every 6 at this width).

## Results

| Run | Effects | `Unlock` → `ActiveChanged(false)` | Last capture before / first after | Frames differing | Red over the ground in any capture after | Darkest capture (r+g+b) |
|---|---|---|---|---|---|---|
| 1 | stock | 48 ms | −1 / +1 ms | 0 px | none (r − max(g, b) ≤ −1) | 40.5, the ground's own |
| 2 | stock | 80 ms | −1 / +1 ms | 0 px | none (≤ −0.6) | 40.5 |
| L1 | live | 86 ms | −5 / +0 ms | 0 px | none (≤ −1) | 40.5 |
| L2 | live | 52 ms | −1 / +2 ms | 0 px | none (≤ −2) | 40.5 |

- **The scene holds still across the unlock.** Every capture from the gate's last frame through the unlock reads
  `9.8, 14.5, 16.2`, the ground with its dots and scanlines, until the greeting starts moving 141–182 ms after
  `ActiveChanged(false)`. That delay is the IPC stand-in for ghostd (a `qs ipc` process per call); it bounds when the
  greeting moves, not what shows.
- **What follows is the greeting itself:** a bright neutral flash (`203, 204, 204` at its peak) from its tube line,
  `burn` white, fading back to the ground. Red never exceeds green or blue in any capture, so no frame shows the
  window under the overlay.
- 4 400–5 500 captures per 4 s watch, at most 24 ms apart, and 0–5 ms apart around the unlock.
- The gate loaded through `PLASMA_DEFAULT_SHELL` (`gate: prompt lock` in KWin's log), took the typed keys, played
  its pass (`gate: succeeded`, then `gate: done`) and rested on ground and dots
  (`3-locked-prompt.png`, `4-gate-last.png`).

## Limits

- The exit is the logind route, not a real password's `Qt.quit()`. Both end the greeter process before `doUnlock`;
  the real one is left to Task 7 with the user.
- One output, scale 1, no GPU timing, stock or copied effects rather than the live compositor; the snapshot was the
  defaults, not the user's file.
- The session values were sent over IPC rather than read by ghostd from KWin's `ScreenSaver`.

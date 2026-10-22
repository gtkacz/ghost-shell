# Plan 2a acceptance (2026-10-21)

Plan 2a is installed as the user's `ghostd` and `ghost-shell` services, HEAD `0b2d333`, reinstalled at 21:43. The measurements below ran against the installed binaries. The README's "Performance" section has every figure.

## Install

- The install followed ruling R17's order: build while the services run, stop the shell, `scripts/install.sh`, restart the daemon, start the shell. Both units were already enabled.
- Already done earlier in the day, with the user's consent:
  - the KWin script (`ghost-shell-desktop`) is enabled;
  - Meta+M and Meta+Shift+M are registered with KGlobalAccel (checked: `monitor` 268435533, `swap` 301989965);
  - the machine settings are in place.
- The user's config was regenerated from `ghostd default-config` and the user's settings re-applied. The old file is `config.toml.pre-2a-accept`. The settings carried over:
  - `alert_chips = false`, `spectrum_fps = 30`;
  - `alert_stripe_step_s = 0.01`, `alert_attention_s = 600`;
  - `display.model = "M27Q"`;
  - São Paulo (−23.55, −46.63) with INMET area 3550308;
  - `[i18n]` pt-BR → en-US.
  
  The user's V1 equalled the new default V1, so the config carries no view override. V2 and MONITOR are now active.

## Budget states

Each state ran in a headless KWin at the user's geometry (2560×1440, scale 1.15) against a daemon of its own, with the real INMET provider and Open-Meteo. That way the user's desktop was never interrupted (ruling R46). The live covered state was read from the user's services.

| State | Shell + daemon | Budget | Result |
|---|---|---|---|
| Animating: V1, the user's settings | 1.59 %, 1.51 % | ≤ 2 % | pass |
| Playing: V1, Spotify, `spectrum_fps = 15` | 1.83 %, 1.83 %, 1.90 % | ≤ 2 % | pass |
| Playing at the user's `spectrum_fps = 30` | 2.36 %, 2.43 % | — | recorded |
| Still: MONITOR, default settings, nothing playing | 0.76 %, 0.76 % | ≤ 1 % | pass |
| Covered: `measure-covered.sh 60` | 0.05 %, 0 frames | ≤ 0.5 %, 0 frames | pass |
| Covered: the live services | 0.07 % | ≤ 0.5 % | pass |
| RSS, live, each page counted once | 250 540 + 37 888 = 288 428 KiB (94 %) | ≤ 300 MB | pass |

## Behaviour checks with the user

- Meta+Shift+M swaps V1 and V2, and Meta+M toggles MONITOR, with the print (`view v1 -> v2 (380 ms)`, `view v1 -> monitor (640 ms)`). With a window focused they do nothing.
- Covering and uncovering logs `dashboard covered` and `dashboard visible` as the user works and uses Meta+D.
- The spectrum falls when playback pauses (R45). Quitting the player gives `STOPPED · LAST PLAYED`, and Play reopens it.
- A `pw-record` recording turns IN to `LIVE` with a moving level and shows `MIC LIVE · PW-RECORD IS RECORDING`. Both go when it stops. No capture stream was left behind, and the journal shows no PipeWire error.
- The OUT and IN rows toggle their device's mute anywhere on the row (`7b1d303`). The user confirmed it on the live desktop by clicking the label, the device name and the meter. Before that fix only the value was clickable. The user finds click-to-mute on the volume meter counterintuitive and wants the meters to become volume controls in a later polish pass (`docs/IDEAS.md`).

## Deviations, each a ruling in the Plan 2a ledger

- **R46:** measurements ran headless rather than on the live desktop, because agents' windows on the user's display had broken the user's show-desktop.
- **R50, the user's decision:** the memory budget counts each page once (`smaps_rollup` Rss), because VmRSS counts NVIDIA's `/dev/nvidiactl` mappings twice. Read by VmRSS, the live shell was over budget.
- **The texture atlas is 512×512** in the shell's unit. That saves 64 MiB of VRAM and 4–10 MiB of RSS. It isn't a config key, because Qt reads it before the config arrives.
- **The user's own settings animate more than the defaults.** With 600 s attention, scrolling held stripes and 30 fps spectrum, an alert on screen keeps the shell at about 1–1.35 %. The playing state at 30 fps is 2.4 %. The pass marks use the defaults.
- **The Playing margin is thin:** 1.83–1.90 % against 2 %. The spectrum adds about 0.8 % to the shell.
- **Remaining memory to explain:** about 8.5 MiB of Qt's image-format plugins load with the first album art, and nothing in the shell can avoid it. A 40 MiB NVIDIA allocation seen once on the live display didn't recur after the atlas change.

## Open for later plans

The ledger's carry-forwards C1–C24 are left to the final review, which rules on each one that is still open.

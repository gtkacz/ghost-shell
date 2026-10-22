# Plan 2b acceptance (2026-10-22)

Plan 2b is installed as the user's `ghostd` and `ghost-shell` services, HEAD `5d8312f`, reinstalled at 16:57 (reinstall #18, the last of eighteen since Plan 2a closed). Main holds 80 commits since 2a's close at `4e3e099`. The measurements below ran against the installed binaries, headless. The README's "Performance" section has every figure.

Gates at the end of the fix wave (HEAD `598ae7c` before it was cherry-picked onto main): `cargo test --workspace` 556 passed, `node --test` 479 passed, `cargo fmt` and `clippy -D warnings` clean, and `make smoke`, `make smoke-layout` and `make smoke-holds` ok, the last with the shell-log error scan.

## What shipped

| Task | What |
|---|---|
| 1 | Shader `MirrorGraph` and `HeatGraph`. The heat history is a ring carried by 4-row `HeatBand` effects (P22a, P22b), because re-sending the whole history every tick cost three times the budget. |
| 2 | `procs` and `procs.tree` topics, split out of `mem`. The fakes were split into `sampled` and `feeds` modules first. |
| 3 | History for cores, memory, disk I/O and temperatures, with the history recording moved out of the sampler. A subscribe whose outbox is within its cap now gets a full History line (H1, H1a). |
| 4 | Per-split `gutter`. |
| 5 | `net` sampled topic (rates) and its history. |
| 6 | `net.links` feed: NetworkManager, Tailscale and ping. The ping socket is opened without `unsafe`. |
| 7 | `telemetry-lane` widget, six lanes on MONITOR. |
| 8 | `network` widget, as the V2 panel and the V1 scope. |
| 8b | Hold and want harness: `make smoke-holds` fails when a frame-clock hold or a topic want does not return to baseline. |
| 8c | Empty follow-up frames removed. The cause was Qt re-laying out `Row`s, not a split publish, so `InlineRow` replaced them. |
| 9 | `process-tree` widget and its fake tree. It is no longer in the default views (see below). |
| 10 | `uplink` feed: live sockets, offline GeoIP, labels, tailnet peers. The databases are unmapped while the globe is hidden. |
| 11 | Land mask script, the dot land bits and the one-shader globe (G1). |
| 12 | `uplink` widget. It dims and stops animating on stale data, and keeps its labels inside the panel. |
| 13 | Tab strip in the panel header row. |
| 13b | Text scrambles again when its widget is shown again (tab, view or settings change). |
| 13c | Lane titles key on the name, not on the scrambling text (274–399 restarts per visit before). Every text's first write in a showing scrambles, values included. |
| 14 | CPU package power and iGPU load, and the processor's PKG cell. A counter reset reads as no value rather than a burst. |
| 15 | `silicon` widget. |
| 16 | `disks.health` feed (UDisks2): wear and power-on hours in the storage rows. |
| 17 | `batteries` feed (UPower and KDE Connect) and the sensors rows. |
| 17b | Spectrum capture retries after a PipeWire restart: 1 s, doubling, capped at 30 s, only while wanted. |
| 18 | Default views without machine placeholders. |
| 18b | Default MONITOR rail: storage replaces the process tree. |
| 19 | This install, the measurements and the checks. |

Fix wave after the final review (cherry-picked as `d2ede4a` to `0e268d5`, 18 commits), plus a follow-up (`276c2d0`, `cbe02d6`, `e84043e`, `5d8312f`):

- silicon stops and dims its packets on stale data (the review's I1);
- README batch 3 (I2);
- `smoke-holds` fails on any shell-log error, through a scan shared with `qml-smoke.sh`, and every smoke, `smoke-holds` and `measure-covered.sh` isolates PipeWire;
- UDisks2 reads bounded to 2 s, keeping the last drive health;
- Tailscale replies capped at 4 MiB (reaching the cap reads as no status);
- NetworkManager's two methods called with `NO_AUTO_START`;
- the lane text shrinks no further than 0.80, then drops whole parts;
- a link reporting no speed shows none, and no peers show while Tailscale is down;
- a text keeps its decided scramble key when shown again;
- the uplink reassigns its shown labels only on a change;
- batteries share the feeds' `drain`, and the Tailscale cadence is one constant;
- the task and ruling names are out of the code, and stale comments are rewritten;
- the process tree stays in the smoke layout, so a load failure would still show;
- `install.sh` prints the advice to warm the shell up after a shader change;
- the design spec records the 2026-10-22 decisions;
- follow-up: `rateShort` shows whole megabytes from 10 MB/s so both net rates fit, the shell-log scan catches shader and error-level lines, the spec attributes the plain globe labels correctly, and four comments are rewrapped.

## Budget states

Each state ran in a headless KWin of its own (2560×1440, `QT_SCALE_FACTOR=1.15`) against the installed build and its own daemon, on a private session bus. The system bus was real, so NetworkManager, UDisks2 and UPower were live. PipeWire was isolated (`PIPEWIRE_REMOTE=/nonexistent`), so no run read the user's audio graph. The daemon's config used the real São Paulo location, no alert provider, and the offline GeoIP files. CPU is the shell and the daemon summed over 60 s after a 26 s settle. Rss is `smaps_rollup` (R50). One warm-up run per state was discarded (C28), then three runs were kept, interleaved.

| State | Shell + daemon | Frames/s | Budget | Result |
|---|---|---|---|---|
| Animating: V1, UPLINK tab (globe, packets, the dead-drive alert) | 1.60 % (1.56–1.68) | 14.92 | ≤ 2 % | pass |
| Animating: V2, SILICON tab | 1.87 % (1.67–1.88) | 14.93 | ≤ 2 % | pass, thin margin |
| Still: MONITOR after 18b and 13c | 0.80 % (0.80–0.81) | 2.00 | ≤ 1 % | pass |
| Still: MONITOR before them, with the process tree | 1.05 % (1.02–1.24) | 2.2 | ≤ 1 % | over, inside the user's accepted 1.25 % |
| Covered: installed build | 0.04 % (0.04–0.05) | 0 | ≤ 0.5 %, 0 frames | pass |
| Covered: `measure-covered.sh 60` | 0.04 % (0.04–0.05) | 0 | ≤ 0.5 %, 0 frames | pass |

| State | Rss (shell + daemon) | Budget |
|---|---|---|
| V1 UPLINK | 176 884 + 49 412 = 226 296 KiB (highest run 227 280) | ≤ 307 200 KiB, pass |
| V2 SILICON | 180 872 + 42 204 = 223 076 KiB | ≤ 307 200 KiB, pass |
| MONITOR after 18b and 13c | 209 152 + 42 356 = 251 508 KiB | ≤ 307 200 KiB, pass |
| Covered | 171 388–176 828 + 41 984–42 208 KiB | none set |

Conditions and caveats:

- The 1.24 % MONITOR run had the machine's load rising from 2.05 to 3.98 from other work. The other two read 1.02 and 1.05 at load below 0.8. After 18b and 13c every load average stayed below 0.3.
- V2 SILICON read 1.87 %, so Task 15's 2.02 % mean was not reproduced.
- Covered, the GeoIP mappings dropped from 2 to 0, and the daemon's read syscalls fell from about 490 to exactly 4 a second, with no irregular traffic. That is indirect evidence that `uplink`, `net.links`, `disks.health` and `batteries` make no calls. `strace` and `bpftrace` are not installed, and a non-root process cannot watch the system bus, so it is not proof.
- **The user's own appearance was not measured.** Every figure is at the default appearance (spectrum 15 fps, default stripe step and attention). The user's settings (spectrum 30 fps, stripe step 0.01 s, attention 600 s) animate more, and 2a recorded the playing state at 30 fps as 2.4 %.
- Headless Rss is lower than the live service's, which holds extra NVIDIA `nvidiactl` chunks (2a). The live figure was last read at 289 880 KiB right after a MONITOR visit at reinstall #13, before several tasks landed, and was not re-read here.
- The dead-drive alert in V1's storage panel and the absence of an alert on MONITOR were not confirmed in a snapshot. The frame rates are consistent with both.
- The first start after any shader change runs about 61 MiB higher until the shell is restarted once the NVIDIA cache is warm (CW1). Reinstalls #12 to #18 each did that restart by hand. `install.sh` now only prints the advice.

## Behaviour checks with the user (2026-10-22)

- **Checked, "mostly looks good":** the globe, the network panel, power, batteries and drives.
- **Process tree:** scrapped from the views at the user's request. The widget, its topic and the fake stay, and the smoke layout still loads it.
- **Scramble:** two issues found.
  - MONITOR's lane titles looped through a different animation for seconds. Task 13c keyed them on the name, not on the scrambling text.
  - On a view switch every text's first write should scramble, values included, with later changes plain. Task 13c did that.

  The user kept 13c's scramble as it is, with its +12–17 MiB of Rss and the full katakana alphabet.
- **Not checkable:** a direct Tailscale peer as `TAILNET:<HOST>`. The only peer is offline and relayed.
- **RAPL:** the user installed the udev rule at 12:24, and `energy_uj` read live at 38.9–41.3 W. The check after the next reboot (that it is still `0444`) is still owed.
- **VPN disconnect turning its row `DOWN`:** the ledger does not record the user doing it. It is not yet confirmed.

## Deviations

- **Call 4, silicon animates.** Spec §15's "nothing animating" example named the SILICON tab, but the mockup moves packets along the fabric links and the mockup wins (spec §1). The still budget is measured on MONITOR and SILICON against the animating 2 %. The spec now says so.
- **Call 5, the silicon schematic draws only what the daemon knows.** The mockup's `AM5`, `DDR5`, `PCIE 5 X16` and `6 NM` are facts about this machine that no topic carries, and they are omitted. The title is `<MODEL> · <C>C / <T>T`, the I/O-die blocks read `MEM`, `GPU`, `NVME` and `IGPU`, and the iGPU reads `OFF` when no AMD DRM device reports a load.
- **Call 7, the V1 scope is two panels.** The plan had tabs `UPLINK | SILICON | TREE` above a headerless network scope. The user scrapped the tree, so the tabs are `UPLINK | SILICON`.
- **R49 and G1, shaders instead of a native module.** The user dropped `GhostKit.Native`. `MirrorGraph`, `HeatGraph` and the globe are packed-uniform shaders. G1 made the globe one fragment shader with no land texture and no vertex shader. The land is 3200 dot bits packed into uniforms by a generated module, which cost +3 MiB of Rss against +21 to +33 MiB for a texture. The 1024×512 "lattice map" the user confirmed no longer exists.
- **V1 layout B.** The right column is 332 px. The tabs take 211, the network scope 109 and the gaps 12. Storage takes 152 and sensors 158. The user first chose 346, then chose B after seeing the numbers: no layout fits both five drives and the batteries at 346.
- **MONITOR's rail is storage plus sensors.** Storage is fixed at 188, which fits five drives and the R/W row, and sensors fill the rest.
- **`rateShort` shows whole megabytes from 10 MB/s**, so both net rates fit the lane.
- **The Plasma panel overlap is left as is.** The user's floating panel covers the bottom of MONITOR's last row.
- Consequences worth knowing:
  - at the 0.80 floor, MONITOR's net lane drops its up rate under heavy traffic;
  - the design spec's process-tree lines now read "available, not in the default views".

## The user's decisions

2026-10-21, with the plan:

- Endpoint labels default to the ASN organisation. `[uplink.labels]` maps an organisation or `AS<number>` to fixed text or to `"process"`, and the `/proc/*/fd` walk runs only for `"process"` labels.
- The land mask is a committed script and a committed PNG.
- RAPL `energy_uj` is world-readable (0444) through a udev rule the user installs with sudo.
- A direct Tailscale peer is located by GeoIP of its current endpoint and labelled `TAILNET:<HOST>`. A relayed peer stays off the globe and counts in the VPN line.
- R49: shaders with packed uniforms, no native module (reaffirmed 2026-10-22 00:03).
- Work waits until 2a is closed, rather than branching.
- Pre-approved: the Natural Earth download, `scripts/fetch-geoip.sh`, and read-only `busctl --user introspect` on KDE Connect.
- "Do it all in main" (22:41).
- Q1: a MONITOR reading over 1.00 % in Task 7 or 9 stops the work for the user's call.
- Q2: confirmed Tailscale status every 10 s, 2 s Tailscale and ping timeouts, the tree every 10 s, 8 globe endpoints, Task 1's targets (≤ 100 ms, ≤ 30 MiB, ≤ 0.30 %), 1000 processes laid out in under 20 ms, and the 1024×512 map.
- Q3: the spectrum retries after a PipeWire restart at 1 s, doubling, capped at 30 s, only while wanted.
- Q4: `[batteries] labels` maps the DualSense's id to `DUALSENSE EDGE`, with no built-in shortening.

2026-10-22, overnight:

- O1: overruns up to +0.25 % CPU and +10 MiB Rss over a budget check are accepted and flagged. Past that margin the item is parked.
- O2: at each reinstall, widgets that passed review and budget join the user's own views.
- O3: Task 17b's live check may restart the user's PipeWire services once.
- O4: no `[uplink.labels]` table.
- O5 and O6: agents may use the live display, keys, clicks and the media player until the user's next message. Smokes and measurements stay headless.
- The user installed `qt6-qtbase-devel` and `qt6-qtdeclarative-devel` "just in case". R49 stands.

2026-10-22, after waking:

- About 12:25:
  - MONITOR's budget is accepted at about 1.1 % (up to 1.25 %), and memory is measured per view in Task 19;
  - the tree's edge smoothing is kept;
  - lane text shrinks to fit (T7a), with a minimum scale;
  - confirmed values: NetworkManager's first read 2 s, UDisks2 reads 2 s, Tailscale replies capped at 4 MiB, the tree's click slop 6 px, the land-mask download timeout 60 s;
  - the Plasma panel overlap stays.
- Text scrambles again when shown again on a tab, view or style change, and not on an uncover. Numbers and clocks were to stay plain, which the Step 3 decision below then amended.
- V1's uplink column grows to 346 px, then layout B (332, storage 152, sensors 158).
- About 14:50, after the Step 3 checks:
  - the process tree leaves the views only (the widget, topic and fake stay);
  - MONITOR's right rail gets storage above sensors;
  - on a view switch every text's first write scrambles, values included, and later changes are plain.
- Keep 13c's scramble as is, at +12–17 MiB of Rss.
- The lane-text minimum scale is 0.80, below which whole parts drop. The GeoIP credit stays as it is.
- The user was given the RAPL install command and installed the rule at 12:24.

## Open items for later plans

- **The RAPL check after the next reboot:** `energy_uj` still `0444`, and `ghostd probe cpu` still reading a plausible `package_w`.
- **The VPN-disconnect check** (`DOWN` on the VPN row) and a **direct Tailscale peer** on the globe, once the peer is online and direct.
- **The 60-minute history cost (B1).** The shell rebuilds all 26 series every tick. That costs about 1.7 ms a tick at the default 5 minutes, but 15–51 ms (1.5–5 % of a core) at the 60 minutes the config allows. A ring-backed history is the fix. The user should know before choosing a long history.
- **The spectrum's 1 Hz fast-fail retry.** If PipeWire keeps accepting the stream and then failing it, the retry runs every second while the spectrum is wanted (the backoff resets on success). Lit frames resuming after a restart were never seen live, since that needs a second PipeWire restart and a source that survives it. Spotify lost its PipeWire client at the overnight restart and needs restarting by the user.
- **Globe labels and media texts do not scramble.** The uplink's endpoint labels and the media title stay plain; the spec says so.
- **Parked minors the final review left parked,** with its reasons in `final-review.md`:
  - Task 1: ring pixel evidence, the `rows` contract line, duplicated `sampleAt`/`skippedColumns`, a few what-comments;
  - Task 2: the tree built when only `procs` is wanted, the pid-only kernel-thread key, `topic-tap.py` tracebacks;
  - Task 3: an unreadable sibling list failing `CpuCollector::new`, test-shape issues around the outbox cap;
  - Task 6: a failed `ListConnections` dropping the LAN links, untested device states;
  - Tasks 5, 8, 14, 16, 17: net, route and RAPL edge cases, tablet and keyboard device types, a timed-out UPower read republishing the previous period;
  - Task 7: `spanS` cap, hidden `Glyph` shapes, the net value's `--` staying bright;
  - Task 9: the `?` root's collapse, the 20 ms layout test under load;
  - Task 10: the walk cost for a churny organisation, malformed-row tests, case-variant label keys;
  - Task 11: the gallery env overlap, land-mask output stable to the pixel but not the byte, the `HIGHEST_BIT` literal;
  - Task 12: `labelX` with no left clamp for a degenerate hostname;
  - Task 13c: placeholders inside longer strings count as written, the public writable `FirstWrite.rebuilding`;
  - harness leftovers from Task 8b.
- **Smaller watch items.**
  - `config_load.rs` is at 798 of 800 lines and `fake/sampled.rs` at 739, so new tests go elsewhere.
  - `fetch-geoip.sh` has no `curl` timeout.
  - The system-bus connections stay open after the first want (+2 MiB once).
  - Silicon, the network widget and the telemetry lanes restate a few helpers because the widget-helper contract forces it (`MISSING`, `NET_FLOOR_BPS`, `loadLevel`).
  - The `ScrambleText` re-decide guard has a node test but no harness case.
  - The GeoIP attribution names the city database only (the user's call: unchanged).
- **Branches left for the user to delete,** since `-d` refuses cherry-picked branches: `t8`, `t8c`, `t9`, `t11`, `t12`, `t13b`, `t15`, `t16`, `t17`, `t17b`, `t18`, `t18b`, `t13c`, `t14-port`, the seven `worktree-agent-*` ones, and `spike/globe-shader` (its notes are on main as `7cf79c3`).

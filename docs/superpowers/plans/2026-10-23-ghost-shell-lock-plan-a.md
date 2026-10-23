# ghost-shell Project 2 — Lock and Login, Style A (Prompt) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** KDE's lock screen and the SDDM login screen draw ghost-shell's style A (Prompt) from one shared QML module. Each is an opt-in install part, and both hand over to the Plan 2c greeting without a flash.

**Architecture:**
- **`ghostd`** gains `[lock] style` and writes a small INI **gate snapshot** (style, appearance, display model, host) on every good config load:
  - to `$XDG_STATE_HOME/ghost-shell/gate.ini` for the lock screen;
  - to `/var/lib/ghost-shell/gate.ini` for SDDM, but only when the login part created that directory.
- **`shell/gate/GhostGate`** is a Quickshell-free QML module:
  - a gate `Theme` singleton exposing the same members GhostKit's `Theme` does, for the files it reuses;
  - thin wrappers over GhostKit's shaders;
  - the gate parts;
  - `lib/gate.mjs` for every rule.
- **Sharing with GhostKit:** the reusable GhostKit files (`Panel`, `Chip`, `Glyph`, `lib/theme.mjs`, `lib/scale.mjs`, `lib/views.mjs`, `lib/attention.mjs`, the shaders) are **copied at assembly time**, never forked in the repo. A test fails if they reference a `Theme` member the gate's `Theme` lacks, or if anything a greeter loads imports Quickshell.
- **Styles and adapters:** `Prompt.qml` arranges the parts against an adapter contract. There's one adapter per backend:
  - `LockAdapter.qml` for kscreenlocker;
  - `LoginAdapter.qml` for SDDM;
  - `FakeAdapter.qml` for tests.
- **Install:** `scripts/install.sh --lock` and `--login` install the parts; `--remove-lock` and `--remove-login` restore what was there. A plain install touches neither.

**Tech Stack:**
- Rust 1.98, no new crates.
- Qt 6.11 Qt Quick + QtCore (`Settings { location }` reads the INI). No Quickshell in anything a greeter loads.
- `qmltestrunner-qt6` with `QT_QPA_PLATFORM=offscreen` for QML tests (never on the user's display).
- Node 24 for `.mjs` tests.
- kscreenlocker 6.7.5, SDDM 0.21.0 (`sddm-greeter-qt6`), KWin 6.7.5 headless for greeter runs.

**Spec:** `docs/superpowers/specs/2026-10-22-ghost-shell-lock-design.md` (all of it; style A is §7.1). The parent is `docs/superpowers/specs/2026-10-20-ghost-shell-design.md` §5 and §15. The mockup `docs/superpowers/specs/mockups/2026-10-22-ghost-shell-lock-directions.html` is the visual and timing truth (CSS `.A`, `.pw`, `.alert`, `.acts`; script `renderPw`, `submit`, `unlock`). Also read:
- the greeting's hand-off contract: `docs/superpowers/notes/2026-10-22-plan2c-acceptance.md` § "Hand-off to project 2";
- `docs/superpowers/notes/2026-10-22-greeting-handoff-spike.md` (how the hand-off was measured headlessly).

## Global Constraints

- Every greeter run, QML test and screenshot happens offscreen or in `scripts/headless-kwin.sh <name>`. **Never on the user's display** (`wayland-0`).
- Nothing loaded by `kscreenlocker_greet` or `sddm-greeter-qt6` imports `Quickshell` or `GhostKit`.
- Nothing is switched on the live machine (lock QML, SDDM theme, system fonts, system keymap, KSplash) without asking the user in that moment.
- Don't edit files under `/usr/share/plasma`. Don't delete `glyph` or any other installed SDDM theme. Don't edit `/etc/sddm.conf.d/kde_settings.conf`.
- A greeter that fails to load must fall back to KDE's stock lock screen or SDDM's default. **Never unlocked.**
- Values from the spec and mockup, at mockup scale (×`Theme.u` at runtime):
  - panel 380 wide;
  - cells 9×14 with 3 px gaps, 28 of them, unlit at 6 %;
  - cursor 9×16, blink 250 ms steps `[1, 0.5, 0, 0.5]`;
  - alert: 3 px crit edge, 16 px stripe tile, arrival 1600 ms, then blink for 8000 ms, then hold;
  - scan pass 30 steps over 800 ms with a 2 px line at 55 % `hi`;
  - name 18 px bold in `signal`;
  - actions in the bottom row, 34 px from the bottom.
- Copy, exact:
  - `AUTHENTICATE` with tag `認証`;
  - `LOCKED HH:MM`;
  - `ACCESS DENIED · ATTEMPT n`;
  - `CAPS LOCK`;
  - `Switch user`, `Sleep`, `Restart`, `Shut down` (shown uppercase);
  - session chip `PLASMA (WAYLAND)` (from the session's own name, uppercased).
- Styles: `prompt | terminal | idcheck`. Only `prompt` is built here. The others resolve to `prompt` in the gate until their plans land.
- Each idle greeter costs ≤ 1 % of one core averaged over 60 s, measured as project 1 does. Log the readings in README "Performance".
- Commits: Conventional Commits as in `git log`, with no attribution trailers. The user pushes; never push.

## Review Focus

1. **A password longer than 28 characters:** every cell stays lit and the cursor sits on the last cell. It must not overflow or wrap. *(Task 3 test.)*
2. **A missing, half-written or hand-edited snapshot** (no file, an unknown style, a bad colour, a non-numeric fill): the gate shows style A with the default palette. It must never fail to load. *(Task 3 test.)*
3. **Keys while authentication is running, and Enter on an empty field:** both do nothing. A second submit while busy would double-count attempts. *(Task 4 QML test.)*
4. **A PAM message instead of a plain failure** (a lockout after repeated attempts, an expired account): it replaces `ACCESS DENIED · ATTEMPT n`, still in crit. *(Task 3 test.)*
5. **The configured monitor not connected** (a laptop-style setup, or a model renamed): the prompt still appears on one screen (the first), not on none. *(Task 3 test via `pickScreen`.)*

---

### Task 1: Spike — greeter entry points, file reading, layouts

**Files:**
- Create: `spikes/lock-greeters/` (throwaway QML and scripts)
- Create: `docs/superpowers/notes/2026-10-23-lock-greeters-spike.md`

**Interfaces:**
- Produces: the note. Tasks 5 and 6 cite it for:
  - **(a)** how kscreenlocker 6.7.5 is pointed at a lock QML without touching `/usr/share/plasma`: a user shell package under `~/.local/share/plasma/shells/<id>`, a config key, or other. Include the exact file and key, how to revert, and what makes a lock QML "outdated";
  - **(b)** the QML API the lock greeter gives its root: the authenticator object and its signal and method names, PAM prompt/info/error messages, switch-user and suspend calls, Caps Lock state, keyboard layout list and switch, user name, and the per-screen window model;
  - **(c)** whether `import QtCore` + `Settings { location: "file:///…/gate.ini" }` reads an INI from `$XDG_STATE_HOME` inside the lock greeter's sandbox, and from `/var/lib/ghost-shell/` inside `sddm-greeter-qt6`;
  - **(d)** the SDDM theme metadata keys needed for the Qt 6 greeter (`QtVersion=6`, `X-SDDM-Theme-InterfaceVersion`) and what `sddm` and `keyboard` expose on 0.21 under Wayland (layouts, current layout, whether it's writable);
  - **(e)** whether a QML module dir shipped inside each theme/package is found by a plain relative `import "GhostGate"`, or needs `QML_IMPORT_PATH` (which the greeters don't let us set).

- [ ] **Step 1: Read the sources.** Read kscreenlocker 6.7.5's greeter (`greeter/greeterapp.cpp`, `greeter/authenticator.*`) and SDDM 0.21's `src/greeter/` (`GreeterProxy`, `KeyboardModel`, `ThemeConfig`) at the installed versions. Cite file:line in the note.
- [ ] **Step 2: Try (a)–(c) and (e) headlessly.**
  - Start `scripts/headless-kwin.sh lockspike`.
  - Run `/usr/libexec/kscreenlocker_greet --testing --theme <spike package>` against it with a minimal QML that prints what it finds.
  - Do the same with `sddm-greeter-qt6 --test-mode --theme spikes/lock-greeters/sddm`.
  - Record what each printed.
- [ ] **Step 3: Write the note.** Give a top line answering each of (a)–(e), the commands used, and any answer that changes this plan. If (c) fails for either greeter, the note proposes the replacement reader (for SDDM, `theme.conf.user` symlinked to `/var/lib/ghost-shell/gate.ini` and read through `config`), and Tasks 5 and 6 use it. **If (a) has no route without editing `/usr/share/plasma`, stop and ask the user.**
- [ ] **Step 4: Commit**

```bash
git add spikes/lock-greeters docs/superpowers/notes/2026-10-23-lock-greeters-spike.md
git commit -m "docs(spike): find how both greeters load a theme, read the gate snapshot and switch layouts"
```

---

### Task 2: `[lock] style` and the gate snapshot in ghostd

**Files:**
- Modify: `crates/ghost-proto/src/config.rs`: add `Lock`, `LockStyle`, and the field `lock: Lock` on `ResolvedConfig`
- Modify: `crates/ghostd/src/config/default.toml`: a `[lock]` section, `style = "prompt"`, with a one-line comment listing the three values
- Create: `crates/ghostd/src/gate.rs`
- Modify: `crates/ghostd/src/lib.rs` (module) and `crates/ghostd/src/main.rs` (spawn, not in `--fake`)
- Test: `crates/ghostd/src/gate.rs` (unit), `crates/ghostd/src/config/load.rs` tests

**Interfaces:**
- Produces:
  - `ghost_proto::LockStyle { Prompt (default), Terminal, Idcheck }`, serde lowercase;
  - `ghost_proto::Lock { style: LockStyle }`, `#[serde(default, deny_unknown_fields)]`;
  - `gate::snapshot_ini(config: &ResolvedConfig, host: &str) -> String`;
  - `gate::write_atomic(path: &Path, contents: &str) -> io::Result<()>`;
  - `gate::snapshot_paths() -> Vec<PathBuf>`;
  - `gate::keep(updates: watch::Receiver<ResolvedConfig>, host: String, paths: Vec<PathBuf>)` (async, never returns).
- **INI layout, consumed by Task 3's `parseSnapshot`:**

```ini
[gate]
version=1
style=prompt
host=<hostname>
display_model=<display.model>
[appearance]
font=Kode Mono
font_cjk=Noto Sans CJK JP
japanese=mixed
panel_fill=55
alert_stripe_period_s=1.6
alert_stripe_step_s=1
[colors]
ground=#0a0f11
; …every key of appearance.colors
[dots]
spacing=24
opacity=16
[scanlines]
opacity=15
```

- [ ] **Step 1: Write the failing tests**
  - `load.rs`:
    - `lock_style_defaults_to_prompt`: `load_str("")` gives `LockStyle::Prompt`;
    - `lock_style_reads_terminal`: `[lock]\nstyle = "terminal"` gives `Terminal`;
    - `unknown_lock_style_is_refused`: `style = "fancy"` is `Err` and its message contains `fancy`. The watcher then keeps the last good config and warns, as for any invalid value.
  - `gate.rs`:
    - `snapshot_has_every_key`: for `load_str("")` and host `"ghost"`, every key above is present, with `style=prompt` and `ground=#0a0f11`;
    - `snapshot_reflects_style_and_fill`: `style=idcheck` and `panel_fill=40` come through;
    - `write_atomic_replaces_whole_file`: writing `a` then `b` into a tempdir leaves exactly `b`, with mode `0o644` and no `*.tmp` left behind;
    - `system_path_skipped_when_dir_missing`: `snapshot_paths_in(state: Some(tmp), system_dir: tmp/"absent")` returns only the state path.
- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p ghostd gate lock_style unknown_lock_style`. Expected: compile errors or FAIL.
- [ ] **Step 3: Implement.**
  - Put the state path in a `snapshot_paths_in(state_home: Option<OsString>, home: Option<OsString>, system_dir: &Path)` helper, following `ghost-alerts-inmet/src/store.rs::store_path_in`. That gives `$XDG_STATE_HOME/ghost-shell/gate.ini`, or `~/.local/state/ghost-shell/gate.ini`, and creates the state dir.
  - The system path is `/var/lib/ghost-shell/gate.ini`, included only if `/var/lib/ghost-shell` exists.
  - `write_atomic`: write a sibling `.gate.ini.tmp`, set mode `0o644`, fsync, then rename.
  - `keep` writes once at start, then on every `updates.changed()`. A write error is a `warn!` with the path, never a panic.
  - Host comes from the same source `server::State` uses.
- [ ] **Step 4: Run them to see them pass.** Run `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit**

```bash
git add crates/ghost-proto crates/ghostd
git commit -m "feat(ghostd): add [lock] style and keep a gate snapshot for the lock and login screens"
```

---

### Task 3: GhostGate core — theme, shared files, rules

**Files:**
- Create: `shell/gate/GhostGate/qmldir`, `Theme.qml` (`pragma Singleton`, `QtObject`), `Dots.qml`, `Scanlines.qml`, `lib/gate.mjs`
- Create: `scripts/assemble-gate.sh`
- Modify: `shell/kit/GhostKit/Glyph.qml`: add `keyboard` and `switch-user` shapes, drawn like the mockup's `#i-kb` and `#i-user` symbols, in the existing shape format
- Modify: `shell/kit/GhostKit/lib/attention.mjs` and `AlertBox.qml`: move `arrivalMs`, `arrivalStartPercents` and `arrivalOpacities` into `attention.mjs` as `ARRIVAL_MS`, `ARRIVAL_START_PERCENTS` and `ARRIVAL_OPACITIES`. `AlertBox` reads them from there; its behaviour is unchanged.
- Test: `shell/tests/gate.test.mjs`, `shell/tests/gate-kit.test.mjs`

**Interfaces:**
- Consumes: Task 2's INI layout.
- Produces:
  - **`scripts/assemble-gate.sh <out_dir>`** writes `<out_dir>/GhostGate/`. It contains the gate's own files plus copies of `GhostKit/{Panel,Chip,Glyph}.qml`, `GhostKit/lib/{theme,scale,views,attention}.mjs` and the `.frag.qsb` files that `Dots`, `Scanlines` and the alert use. The copied names are listed once, in `SHARED_FILES` at the top of the script.
  - **Gate `Theme`:**
    - every member the shared files read: the test derives that list, so the list isn't fixed here;
    - `u` (writable, design px → logical px), `d(px)`;
    - `style`, `host`, `displayModel`, `japanese`, `blinkStepMs: 250`, `blinkSteps: [1, 0.5, 0, 0.5]`;
    - `load(snapshot)`, which applies a `parseSnapshot` result.
  - **`lib/gate.mjs`:**
    - `CELLS = 28`;
    - `parseSnapshot(sections: {[section]: {[key]: string}}) -> {style, host, displayModel, appearance}`, filling defaults per key;
    - `resolveStyle(name) -> 'prompt'` (for now, every value resolves to `'prompt'`; unknown values too);
    - `cells(length, count = CELLS) -> {lit: number, cursor: number}`;
    - `nextLayout(layouts: string[], current: string) -> string`;
    - `alertOpacity(msSinceShown) -> number`;
    - `failureText(attempt: number, message: string) -> string`;
    - `actionsFor(mode: 'lock'|'login', allowed: {switchUser, sleep, restart, shutdown}) -> string[]`;
    - `scanStep(ms) -> number` (0…30).
  - `Dots.qml` and `Scanlines.qml` use the same shaders and uniforms as GhostKit's `DotGrid` (with motion off, matching the greeting's resting frame in `shell/host/GreetingScreen.qml`) and `Scanlines`. They read the pixel ratio from `Screen.devicePixelRatio`.

- [ ] **Step 1: Write the failing tests**
  - `gate.test.mjs`:
    - `cells`: `cells(0)` gives `{lit: 0, cursor: 0}`; `cells(5)` gives `{lit: 5, cursor: 5}`; `cells(27)` gives `{lit: 27, cursor: 27}`; `cells(28)` and `cells(40)` both give `{lit: 28, cursor: 27}`.
    - `parseSnapshot`:
      - `{}` gives `style 'prompt'` and the default palette (`ground '#0a0f11'`, `signal '#a3d4cc'`, `panel_fill 55`);
      - `{gate:{style:'fancy'}}` gives `'prompt'`;
      - `{colors:{ground:'red'}}` keeps `'#0a0f11'`;
      - `{appearance:{panel_fill:'abc'}}` keeps `55`.
    - `failureText`: `(2, '')` gives `'ACCESS DENIED · ATTEMPT 2'`; `(3, 'Account locked for 10 minutes')` gives `'ACCOUNT LOCKED FOR 10 MINUTES'`.
    - `alertOpacity`: it follows `ARRIVAL_START_PERCENTS`/`ARRIVAL_OPACITIES` over 0–1600 ms; at 1600 ms it's `1`, at 1850 `0.5`, at 2100 `0`; from 9600 ms on it's `1`.
    - `actionsFor`: `('lock', all)` gives `['switch-user','sleep']`; `('login', all)` gives `['sleep','restart','power']`; `('lock', {switchUser:false, sleep:true})` gives `['sleep']`.
    - `nextLayout(['us','br'],'us')` gives `'br'`, then back to `'us'`; `nextLayout(['us'],'us')` gives `'us'`.
    - `scanStep`: `0` gives 0, `800` gives 30, `400` gives 15.
    - The fifth Review Focus item: `pickScreen([{model:'X'}], 'M27Q')` from `lib/scale.mjs` returns that screen.
  - `gate-kit.test.mjs`:
    - every `Theme.<name>` in the `SHARED_FILES` QML resolves to a property or function declared in `shell/gate/GhostGate/Theme.qml`;
    - no file under `shell/gate/` or in `SHARED_FILES` has `import Quickshell` or `import GhostKit`;
    - every `SHARED_FILES` entry exists. Parse `SHARED_FILES` out of `scripts/assemble-gate.sh` so there's one list.
- [ ] **Step 2: Run them to see them fail.** Run `node --test shell/tests/gate.test.mjs shell/tests/gate-kit.test.mjs`. Expected: FAIL.
- [ ] **Step 3: Implement** the files above. The gate `Theme` computes `u` with `scale.mjs`'s `designUnit`.
- [ ] **Step 4: Run all the tests** with `make test`. Expected: PASS, and the existing `attention` tests still pass after the move.
- [ ] **Step 5: Commit**

```bash
git add shell/gate shell/kit/GhostKit scripts/assemble-gate.sh shell/tests
git commit -m "feat(gate): add the Quickshell-free gate module on GhostKit's shared parts"
```

---

### Task 4: The parts, the adapter contract and style A, tested offscreen

**Files:**
- Create: `shell/gate/GhostGate/{Password,GateAlert,Chips,Actions,ScanPass,GateScreen,Prompt,FakeAdapter}.qml` and add them to the qmldir
- Create: `shell/gate/tests/tst_prompt.qml`
- Modify: `Makefile`: add a `test-gate` target that assembles into a temp dir and runs `QT_QPA_PLATFORM=offscreen qmltestrunner-qt6 -import <tmp> -input shell/gate/tests`. Make `test` depend on it.

**Interfaces:**
- Consumes: Task 3's `Theme`, `lib/gate.mjs`, `Panel`, `Chip`, `Glyph`.
- **Produces the adapter contract.** Every adapter is a `QtObject` with:
  - read-only properties: `string mode` (`'lock'` or `'login'`), `string userName`, `bool busy`, `int attempts`, `string message`, `bool capsLock`, `var layouts`, `string layout`, `var sessions`, `int sessionIndex`, `var allowed` (`{switchUser, sleep, restart, shutdown}`);
  - methods: `submit(password)`, `cycleLayout()`, `chooseSession(index)`, `act(name)`, where `name` is one of `actionsFor`'s values;
  - signals: `succeeded()`, `failed()`.

  `FakeAdapter` accepts `"ghost"`. For tests, its `mode`, `capsLock`, `message` and `holdBusy` (stay busy after a submit) are settable, and `submitCount` counts calls to `submit`.
- **`GateScreen { adapter; primary: bool }`** draws ground, `Dots` and `Scanlines`. On the primary screen it loads the style named by `Theme.style` (via `resolveStyle`); other screens get dots only. On `adapter.succeeded` it plays `ScanPass` (lock mode only) and then emits `done()`. At login there's no pass (spec §6.5), so it emits `done()` at once.
- **`Prompt { adapter }`** implements spec §7.1 using `Panel` (label `AUTHENTICATE`, `tagKey` for `認証`, and meta `LOCKED HH:MM` in lock mode or `HH:MM` at login), `Password`, `GateAlert`, `Chips` and `Actions`.

- [ ] **Step 1: Write the failing test.** `tst_prompt.qml` uses `TestCase` with `when: windowShown` and builds `GateScreen { adapter: FakeAdapter {} ; primary: true }`. The cases:
  - `test_cells_light_per_key`: `keyClick` "abc" leaves 3 lit cells and the cursor at index 3;
  - `test_long_password_caps`: 40 keys leave 28 lit cells;
  - `test_escape_clears`: Esc leaves 0 lit cells;
  - `test_wrong_password_shows_alert`: "nope" + Enter makes `GateAlert` visible with text `ACCESS DENIED · ATTEMPT 1` and clears the field; typing hides the alert;
  - `test_busy_ignores_keys_and_enter`: with `FakeAdapter.holdBusy = true` and after a submit, keys and Enter don't change the cells and `submit` was called once (Review Focus 3);
  - `test_empty_enter_does_nothing`: `submit` is never called;
  - `test_pam_message_replaces_alert`: the fake's `message` = "Account locked" shows `ACCOUNT LOCKED`;
  - `test_caps_chip`: `capsLock = true` shows `CAPS LOCK`;
  - `test_layout_chip_cycles`: clicking the chip goes `US`, then `BR`;
  - `test_lock_vs_login_actions`: the action row reads `SWITCH USER`, `SLEEP` in lock mode and `SLEEP`, `RESTART`, `SHUT DOWN` at login; the session chip only shows at login;
  - `test_japanese_off_hides_tag`: `Theme.japanese = 'off'` leaves no `認証`;
  - `test_success_lock_plays_pass_then_done`: "ghost" + Enter means `done` fires after about 800 ms (`tryCompare` with a 2000 ms timeout);
  - `test_success_login_done_immediately`: `done` within 100 ms.
- [ ] **Step 2: Run it to see it fail.** Run `make test-gate`. Expected: FAIL (types missing).
- [ ] **Step 3: Implement the parts.**
  - Timing comes from `gate.mjs` and runs on one stepped `Timer` per animated part:
    - the cursor at 250 ms;
    - the alert at the arrival step boundaries, then 250 ms, then it stops;
    - the scan pass at 800/30 ms.
  - Nothing uses a continuous `Animation` or binds to the frame rate (§8.4).
  - The password text lives only in a local `TextInput` with `echoMode: TextInput.NoEcho`, which is never shown. Clear it on submit and on Esc.
- [ ] **Step 4: Run it to see it pass.** Run `make test`. Expected: PASS.
- [ ] **Step 5: Render the screenshots** offscreen, at 2560×1440 with the scale at 1.15: lock idle, lock with 12 characters and the alert, and login idle, using `grabToImage` in a helper test. Compare them by eye against the mockup's frame A. Fix any spacing differences, then save the shots to `docs/superpowers/notes/lock-a-shots/` for the user.
- [ ] **Step 6: Commit**

```bash
git add shell/gate Makefile docs/superpowers/notes/lock-a-shots
git commit -m "feat(gate): build the prompt style on the shared password, alert, chip and action parts"
```

---

### Task 5: The lock screen part (`--lock`)

**Files:**
- Create: `packaging/lock/`, a package laid out as spike note (a) says, with the lock entry QML that builds one `GateScreen` per screen with a `LockAdapter`
- Create: `shell/gate/GhostGate/LockAdapter.qml`
- Modify: `scripts/install.sh`: add `--lock` and `--remove-lock`; a plain run stays exactly as today
- Create: `scripts/smoke-lock.sh`

**Interfaces:**
- Consumes:
  - spike note (a), (b), (c), (e);
  - Task 4's contract and `GateScreen`;
  - the snapshot at `$XDG_STATE_HOME/ghost-shell/gate.ini`, read with `Settings { location }` (or note (c)'s replacement) and passed to `Theme.load`.
- Produces: `LockAdapter`, which maps note (b)'s API onto the contract:
  - `mode: 'lock'`;
  - `allowed.switchUser` and `allowed.sleep` come from the greeter's capability flags;
  - `restart` and `shutdown` are always false.

  On `GateScreen.done`, it calls whatever note (b) says ends the greeter after success.

- [ ] **Step 1: Write the smoke.** `scripts/smoke-lock.sh`:
  - starts `headless-kwin.sh smokelock`;
  - assembles and installs the package into that sandbox's `XDG_DATA_HOME`;
  - runs `kscreenlocker_greet --testing` with the package, with the root sized by `Theme.d`;
  - asserts from the greeter's log that the gate root loaded (a `console.info("gate: prompt lock")` line) and that there are no QML warnings;
  - kills KWin.
- [ ] **Step 2: Run it to see it fail.** Run `scripts/smoke-lock.sh`. Expected: FAIL (no package).
- [ ] **Step 3: Implement** `LockAdapter`, the package and the install flags.
  - `--lock` assembles GhostGate into the package and applies note (a)'s pointer, recording the previous value in `~/.local/share/ghost-shell/lock.previous`.
  - `--remove-lock` restores that value and removes the package.
  - If the user's config has no snapshot yet, the install restarts nothing; the gate uses its defaults.
- [ ] **Step 4: Run the smoke to see it pass.** Run `scripts/smoke-lock.sh`. Expected: `smoke-lock: PASS`.
- [ ] **Step 5: Prove the hand-off** (spike 3 of the spec), headlessly, by the method in `2026-10-22-greeting-handoff-spike.md`:
  - lock the sandbox session, with ghost-shell's greeting overlay running against the sandbox;
  - unlock;
  - capture the frames around the exit.

  Pass means the gate's last frame (ground and dots after the pass) and the overlay's resting frame differ by no pixel outside the scanline pattern, and no frame shows the windows. Record the result and the frame timings in `docs/superpowers/notes/2026-10-23-lock-handoff.md`. **If it fails, stop and report to the user; don't ship a flash.**
- [ ] **Step 6: Commit**

```bash
git add packaging/lock shell/gate scripts/install.sh scripts/smoke-lock.sh docs/superpowers/notes/2026-10-23-lock-handoff.md
git commit -m "feat(lock): draw the lock screen with the gate's prompt as an opt-in install part"
```

---

### Task 6: The login part (`--login`)

**Files:**
- Create: `packaging/sddm/ghost-shell/{Main.qml,metadata.desktop,theme.conf}`
- Create: `shell/gate/GhostGate/LoginAdapter.qml`
- Create: `packaging/sddm/zz-ghost-shell.conf` (`[Theme]\nCurrent=ghost-shell`)
- Modify: `scripts/install.sh`: add `--login` and `--remove-login`
- Create: `scripts/smoke-login.sh`

**Interfaces:**
- Consumes:
  - spike note (c), (d), (e);
  - Task 4's contract;
  - the snapshot at `/var/lib/ghost-shell/gate.ini`.
- Produces: `LoginAdapter`:
  - `mode: 'login'`;
  - `userName` is the last user from `userModel` (cycling when there's more than one user is a chip in Prompt only if `userModel.count > 1`);
  - `sessions` and `sessionIndex` come from `sessionModel`, defaulting to the last session;
  - `layouts`, `layout` and `cycleLayout` come from note (d). If switching isn't possible, `layouts` holds just the current one, so the chip doesn't cycle;
  - `submit(pw)` calls `sddm.login(userName, pw, sessionIndex)`;
  - `failed` is `sddm.loginFailed`;
  - `act` maps to `sddm.suspend`, `reboot` and `powerOff`, with `allowed` from `sddm.canSuspend`, `canReboot` and `canPowerOff`.

- [ ] **Step 1: Write the smoke.** `scripts/smoke-login.sh` runs `sddm-greeter-qt6 --test-mode --theme <assembled theme>` in `headless-kwin.sh smokelogin`, then asserts the `gate: prompt login` log line and that there are no QML warnings.
- [ ] **Step 2: Run it to see it fail.** Expected: FAIL.
- [ ] **Step 3: Implement** `Main.qml`, the adapter and the install flags.
  - **`--login`** prints what it's about to change, then runs these with `sudo`:
    - install Kode Mono (plus Chakra Petch Slashed if the config names it) to `/usr/local/share/fonts/ghost-shell/`, then `fc-cache`;
    - install the assembled theme to `/usr/share/sddm/themes/ghost-shell/`;
    - `install -d -o "$USER" -m 0755 /var/lib/ghost-shell`, so ghostd's next write lands there (restart `ghostd` or wait for the next config change);
    - install the drop-in to `/etc/sddm.conf.d/zz-ghost-shell.conf`;
    - **ask y/N** whether to run `localectl set-x11-keymap us,br` (built from the user's `kxkbrc` `LayoutList`), and skip it on no.
  - **`--remove-login`** removes the drop-in, the theme, `/var/lib/ghost-shell` and the system font copy, and leaves the keymap as it is (it says so).
- [ ] **Step 4: Run the smoke to see it pass.** Expected: `smoke-login: PASS`. Also run `scripts/smoke-login.sh --shot` to render one login frame, and add it to `lock-a-shots/`.
- [ ] **Step 5: Commit**

```bash
git add packaging/sddm shell/gate scripts/install.sh scripts/smoke-login.sh docs/superpowers/notes/lock-a-shots
git commit -m "feat(login): draw the SDDM login screen with the gate's prompt as an opt-in install part"
```

---

### Task 7: Acceptance on the live machine

**Files:**
- Create: `docs/superpowers/notes/2026-10-23-lock-a-acceptance.md`
- Modify: `README.md`: document the "Performance" readings, `--lock` and `--login`, and the `[lock]` setting

- [ ] **Step 1: Show the user the shots** from `lock-a-shots/` and **ask** whether to install `--lock` on this machine. If they say yes, run it.
- [ ] **Step 2: Have the user lock with Meta+L.** Check with them:
  - the prompt shows on the M27Q only;
  - a wrong password shows the alert;
  - the right one passes, and the greeting plays with no flash;
  - US/BR switching and Caps Lock work.
- [ ] **Step 3: Have the user sleep and wake.** One greeting plays.
- [ ] **Step 4: Measure** the idle greeter's CPU over 60 s while locked, by project 1's method (`scripts/measure-cpu.sh` against `kscreenlocker_greet`'s pid). Expected: ≤ 1 %. Record it in README "Performance".
- [ ] **Step 5: Ask** whether to install `--login`, including the keymap question. If they say yes, run it.
- [ ] **Step 6: Have the user log out and in.** Check that:
  - the login screen matches the lock screen;
  - US/BR works;
  - the greeting plays.

  Ask whether KDE's splash showed between login and the greeting, and **only if they want it gone**, set `ksplashrc` `[KSplash] Theme=None` and `Engine=none`.
- [ ] **Step 7: Write the acceptance note.** List each check with pass/fail and the readings, then commit.

```bash
git add README.md docs/superpowers/notes/2026-10-23-lock-a-acceptance.md
git commit -m "docs: record lock and login style A acceptance and the greeter's idle cost"
```

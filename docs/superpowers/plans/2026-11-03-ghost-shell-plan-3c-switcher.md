# ghost-shell Plan 3c: the Alt+Tab Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace KDE's Alt+Tab with layout C, "Readout": a flick draws nothing, and holding Alt shows a name list beside one large live preview, in ghost-shell's look.

**Architecture:** A KWin TabBox package (`packaging/switcher/`) whose `main.qml` holds only the KWin wiring. The panel, its rules and its tests live in the existing `GhostGate` module, so `scripts/assemble-gate.sh` carries them and `make test-gate` runs them. Colours come from `gate.ini`, the lock screen's snapshot, through `QtCore`'s `Settings`. An opt-in `install.sh --switcher` installs the package and writes two `[TabBox]` keys, and `--remove-switcher` restores them.

**Tech Stack:** QML in KWin 6.8.2's engine (`org.kde.kwin`), Qt 6.11, Node 24 for `.mjs` tests, `qmltestrunner-qt6`, Rust for one snapshot key, bash, `uv run --with pytest`.

**Spec:** `docs/superpowers/specs/2026-11-03-ghost-shell-plan-3c-switcher-design.md`. Visual reference: `docs/superpowers/specs/mockups/2026-11-03-ghost-shell-switcher-layouts.html`, tab `C READOUT`.

## Decisions the spec left to the builder

1. **App names come from a pure rule, not `.desktop` files.** Short names fit the column, and nothing reads the disk during the 90 ms before the switcher draws. VS Code therefore reads `CODE`, where the mockup says `VS CODE`.
2. **The panel lives in `GhostGate`** so it runs under the existing `make test-gate`. Files stay unloaded on the lock and login screens because nothing there references them.
3. **`gate.ini` keeps version 1.** The new key is additive, as `signals` was, and readers default a missing key.
4. **The backdrop is `Theme.ground` at 60 %**, not the October literal, so palette presets reach it.

## Global Constraints

Plan 3a's process rules apply: never `rm -rf` outside a sandbox, `pkill`, `git reset`/`rebase`/`push`/`stash`. Stage by explicit path. Conventional Commits with **no trailer or attribution of any kind**. WHY-only comments, braces always, files ≤ 800 lines. These add to them.

- **Never touch the user's live KWin before Task 6.** No `qs`, no `--switcher`, no `kwriteconfig6` and no `reconfigure` against the session. Every script under test runs with `env -u DBUS_SESSION_BUS_ADDRESS` and sandboxed `HOME` and `XDG_*`. Headless KWins are named `3c-t<N>…`.
- **Other work shares this checkout.** The palette presets (`crates/ghost-proto/src/config.rs`, `crates/ghostd/src/config/load.rs`, `crates/ghostd/src/config/default.toml`, `shell/kit/GhostKit/Theme.qml`, `crates/ghostd/tests/config_load.rs`, `README.md`) were uncommitted at writing. Never stage or edit those files for them. If `README.md` still carries their changes when a task needs it, stop and ask.
- Stage Markdown in its own command. If the `pre-commit-spec-lint` hook blocks a commit, hand the user the `! git commit` line, since it is their check.
- Before each commit, as relevant: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`; `node --test 'shell/tests/*.test.mjs'`; `make test-gate`; `bash -n` on changed scripts.
- Package id: `org.ghostshell.switcher`. Snapshot key: `[appearance] icons`, values `mono` or `colour`.
- Design values, in design px (1 mockup px): list 366 wide, rows 30 high, 10 rows shown, gap 18, preview 528 × 286, caption 24 high, panel fill 94 %, row text 10, number text 9, preview caption 11, selected row fill `Theme.signal` at 8 %. Opening print 110 ms in 6 steps. `Panel` gives the frame, header, chamfer and brackets.
- Evdev codes for `fake-keys.py`: Alt 56, Tab 15, Shift 42, Esc 1.

## Review Focus

1. **A window title holding markup (`<b>x</b>`)** must show literally. Pinned in Task 3's `test_title_markup_stays_literal`.
2. **A window closing while the switcher is open** must leave a valid selection and no QML error. Pinned in Task 3's `test_a_row_removed_while_open_keeps_a_valid_selection` and Task 5's smoke.
3. **One window, or none** must open without a `TypeError` and read `1 / 1` or `0 / 0`. Pinned in Task 3's `test_one_or_no_windows`.
4. **`--switcher` run twice, or `LayoutName` changed by the user afterwards,** must not lose the original values or overwrite the user's choice. Pinned in Task 4's `test_a_second_install_keeps_the_first_record` and `test_remove_leaves_a_layout_the_user_chose`.
5. **`gate.ini` missing or garbled** must still open, with the default look. Pinned in Task 5's smoke.

---

### Task 1: Prove it in a headless KWin (the gate)

Standalone. **No later task starts until this one's note says every proof passed.** If a proof fails, stop and bring it to the user with what you saw. Do not redesign around it.

**Files:**
- Modify: `scripts/fake-keys.py` (read `+<code>` as press, `-<code>` as release, a bare code as press and release; update the docstring)
- Create: `spikes/switcher/run.sh`, `spikes/switcher/layout/metadata.json`, `spikes/switcher/layout/contents/ui/main.qml`, `spikes/switcher/window.qml`, `spikes/switcher/bar.qml`
- Create: `docs/superpowers/notes/2026-11-03-switcher-spike.md`

**Interfaces:**
- Produces: the hold syntax of `fake-keys.py`, the sandbox recipe Task 5 reuses, and the note's findings on how to look a window up, whether an upgrade needs a new login, and how the effects behave.

- [ ] **Step 1: Build the harness**

Model `run.sh` on `spikes/lock-handoff/run.sh`: the same `env -i` KWin with `KWIN_SCREENSHOT_NO_PERMISSION_CHECKS=1 KWIN_WAYLAND_NO_PERMISSION_CHECKS=1 QT_FORCE_STDERR_LOGGING=1` and `--no-lockscreen`, the same `SPIKE_LIVE_EFFECTS=1` copy of the user's effects and scripts, and `spikes/greeting-handoff/grab.py` for captures.

Before KWin starts, write the sandbox's `kwinrc` with `[TabBox] LayoutName=org.ghostshell.switcher-spike` and `HighlightWindows=false`, and put the layout under `$root/data/kwin/tabbox/`, with `scripts/assemble-gate.sh` run into its `contents/ui`, plus `Duotone.qml`, `shaders/duotone.frag.qsb` and a `Duotone` line in its `qmldir`.

Stand-ins: `window.qml` is a `qs` window that takes its name from the environment, changes colour every second and logs `spike <name> active <bool>`; run two. `bar.qml` is a solid layer-shell strip along the top.

The layout has the shape of §5.1: `KWin.TabBoxSwitcher`, an `Instantiator` with `active: tabBox.visible`, a full-screen `Window` with `Qt.ToolTip | Qt.BypassWindowManagerHint | Qt.FramelessWindowHint`, a 60 % backdrop, one `Panel` and one `KWin.WindowThumbnail`. It logs `spike-layout: open <epoch ms>` on the first `frameSwapped` after each opening, and each row's `windowId`, `caption`, `desktopFileName` and `resourceClass`.

- [ ] **Step 2: Run the proofs and record each result in the note**

Run: `spikes/switcher/run.sh`, then again with `SPIKE_LIVE_EFFECTS=1`.

| # | Proof | Passes when |
|---|---|---|
| 1 | Opens over the bar, live | Holding `+56`, `15`: a capture shows the bar strip dimmed, and two captures 1.5 s apart differ inside the preview |
| 2 | A flick draws nothing | `+56`, `15`, `-56` with no wait, 20 times: no `spike-layout: open`, and the other stand-in logs `active true` each time. Holding 300 ms opens it, as the control |
| 3 | Effects leave it alone | With `SPIKE_LIVE_EFFECTS=1`, `grab.py watch` on the panel's centre during an opening and a close: it settles within 150 ms of its first change on opening and is gone on the next capture on closing, matching the run without effects |
| 4 | `Settings` reads `gate.ini` | A `Settings` inside the delegate logs the file's `colors/signal`. Rewriting the file between two openings logs the new value without a KWin restart |
| 5 | The module loads | `Panel`, `Theme`, `Scanlines` and `Duotone` render, with no `qml`, `ReferenceError`, `TypeError` or "not installed" lines in `kwin.log`, with the user's scripts loaded too |
| 6 | Windows can be looked up | Each row's `windowId` finds its window through `KWin.Workspace.stackingOrder`, comparing `String(internalId)` |
| 7 | Reinstalling | Change the logged marker, replace the package, `reconfigure`: record whether the next opening shows the new marker or needs a new KWin |
| 8 | A broken layout is survivable | With `SPIKE_LAYOUT_BROKEN=load` (an unknown type) and `=open` (a throw when the delegate completes): KWin stays up, and flicks and holds still move focus between the stand-ins. Record what KWin logs and what shows |
| 9 | Nothing loads before the first hold | A log line at the layout's root `Component.onCompleted`: absent after KWin's start and after 5 flicks, present after the first hold |

- [ ] **Step 3: Measure the opening against stock**

With `SPIKE_LAYOUT=thumbnail_grid` and then the spike's layout, record key press to first changed capture (`fake-keys.py`'s log against `grab.py watch`), for the first opening and for a later one. Put the table in the note.

- [ ] **Step 4: Commit and stop**

```bash
git add scripts/fake-keys.py spikes/switcher
git commit -m "test(spikes): prove the switcher layout route on KWin 6.8"
```
```bash
git add docs/superpowers/notes/2026-11-03-switcher-spike.md
git commit -m "docs(notes): record the switcher spike"
```

Report the note's results before starting Task 2. If proof 3 or the timing table disappoints, that is the user's call.

### Task 2: Carry the icon style in the snapshot

**Files:**
- Modify: `crates/ghostd/src/gate.rs` (`KEYS`, `snapshot_ini`, a helper beside `japanese_name`)
- Modify: `shell/gate/GhostGate/lib/gate.mjs` (`DEFAULTS.appearance`, `parseSnapshot`), `shell/gate/GhostGate/Theme.qml`
- Test: `crates/ghostd/src/gate.rs` tests, `shell/tests/gate.test.mjs`

**Interfaces:**
- Produces: `[appearance] icons="mono"|"colour"` in `gate.ini`, and `Theme.icons: string` in `GhostGate`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn snapshot_carries_the_bar_icon_style() {
    assert!(default_ini().contains("\nicons=\"mono\"\n"));
    let mut config = /* as snapshot_reflects_style_and_fill builds it */;
    config.bar.icons = IconStyle::Colour;
    assert!(ini_of(&config).contains("\nicons=\"colour\"\n"));
}
```

```js
test('parseSnapshot reads the icon style and defaults one it cannot use', () => {
  assert.equal(parseSnapshot({}).appearance.icons, 'mono');
  assert.equal(parseSnapshot({ appearance: { icons: 'colour' } }).appearance.icons, 'colour');
  assert.equal(parseSnapshot({ appearance: { icons: 'neon' } }).appearance.icons, 'mono');
});
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ghostd --lib gate && node --test shell/tests/gate.test.mjs`
Expected: FAIL (no `icons` key; `undefined`).

- [ ] **Step 3: Implement**

Write the key in `snapshot_ini`, following `japanese_name`. In `gate.mjs`, validate as `JAPANESE_MODES` validates `japanese`. Add `readonly property string icons: appearance.icons` to the gate's `Theme`. `readSnapshot` picks it up from `DEFAULTS` by itself.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ghostd --lib gate && node --test shell/tests/gate.test.mjs && make test-gate`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/gate.rs shell/gate/GhostGate/lib/gate.mjs shell/gate/GhostGate/Theme.qml shell/tests/gate.test.mjs
git commit -m "feat(ghostd): carry the bar's icon style in the gate snapshot"
```

### Task 3: The panel, its rules and the package

**Files:**
- Create: `shell/gate/GhostGate/lib/switcher.mjs`, `shell/gate/GhostGate/SwitcherPanel.qml`, `shell/gate/GhostGate/SwitcherIcon.qml`
- Create: `packaging/switcher/org.ghostshell.switcher/metadata.json`, `packaging/switcher/org.ghostshell.switcher/contents/ui/main.qml`
- Modify: `shell/gate/GhostGate/qmldir`, `scripts/assemble-gate.sh` (`SHARED_FILES` gains `Duotone.qml` and `shaders/duotone.frag.qsb`)
- Test: `shell/tests/switcher.test.mjs`, `shell/gate/tests/tst_switcher.qml`

**Interfaces:**
- Consumes: `Theme.icons` (Task 2); the lookup and module findings of Task 1's note.
- Produces, in `lib/switcher.mjs`: `PRINT_MS = 110`, `PRINT_STEPS = 6`, `appName(desktopFileName: string, resourceClass: string, caption: string) -> string`, `titleWithoutApp(caption: string, app: string) -> string`, `rowNumber(index: number) -> string`.
- Produces, `SwitcherPanel`: `property var model` with roles `caption`, `windowId`, `icon`; `property int currentIndex`; `property var nameOf` (`(windowId, caption) => string`); `default property alias preview`; `readonly property var currentWindowId`; `readonly property int printStep`; `signal activated(int index)`.
- `SwitcherIcon`: `property var source` (a `QIcon` or a theme name), `property bool selected`. It draws `Kirigami.Icon` plainly when `Theme.icons === "colour"`, and otherwise through `Duotone` with a transparent shadow and `selected ? Theme.hi : Theme.dim`.

- [ ] **Step 1: Write the failing rule tests**

```js
for (const [desktop, cls, caption, want] of [
  ['com.mitchellh.ghostty', 'com.mitchellh.ghostty', 'zsh', 'GHOSTTY'],
  ['thorium-browser', 'thorium-browser', 'x', 'THORIUM'],
  ['org.kde.dolphin', 'dolphin', 'Downloads — Dolphin', 'DOLPHIN'],
  ['code', 'Code', 'x', 'CODE'],
  ['', 'Slack', '#general - Slack', 'SLACK'],
  ['thorium-hnpfjngllnobngcgfapefoaidbinmjnm-Default', 'thorium-browser', 'WhatsApp', 'WHATSAPP'],
  ['', '', 'Some Window', 'SOME WINDOW'],
  ['', '', '', 'WINDOW'],
]) {
  test(`appName of ${desktop || cls || caption || 'nothing'} is ${want}`, () => {
    assert.equal(appName(desktop, cls, caption), want);
  });
}

for (const [caption, app, want] of [
  ['Toast.qml — ghost-shell — Visual Studio Code', 'CODE', 'Toast.qml — ghost-shell'],
  ['KWin/TabBox – Mozilla Firefox', 'FIREFOX', 'KWin/TabBox'],
  ['#general - Slack', 'SLACK', '#general'],
  ['Derezzed - Daft Punk', 'SPOTIFY', 'Derezzed - Daft Punk'],
  ['Slack', 'SLACK', 'Slack'],
  ['', 'CODE', ''],
]) {
  test(`titleWithoutApp keeps "${want}" of "${caption}"`, () => {
    assert.equal(titleWithoutApp(caption, app), want);
  });
}

test('rowNumber pads to two digits', () => {
  assert.equal(rowNumber(0), '01');
  assert.equal(rowNumber(11), '12');
  assert.equal(rowNumber(99), '100');
});
```

The rule: strip `.desktop`; a Chromium-style web app id (`<browser>-<32 lowercase letters>-<profile>`) takes the caption's last segment; otherwise take the last dot segment of the desktop id, or else of the class, and drop a `-browser`, `-bin`, `-stable` or `-desktop` suffix; with no id, the caption's last segment; with nothing, `WINDOW`. Uppercase the result. A caption's segments split on ` — `, ` – ` and ` - `. `titleWithoutApp` drops the last segment only when it contains `app` case-insensitively and something remains.

- [ ] **Step 2: Write the failing panel tests**

In `tst_switcher.qml`, in the style of `tst_prompt.qml` (`TestCase`, `when: windowShown`, its `texts()` and `find()` helpers), with a `ListModel` for the model and `Theme.fit()` at 2560:

```qml
function test_rows_header_and_count() {
    const panel = makePanel(rows(3), 1);
    verify(texts(panel).includes("SWITCH WINDOW"));
    verify(texts(panel).includes("窓切替"));
    verify(texts(panel).includes("2 / 3"));
    verify(texts(panel).includes("01"));
    compare(panel.currentWindowId, "w1");
}
function test_japanese_off_hides_the_tag() { /* Theme.load(parseSnapshot({appearance:{japanese:"off"}})); no 窓切替 */ }
function test_title_markup_stays_literal() { /* caption "<b>x</b>": every Text has textFormat PlainText and shows the tags */ }
function test_text_is_plain_at_once() { /* the rows' texts are right immediately after creation: no scramble */ }
function test_size_does_not_change_with_the_list() { /* width and height equal with rows(3) and rows(15) */ }
function test_selection_stays_visible_past_ten_rows() { /* rows(15), currentIndex 14: its row is inside the list's viewport; "chevron-up" visible, "chevron-down" not */ }
function test_a_row_removed_while_open_keeps_a_valid_selection() { /* rows(3), currentIndex 2, remove(2), then currentIndex 1: no warning, currentWindowId "w1" */ }
function test_one_or_no_windows() { /* rows(1) reads "1 / 1"; rows(0) reads "0 / 0" and currentWindowId is undefined, no TypeError */ }
function test_click_activates_its_row() { /* mouseClick on row 2: activated(2) */ }
function test_print_runs_once_to_full() { /* printStep starts below PRINT_STEPS; tryCompare(panel, "printStep", PRINT_STEPS, PRINT_MS * 3) */ }
function test_fits_a_smaller_screen() { /* Theme.fit(1920): the panel's width is below 1920 */ }
```

Call `failOnWarning(/.?/)` in `init()`, so any QML warning fails the test.

- [ ] **Step 3: Run them to see them fail**

Run: `node --test shell/tests/switcher.test.mjs && make test-gate`
Expected: FAIL (no such module; `SwitcherPanel` is not a type).

- [ ] **Step 4: Implement `lib/switcher.mjs`, `SwitcherIcon` and `SwitcherPanel`**

`SwitcherPanel` is a `Panel` with `label: "SWITCH WINDOW"`, `tagKey: "switch"`, `scrambles: false`, the `n / N` as `meta`, and `fill: ThemeLib.withAlpha(Theme.panel, 94)`, holding a `ListView` (`interactive: false`, `clip: true`, `positionViewAtIndex(currentIndex, ListView.Contain)` on change, no move animation) and the preview box with its caption. Rows have fixed height, `textFormat: Text.PlainText`, and elide on the right. Selection looks as the mockup's `.row.sel`. Use `Glyph`'s `chevron-up` and `chevron-down` at the list's ends when rows are hidden. The print clips a container to `height * printStep / PRINT_STEPS`, stepped by a `Timer` that stops at the end. Only CJK characters use `Theme.fontCjk`, which `Panel` already does for its tag.

Add `SwitcherPanel`, `SwitcherIcon` and `Duotone` to `qmldir`.

- [ ] **Step 5: Write `main.qml` and `metadata.json`**

`metadata.json`: `"KPackageStructure": "KWin/WindowSwitcher"`, `KPlugin.Id` the package id, a `Name`.

`main.qml` is wiring only, on Task 1's proven shape: `import "GhostGate"`; an `Instantiator` with `active: tabBox.visible` whose delegate is the full-screen `Window`, with a backdrop `Rectangle` at `ThemeLib.withAlpha(Theme.ground, 60)` and a centred `SwitcherPanel` holding `KWin.WindowThumbnail { anchors.fill: parent; wId: panel.currentWindowId }`. A `Settings` inside the delegate loads `Theme.load(GateLib.readSnapshot(...))` as `LockScreen.qml` does, and calls `Theme.fit(tabBox.screenGeometry.width)`. `nameOf` looks the window up as Task 1 proved and calls `appName`. `onActivated` calls `tabBox.model.activate(index)`. Log `switcher: open` once on the first `frameSwapped` of each opening; the smoke reads it.

- [ ] **Step 6: Run the tests to see them pass**

Run: `node --test shell/tests/switcher.test.mjs && make test-gate`
Expected: PASS, with `tst_prompt` unchanged.

- [ ] **Step 7: Commit**

```bash
git add shell/gate/GhostGate/lib/switcher.mjs shell/gate/GhostGate/SwitcherPanel.qml shell/gate/GhostGate/SwitcherIcon.qml shell/gate/GhostGate/qmldir scripts/assemble-gate.sh shell/tests/switcher.test.mjs shell/gate/tests/tst_switcher.qml packaging/switcher
git commit -m "feat(switcher): add the Readout layout as a KWin window switcher package"
```

### Task 4: `install.sh --switcher` and `--remove-switcher`

**Files:**
- Modify: `scripts/install.sh` (variables beside `lock_*`, two functions beside `install_lock`, the `case` and its usage line)
- Test: `scripts/test_install_switcher.py`
- Modify: `README.md`, "Install": one paragraph after the lock's, covering what it writes, how to undo it, when it takes effect (per Task 1's proof 7), and the way back from spec §5.6

**Interfaces:**
- Consumes: `packaging/switcher/org.ghostshell.switcher`, `scripts/assemble-gate.sh`.
- Produces: the package at `$data_home/kwin/tabbox/org.ghostshell.switcher`, and the record `$app_dir/switcher.previous` with one `<key>=<value>` or `<key> unset` line per key.

- [ ] **Step 1: Write the failing tests**

Run `install.sh` in a temp `HOME` with `XDG_*` under it and `env -u DBUS_SESSION_BUS_ADDRESS`, so it can never reach the live KWin, as `scripts/test_term_theme.py` does. Seed `kwinrc` through `kwriteconfig6` so it is in canonical form.

```python
def test_install_sets_the_layout_and_turns_highlight_off(tmp_path): ...
    # [TabBox] LayoutName=org.ghostshell.switcher, HighlightWindows=false;
    # the package has contents/ui/main.qml and contents/ui/GhostGate/qmldir
def test_remove_restores_kwinrc_byte_for_byte_when_the_keys_were_unset(tmp_path): ...
def test_remove_restores_kwinrc_byte_for_byte_when_the_keys_were_set(tmp_path): ...
def test_kdedefaults_does_not_count_as_set(tmp_path): ...
    # kdedefaults/kwinrc has LayoutName, kwinrc has none: removal deletes the key rather than writing it
def test_a_second_install_keeps_the_first_record(tmp_path): ...
def test_remove_leaves_a_layout_the_user_chose(tmp_path): ...
    # LayoutName changed after install: left as it is, a warning names it, HighlightWindows still restored
def test_a_failed_assemble_keeps_the_working_package(tmp_path): ...
    # second install with GHOST_SWITCHER_ASSEMBLE=/bin/false exits non-zero; main.qml still there; kwinrc untouched
def test_reconfigure_failing_only_warns(tmp_path): ...
def test_remove_works_with_no_display_and_no_bus(tmp_path): ...
    # env -i with only HOME and PATH, as a text console could be: exit 0, kwinrc restored
def test_a_hung_kwin_cannot_hang_removal(tmp_path): ...
    # a qdbus-qt6 stub on PATH that sleeps 60 s: --remove-switcher exits 0 within 15 s, kwinrc restored
```

- [ ] **Step 2: Run them to see them fail**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_install_switcher.py -q`
Expected: FAIL ("unknown option --switcher").

- [ ] **Step 3: Implement**

`install_switcher`: `require_tools kwriteconfig6`, `require_shaders`; stage with `mktemp -d`, as `install_login` does; run `"${GHOST_SWITCHER_ASSEMBLE:-$repo/scripts/assemble-gate.sh}" "$stage/contents/ui"` (the override exists only so a test can make the build fail, say so in one line); swap the stage in only after that succeeds; record the previous values once; write the two keys with `kwriteconfig6 --file kwinrc --group TabBox`; `timeout 5 qdbus-qt6 org.kde.KWin /KWin reconfigure || warn`, with a one-line comment that a hung KWin must not hang the way back. Mention `[TabBox]` only; `[TabBoxAlternative]` stays untouched.

Read the previous values by parsing `kwinrc`'s `[TabBox]` group directly. `kreadconfig6` also reads `kdedefaults`: on this machine it reports `org.kde.breeze.desktop` with no such group in `kwinrc`, and that must count as unset.

`remove_switcher`: restore per the record, using `kwriteconfig6 --delete` for an unset key; leave `LayoutName` and warn if it is no longer ours; remove the package and the record; reconfigure the same way. It needs no shaders, no display and no bus. It ends by saying KWin picks the change up at its next start if the reconfigure failed.

- [ ] **Step 4: Run them to see them pass**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_install_switcher.py -q && bash -n scripts/install.sh`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/install.sh scripts/test_install_switcher.py
git commit -m "feat(install): add an opt-in --switcher that restores KWin's settings on removal"
```
```bash
git add README.md
git commit -m "docs(readme): describe the switcher's install"
```

### Task 5: `make smoke-switcher`

**Files:**
- Create: `scripts/smoke-switcher.sh`, `scripts/smoke-window.qml`
- Modify: `Makefile` (`smoke-switcher: shaders`, `.PHONY`)

**Interfaces:**
- Consumes: Task 1's sandbox recipe, the hold syntax of `fake-keys.py`, the `switcher: open` line, and `install.sh --switcher`.

- [ ] **Step 1: Write the smoke**

Follow `scripts/smoke-lock.sh` for its structure, traps and `warning_pattern`: a headless KWin, the real `install.sh --switcher` in the sandbox, three stand-in windows that log `active`. Failing checks fail the script and keep its logs.

| Check | Input | Asserts |
|---|---|---|
| flick | `+56 15 -56`, 10 times | no `switcher: open`; focus alternates |
| hold | `+56 15`, wait 300 ms, `-56` | one `switcher: open`; the expected window active |
| step | `+56 15`, wait, `15 15`, `-56` | the third-next window active |
| back | `+56 15`, wait, `+42 15 -42`, `-56` | one step back |
| cancel | `+56 15`, wait, `1`, `-56` | focus unchanged |
| closing | `+56 15`, wait, kill a stand-in, `-56` | no warning; a live window active |
| no snapshot | `gate.ini` removed | opens |
| garbled snapshot | `gate.ini` holds noise | opens |
| new colour | `gate.ini`'s `signal` changed between openings | a capture's selected border has the new colour |

Finish with: no `warning_pattern` match in `kwin.log`, and `install.sh --remove-switcher` leaves the sandbox's `kwinrc` as it began.

- [ ] **Step 2: Run it**

Run: `make smoke-switcher`
Expected: `smoke-switcher: PASS`. A failure is a bug in Task 3 or 4: fix it there, with a test that fails first.

- [ ] **Step 3: Commit**

```bash
git add scripts/smoke-switcher.sh scripts/smoke-window.qml Makefile
git commit -m "test(switcher): smoke the flick, the browse and a broken snapshot in a headless KWin"
```

### Task 6: Live install, measurements and acceptance, with the user

Nothing here starts without the user present and saying yes. Baselines come first because the install changes what they measure.

**Files:**
- Modify: `README.md`, "Performance": a `### Plan 3c: the switcher` section
- Create: `docs/superpowers/notes/2026-11-03-plan3c-acceptance.md`

- [ ] **Step 1: Measure stock**

Read `kwin_wayland`'s CPU from `/proc/<pid>/stat` over 60 s with the switcher closed and record its `VmRSS`. Then ask the user to hold Alt+Tab open for 30 s on the same windows and read it again.

- [ ] **Step 2: Install and rehearse the way back, before any hold**

The headless runs cannot stand in for NVIDIA, so the first hold on this desktop is the risky moment (spec §5.6). Go over that section's way back with the user first. Then, with their yes and without holding Alt+Tab:

1. Run `scripts/install.sh --switcher`, after `make shaders`.
2. The user switches to a text console (`Ctrl+Alt+F3`), logs in and runs `scripts/install.sh --remove-switcher` there. Record its output.
3. Back in the session, confirm `kwinrc` has its original `[TabBox]` values.
4. Run `scripts/install.sh --switcher` again. Take effect as Task 1's proof 7 found: if it needs a new KWin, the user logs out and in.

If step 2 fails, stop: there is no first hold until the way back works.

- [ ] **Step 3: First hold**

The user saves their work, then holds Alt+Tab. If KWin hangs, follow the way back and bring what happened to the user's attention before trying anything else.

- [ ] **Step 4: Measure again**

Repeat Step 1, plus `VmRSS` after a first opening. Put both sets beside Task 1's opening-time table in the README, with the date and HEAD. If anything is noticeably worse than stock, stop and bring it to the user.

- [ ] **Step 5: Walk the acceptance list with the user**

Flick; browse; Shift back; Esc; click a row; `` Alt+` ``; more than 10 windows; a minimised window; a window closing while open; a palette change showing at the next Alt+Tab; `journalctl --user` clean of `switcher` warnings. Then `install.sh --remove-switcher`, a stock Alt+Tab, and `--switcher` again. Record each result in the note, including anything not checked. The app name reading `CODE` for VS Code gets an explicit look, since it differs from the mockup.

- [ ] **Step 6: Commit**

```bash
git add README.md
git commit -m "docs(readme): record the switcher's cost"
```
```bash
git add docs/superpowers/notes/2026-11-03-plan3c-acceptance.md
git commit -m "docs(notes): record plan 3c's acceptance"
```

## Self-review notes

- **Spec coverage:** §3 behaviour is Tasks 3 and 5. §4 layout is Task 3. §5.1 is Tasks 1 and 3. §5.2 is Task 3, with the re-read in proof 4 and the smoke. §5.3 is Tasks 2 and 3. §5.4 is Task 3 and proof 6. §5.5 is Task 4. §5.6 is proofs 8 and 9, Task 4's two tests and its README paragraph, and Task 6's Steps 2 and 3. §6 is Task 1. §7 is Tasks 1 and 6. §8 is Tasks 3, 4, 5 and 6.
- **Left out on purpose:** the audio items (M9, M4, C24), which the spec makes a separate small change after this plan.
- **Open for Task 1 to settle:** the exact `windowId` lookup, and whether `reconfigure` is enough after an upgrade. Tasks 3, 4 and 6 read the note's answers.

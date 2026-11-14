# ghost-shell Audit Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 32 findings of the 2026-10-26 audit: the Plasma restore data loss, the notification server's unbounded inputs, the daemon freezes a hung collector or mount can cause, the install and terminal script hazards, the notification UI bugs, and the dead code.

**Architecture:** Every task is a small, independent change in the file the audit names, with the one test that fails without it. Daemon fixes stay inside the module that owns the rule (`history.rs` for cap and ids, `server.rs` for what a call may carry, `state.rs` for passing records and signals, `sampling.rs` for the tick deadline). Script fixes add a guard or a staging step next to the destructive line. Shell fixes live in `lib/*.mjs` with node tests, and the QML only calls them. One wire change: `Notification.updated_s`, defaulted for old history files.

**Tech Stack:** Rust 1.98 (tokio, zbus 6, serde, jiff already in the workspace), Quickshell 0.2.1 QML, Node 24 for `.mjs` tests, bash plus `uv run --with pytest` for the terminal scripts, the sandboxed Plasma of `spikes/plasma-hidden/`.

**Spec:** `docs/superpowers/notes/2026-10-26-audit.md` (the finding numbers below are its numbers). Rules it rests on: `docs/superpowers/specs/2026-10-24-ghost-shell-plan-3b-notifications-design.md` §"Calls" 6 and 7 (images, markup) and the Desktop Notifications Specification 1.2 (`replaces_id`, close reasons, `image-path` forms).

## Decisions

1. **[call] Unread criticals are capped too**, at `2 × history_limit`, oldest first. The 3b design said "never dropped"; the audit shows that is an unbounded resource any app controls. README and `default.toml` wording change with it.
2. **[call] `launcher.style` is deleted** (finding 20). `deny_unknown_fields` then refuses a user config with a `[launcher]` table. **Before Task 20 ships, the user checks `~/.config/ghost-shell/config.toml` for a `[launcher]` table and removes it themselves**; the plan never touches that file.
3. **[call] `make smoke` refuses the live compositor** unless `GHOST_SMOKE_LIVE=1` is set (finding 29), so an agent cannot paint on the user's display by mistake; the user opts in with one variable.
4. **[call] `Notification` gains `updated_s`** (the wall-clock second of the record's last write) so a replacement of an old record can toast; `arrived_s` keeps its meaning. Old history files read `updated_s = arrived_s`.
5. **[call] The collector deadline is `30 × tick`** (30 s at 1 Hz, 1.5 s in the 50 ms test tick), reported as a new `ServeError::SamplingHung`; the process already exits non-zero and systemd restarts it.

## Global Constraints

Plan 3b's Global Constraints apply unchanged (process rules: never `rm -rf`, `pkill`, `git reset`/`rebase`/`push`/`stash`; stage by explicit path; no `qs` on the user's display; never touch the running services, `~/.config/ghost-shell/` or the user's Plasma; Conventional Commits with **no trailer or attribution of any kind**; WHY-only comments; braces always; files ≤ 800 lines). These add to them.

- Before every commit, for the crates it touches: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. For `shell/`: `node --test 'shell/tests/*.test.mjs'`. For `scripts/*.sh`: `bash -n` on the file. For `scripts/term-*.sh`, `install-term.sh`, `term-theme.py`: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py`.
- Smokes use `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-audit-t<N>.sock`; headless KWins are named `audit-t<N>…`; every `qs` run sets `GHOST_GREETING=${GHOST_GREETING:-off}`.
- Tasks 1 and 2's sandbox tests (`spikes/plasma-hidden/test-plasma-panel.sh`, `scripts/smoke-lock.sh`) run only in their sandboxes; no live `retire`, `restore` or `install.sh --lock`.
- A test must fail on the code before the fix. Where the plan says "Run: … Expected: FAIL", check that before implementing.
- Install nothing on the live desktop from this plan; the user installs after review (`reinstall-after-each-task` applies to the user's own reinstall, not to agents).

## Review Focus

1. **`image-path` naming a FIFO, a directory or `/dev/zero`** — must leave the record without a picture and never block a reader. Pinned in Task 9's `image_path_that_is_not_a_regular_file_is_dropped`.
2. **A history file written before `updated_s` existed** — must load, with every item's `updated_s` equal to its `arrived_s`, so no old record toasts at start. Pinned in Task 10's `old_history_files_read_updated_as_arrived`.
3. **`restore` when the live file was deleted after the retire** — the before-restore copy step must tolerate a missing file and still put the backup back. Pinned in Task 1's sandbox check "restore with a deleted live file still restores".
4. **A transient replace of an id that has already expired** — must get a fresh id, as the spec says for an unknown `replaces_id`, and not touch another record. Pinned in Task 7's `a_transient_replace_of_an_expired_id_is_new`.
5. **A body of 100 000 nested `<b>` tags** — must be cut to the body cap before the sanitiser runs, so `open` stays bounded and the markup closes cleanly. Pinned in Task 6's `oversized_fields_are_cut_to_their_caps`.

---

## Must fix

### Task 1: Restore keeps a copy of the live Plasma files (finding 1)

**Files:**
- Modify: `scripts/plasma-panel.sh:489-502` (the `file`/`absent` loop in `restore`)
- Modify: `scripts/plasma-notifications.sh:198-210` (the same loop in `restore_notifications`)
- Modify: `scripts/plasma-lib.sh` (new helper)
- Test: `spikes/plasma-hidden/test-plasma-panel.sh` (`run_test`, after "== restore")

**Interfaces:**
- Produces: `keep_before_restore <backup_dir> <config_home> <file>` in `plasma-lib.sh`: copies `<config_home>/<file>` with `cp -p` into `<backup_dir>/before-restore/<file>` when it exists; a missing live file is not an error.

- [ ] **Step 1: Add the sandbox checks**

In `run_test`, between `try retire` succeeding and `try restore`, change one backed-up file in the sandbox and delete another:

```bash
echo '# drift after retire' >> "$root/config/plasmashellrc"
rm -- "$root/config/kglobalshortcutsrc"
```

After `try restore` add:

```bash
check "restore kept the drifted plasmashellrc" grep -q '^# drift after retire$' "$backup/before-restore/plasmashellrc"
check "restore with a deleted live file still restores" same_files "$backup" "$root/config"
```

`same_files` already compares the backup against `$root/config`; `before-restore/` must be excluded from that comparison (adjust `same_files` to skip the `before-restore` directory if it walks the backup dir).

- [ ] **Step 2: Run the sandbox test to see the new checks fail**

Run: `spikes/plasma-hidden/test-plasma-panel.sh`
Expected: "restore kept the drifted plasmashellrc" FAILS (no `before-restore/` dir); everything else as before.

- [ ] **Step 3: Implement `keep_before_restore` and call it**

In both restore loops, inside the `file)` and `absent)` branches, call `attempt keep_before_restore "$backup" "$config_home" "$file"` before the `cp`/`rm`. In `restore` print one `step` naming `$backup/before-restore` once.

- [ ] **Step 4: Run the sandbox tests**

Run: `spikes/plasma-hidden/test-plasma-panel.sh && spikes/plasma-hidden/test-plasma-notifications.sh && bash -n scripts/plasma-panel.sh scripts/plasma-notifications.sh scripts/plasma-lib.sh`
Expected: `failures: 0` in both.

- [ ] **Step 5: Document it**

README "Retiring the Plasma panel", the `restore` paragraph: add one sentence: the live copies go to `plasma-backup/<stamp>/before-restore/` first.

- [ ] **Step 6: Commit**

```bash
git add scripts/plasma-panel.sh scripts/plasma-notifications.sh scripts/plasma-lib.sh spikes/plasma-hidden/test-plasma-panel.sh README.md
git commit -m "fix(scripts): keep the live Plasma files beside the backup before a restore puts the old ones back"
```

### Task 2: `install.sh --lock` stages the package before swapping it (finding 5)

**Files:**
- Modify: `scripts/install.sh:322-326` (`install_lock`), `scripts/install.sh:361-366` (`remove_lock`)
- Test: `scripts/smoke-lock.sh:131-134`

- [ ] **Step 1: Add the failure check to the smoke**

Right after the existing `install.sh --lock` call in `smoke-lock.sh`, run a second install that must fail without harming the first: point `GHOST_LOCK_ASSEMBLE` (new env override, see step 3) at `/bin/false` and assert the package still holds `contents/lockscreen/LockScreen.qml`:

```bash
env -u DBUS_SESSION_BUS_ADDRESS HOME="$root" XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
    XDG_STATE_HOME="$root/state" XDG_RUNTIME_DIR="$root/run" GHOST_LOCK_ASSEMBLE=/bin/false \
    "$repo/scripts/install.sh" --lock > "$root/install-fail.log" 2>&1 && fail "a failing assemble did not fail install.sh --lock"
[[ -f $root/data/plasma/shells/$package/contents/lockscreen/LockScreen.qml ]] \
    || fail "a failed --lock removed the working package"
```

- [ ] **Step 2: Run the smoke to see it fail**

Run: `make smoke-lock`
Expected: FAIL at "a failed --lock removed the working package".

- [ ] **Step 3: Stage first**

In `install_lock`, mirror `install_login` (`install.sh:418-439`): `stage=$(mktemp -d)`, a global `lock_stage=$stage` and `trap 'rm -rf -- "$lock_stage"' EXIT`. Copy `$lock_package_src/.` into the stage, run `"${GHOST_LOCK_ASSEMBLE:-$repo/scripts/assemble-gate.sh}" "$stage/contents/lockscreen"`, then `rm -rf -- "$lock_package"; mkdir -p -- "$lock_package"; cp -r -- "$stage/." "$lock_package/"`. The env override exists only so the smoke can make the assemble fail; say so in a one-line comment.

In `remove_lock`: in the `warn "left $lock_dropin in place…"` branch, skip `rm -rf -- "$lock_package"` and say the package is kept because the drop-in still names it.

- [ ] **Step 4: Run the smoke and the syntax check**

Run: `bash -n scripts/install.sh && make smoke-lock`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/install.sh scripts/smoke-lock.sh
git commit -m "fix(install): build the lock package in a stage so a failed --lock keeps the working one"
```

### Task 3: `install-term.sh` refuses an unbalanced marker (finding 6)

**Files:**
- Modify: `scripts/install-term.sh:50-52`
- Test: `scripts/test_term_theme.py`

- [ ] **Step 1: Write the failing test**

```python
def test_install_refuses_a_zshrc_whose_end_marker_is_missing(tmp_path):
    home = _home(tmp_path)
    _run_script("install-term.sh", home)
    text = (home / ".zshrc").read_text()
    (home / ".zshrc").write_text(text.replace("# <<< ghost-shell terminal <<<\n", ""))
    before = (home / ".zshrc").read_text()
    result = _run_script("install-term.sh", home)
    assert result.returncode != 0
    assert "marker" in result.stderr
    assert (home / ".zshrc").read_text() == before
```

- [ ] **Step 2: Run it to see it fail**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py -k end_marker -v`
Expected: FAIL (returncode 0 and `.zshrc` truncated).

- [ ] **Step 3: Add the guard before the awk**

```bash
if [[ $(grep -cxF -- "$marker_begin" "$zshrc") != $(grep -cxF -- "$marker_end" "$zshrc") ]]; then
    echo "install-term: $zshrc has an unbalanced ghost-shell marker; fix it by hand first" >&2
    exit 1
fi
```

`grep -c` exits 1 on zero matches, so wrap each in `|| true` or compare with `$(... || true)`.

- [ ] **Step 4: Run the whole file**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py -q && bash -n scripts/install-term.sh`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add scripts/install-term.sh scripts/test_term_theme.py
git commit -m "fix(term): refuse to edit a .zshrc whose ghost-shell markers are unbalanced"
```

### Task 4: `term-backup.sh` refuses a config home outside `$HOME` (finding 18)

**Files:**
- Modify: `scripts/term-backup.sh:8-9`
- Test: `scripts/test_term_theme.py`

- [ ] **Step 1: Write the failing test**

```python
def test_install_refuses_a_config_home_outside_home(tmp_path):
    import os, subprocess
    home = _home(tmp_path)
    outside = tmp_path / "cfg"
    (outside / "ghostty").mkdir(parents=True)
    (outside / "ghostty/config").write_text("old = 1\n")
    env = {**os.environ, "HOME": str(home), "XDG_CONFIG_HOME": str(outside)}
    result = subprocess.run(["bash", str(REPO / "scripts/install-term.sh")], env=env, capture_output=True, text=True, cwd=REPO)
    assert result.returncode != 0
    assert (outside / "ghostty/config").read_text() == "old = 1\n"
```

- [ ] **Step 2: Run it to see it fail**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py -k outside_home -v`
Expected: FAIL (the config is overwritten).

- [ ] **Step 3: Add the guard in `term-backup.sh`**

After `config_home=…`: `[[ $config_home == "$HOME"/* ]] || { echo "term-backup: XDG_CONFIG_HOME must be under HOME for an in-place restore" >&2; exit 1; }`. `install-term.sh` already stops when the backup fails.

- [ ] **Step 4: Run the file**

Run: `uv run --no-project --with pytest python -I -m pytest scripts/test_term_theme.py -q && bash -n scripts/term-backup.sh`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add scripts/term-backup.sh scripts/test_term_theme.py
git commit -m "fix(term): refuse a config home the backup cannot archive relative to HOME"
```

### Task 5: Unread criticals are capped too (finding 2)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/history.rs:88-91,247-262`
- Modify: `README.md:155`, `crates/ghostd/src/config/default.toml:190-191`
- Test: `crates/ghostd/src/feeds/notifications/history.rs` tests

**Interfaces:**
- Produces: `const CRITICAL_HEADROOM: usize = 2;` and `enforce(&mut self, limit: usize)` keeping at most `limit` non-critical-unread records and at most `limit * CRITICAL_HEADROOM` unread criticals, newest kept.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn unread_criticals_are_capped_at_twice_the_limit_newest_kept() {
    let mut history = History::new();
    for n in 0..10u64 {
        history.notify(critical(&n.to_string()), n * 2_000, false, 3).unwrap();
    }
    assert_eq!(ids(&history), [10, 9, 8, 7, 6, 5]);
}

#[test]
fn a_zero_limit_keeps_nothing() {
    let mut history = History::new();
    history.notify(critical("a"), 0, false, 0).unwrap();
    history.notify(incoming("b"), 2_000, false, 0).unwrap();
    assert!(history.items().is_empty());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ghostd --lib notifications::history::tests::unread_criticals -- --nocapture`
Expected: FAIL (10 ids kept).

- [ ] **Step 3: Implement**

In `enforce`, after the existing drop, run the same pass over `is_unread_critical` items with the bound `limit.saturating_mul(CRITICAL_HEADROOM)`. Update the `is_unread_critical` doc comment and the `notify` doc ("`limit` caps the others; unread criticals get twice the room"). Change README line 155 to `# records kept; unread critical ones get twice the room` and the matching `default.toml` comment.

- [ ] **Step 4: Run the module's tests and the integration tests**

Run: `cargo test -p ghostd notifications`
Expected: PASS, including `cap_keeps_the_newest_and_every_unread_critical` (still true at 3 with one critical).

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/notifications/history.rs README.md crates/ghostd/src/config/default.toml
git commit -m "fix(notifications): cap unread criticals at twice the history limit"
```

### Task 6: A call's strings and actions are capped (finding 3)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/server.rs:144-198` (`parse`)
- Test: `crates/ghostd/tests/notifications_server.rs`

**Interfaces:**
- Produces, in `server.rs`: `const MAX_SUMMARY_BYTES: usize = 1_024; const MAX_BODY_BYTES: usize = 8_192; const MAX_NAME_BYTES: usize = 512; const MAX_ACTIONS: usize = 8;` and `fn truncated(text: &str, max_bytes: usize) -> &str` (cuts at a char boundary at or before `max_bytes`). Names are `app_name`, `app_icon`, `desktop-entry`, `image-path`, action keys and labels.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn oversized_fields_are_cut_to_their_caps() {
    let served = serving().await;
    let body = "<b>".repeat(100_000);
    let actions: Vec<String> = (0..40).flat_map(|n| [format!("k{n}"), format!("L{n}")]).collect();
    let (incoming, _) = notify_and_capture(&served, &"é".repeat(2_000), &body, &actions, &Hints::new()).await;
    assert!(incoming.summary.len() <= 1_024 && incoming.summary.chars().all(|c| c == 'é'));
    // 8 KiB of `<b>` is about 2 730 open tags, each closed again by the sanitiser: at most 7 bytes per tag.
    assert!(incoming.body.len() <= 8_192 * 3, "the markup of a cut body stays bounded: {}", incoming.body.len());
    assert_eq!(incoming.actions.len(), 8);
}
```

Adapt the `notify_and_capture` arguments to its real signature (`notifications_server.rs:79`).

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --test notifications_server oversized`
Expected: FAIL (40 actions, 4 000-byte summary).

- [ ] **Step 3: Implement**

In `parse`: `truncated(call.summary, MAX_SUMMARY_BYTES)`, `truncated(call.body, MAX_BODY_BYTES)` before `sanitise`, `truncated(…, MAX_NAME_BYTES)` on the names, `pairs.iter().take(MAX_ACTIONS)` after the `default` pair is skipped. One WHY comment: the server, not the app, bounds what the history keeps.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ghostd --test notifications_server`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/notifications/server.rs crates/ghostd/tests/notifications_server.rs
git commit -m "fix(notifications): cap a call's text, names and action count before they enter the history"
```

### Task 7: A transient replace rewrites in place and is not deduped (finding 7)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/history.rs:195-245` (`notify`, `repeats`)
- Modify: `crates/ghostd/src/feeds/notifications/state.rs:226-258` (`notify`)
- Test: `history.rs` tests; `crates/ghostd/tests/notifications.rs`

**Interfaces:**
- `History::notify`: the `.filter(|_| !transient)` goes; a call with `replaces_id != 0` is never refused as a repeat.
- Produces, on `State`: `fn replace_passing(&mut self, incoming: &Incoming, now_ms: u64, lifetime: Duration) -> Option<u32>`: when `incoming.transient && incoming.replaces_id != 0` names a record in `passing`, rewrites it (same id, `revision + 1`, new expiry) and returns the id.

- [ ] **Step 1: Write the failing unit tests**

```rust
#[test]
fn a_transient_replacement_of_a_kept_record_rewrites_it() {
    let mut history = History::new();
    history.notify(incoming("a"), 0, false, LIMIT).unwrap();
    let transient = Incoming { transient: true, replaces_id: 1, ..incoming("b") };
    let outcome = history.notify(transient, 2_000, false, LIMIT).unwrap();
    assert_eq!((outcome.id, outcome.kept), (1, true));
    assert_eq!(history.get(1).unwrap().summary, "b");
}

#[test]
fn a_replacement_within_a_second_is_not_refused() {
    let mut history = History::new();
    history.notify(incoming("a"), 0, false, LIMIT).unwrap();
    let again = Incoming { replaces_id: 1, ..incoming("a") };
    assert!(history.notify(again.clone(), 100, false, LIMIT).is_ok());
    assert!(history.notify(again, 200, false, LIMIT).is_ok());
}
```

- [ ] **Step 2: Write the failing integration tests**

In `tests/notifications.rs`, with `Options { popup_timeout_s: 1, .. }`:

```rust
#[tokio::test]
async fn a_transient_replace_keeps_the_id_and_one_passing_item() {
    // two notifies with transient=true, the second with replaces_id = first id, 100 ms apart
    // assert: same id returned; the next snapshot has exactly one item with that id and revision 1
}

#[tokio::test]
async fn a_transient_replace_of_an_expired_id_is_new() {
    // transient notify, wait > 1 s for expiry, transient notify with replaces_id = old id
    // assert: a different id; the snapshot has one item, revision 0
}
```

Fill the bodies with the file's `Hints`, `send`-style helpers; set `hints.insert("transient", Variant::from(true))`.

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p ghostd transient`
Expected: the unit test fails with `kept == false`; the first integration test fails on two items or an `Excess` error.

- [ ] **Step 4: Implement**

`history.rs`: drop the `.filter(|_| !transient)`; in the dedupe `if`, add `&& incoming.replaces_id == 0`. `state.rs` `notify`: before `self.history.notify`, `if let Some(id) = self.replace_passing(&incoming, now_ms, lifetime) { reply Ok(id); publish; queue_image; return; }`. `replace_passing` builds the record with `record(id, incoming.clone(), old.arrived_s, old.held)`, `revision = old.revision + 1`.

- [ ] **Step 5: Run the notification tests**

Run: `cargo test -p ghostd notifications`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ghostd/src/feeds/notifications/history.rs crates/ghostd/src/feeds/notifications/state.rs crates/ghostd/tests/notifications.rs
git commit -m "fix(notifications): rewrite a transient replacement in place and never dedupe a replace"
```

### Task 8: Expiry and eviction emit `NotificationClosed` (finding 11)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/state.rs:475-482` (`expire`), `history.rs:247` (`enforce` returns the dropped ids), `state.rs:226-258` (`notify` emits for evicted ids), `mod.rs:281` (`state.expire().await`)
- Test: `crates/ghostd/tests/notifications.rs`

**Interfaces:**
- `History::enforce(&mut self, limit: usize) -> Vec<u32>` (the ids it dropped); `History::notify` returns them in `Outcome { evicted: Vec<u32>, .. }` (`Outcome` loses `Copy`).
- `State::expire(&mut self)` becomes `async fn`, emitting `server::EXPIRED` for every record it removes.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn an_expired_transient_emits_closed_with_reason_expired() {
    let mut feed = Running::start_with(Options { popup_timeout_s: 1, ..Options::default() }).await;
    let mut closed = feed.signals("NotificationClosed").await;
    let mut hints = Hints::new();
    hints.insert("transient", Variant::from(true));
    let id = feed.notify.notify("app", "passing", "", &[], &hints).await.unwrap();
    let message = next(&mut closed).await.unwrap();
    assert_eq!(message.body().deserialize::<(u32, u32)>().unwrap(), (id, EXPIRED));
}
```

Import `EXPIRED` from `ghostd::feeds::notifications::server`.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --test notifications expired`
Expected: FAIL (no signal within `BUS_WAIT`).

- [ ] **Step 3: Implement**

`expire` collects the removed ids, publishes, then `emit_closed(self.signal_bus(), id, EXPIRED).await` for each. `notify` does the same with `outcome.evicted`. Remove the `EXPIRED` constant assertion from `notifications_server.rs:324` only if clippy flags the import; otherwise leave it.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ghostd notifications`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/notifications/state.rs crates/ghostd/src/feeds/notifications/history.rs crates/ghostd/src/feeds/notifications/mod.rs crates/ghostd/tests/notifications.rs
git commit -m "fix(notifications): send NotificationClosed when a record expires or the cap drops it"
```

### Task 9: `image-path` becomes a checked file path or an icon name (finding 8, Review Focus 1)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/server.rs:171-175,177-184`
- Test: `crates/ghostd/tests/notifications_server.rs:162-200` (the `file:///a.png` assertion) and a new test

**Interfaces:**
- Produces: `fn image_path(hint: &str) -> Option<PathBuf>`: strips a leading `file://`, keeps only an absolute path whose `fs::metadata` is a regular file. `parse` sets `image` to that path's string or `""`. A hint with no `/` and no scheme is an icon name: it fills `app_icon` when `app_icon` is empty.

- [ ] **Step 1: Write the failing tests**

```rust
#[tokio::test]
async fn image_path_forms_are_normalised() {
    let dir = tempfile::tempdir().unwrap();
    let picture = dir.path().join("a.png");
    std::fs::write(&picture, b"x").unwrap();
    // hint "file://<picture>" -> incoming.image == picture, app_icon unchanged
    // hint "<picture>" -> the same
    // hint "mail-unread" with app_icon "" -> image "", app_icon "mail-unread"
}

#[tokio::test]
async fn image_path_that_is_not_a_regular_file_is_dropped() {
    // hint = the temp dir itself -> image ""; hint "/dev/zero" -> image ""
}
```

Change the existing assertion at `notifications_server.rs:197` to expect `""` (that file does not exist).

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ghostd --test notifications_server image_path`
Expected: FAIL (`image == "file:///…"`).

- [ ] **Step 3: Implement `image_path` and use it in `parse`**

`fs::metadata` on the handler path is one local stat; say so in a comment and that it is what keeps a FIFO or a device out of the shell's image loader.

- [ ] **Step 4: Run the tests and the shell smoke**

Run: `cargo test -p ghostd notifications && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-audit-t9.sock make smoke` (in a headless KWin per the constraints)
Expected: PASS; no QML error.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/notifications/server.rs crates/ghostd/tests/notifications_server.rs
git commit -m "fix(notifications): keep image-path only as a checked file path and treat a bare name as the icon"
```

### Task 10: A replaced record toasts again (finding 10, Decision 4, Review Focus 2)

**Files:**
- Modify: `crates/ghost-proto/src/snapshot.rs:620-645` (`Notification.updated_s: u64`, `#[serde(default)]`)
- Modify: `crates/ghostd/src/feeds/notifications/history.rs:104-125` (`record`), `:205-240` (rewrite sets `updated_s = now_ms / MS_PER_S`), `:138-170` (`from_json` fills a zero `updated_s` with `arrived_s`)
- Modify: `crates/ghostd/src/fake/notifications.rs:145-155` (`updated_s: at_s`), `state.rs` `replace_passing` from Task 7
- Modify: `shell/kit/GhostKit/lib/notifications.mjs:26-31`, `shell/host/Toasts.qml:28,70-75`
- Test: `history.rs` tests, `shell/tests/notifications.test.mjs`

**Interfaces:**
- Produces, in `notifications.mjs`: `export function toastKey(item) { return item.id + ':' + (item.revision ?? 0); }`; `toastQueue` filters on `item.updated_s >= shellStartS && !shown.has(toastKey(item))`. `shown` becomes a `Set` of keys.
- `Toasts.qml` `finish(id, reason)` adds `Rules.toastKey(items.find(item => item.id === id) ?? { id, revision: 0 })`.

- [ ] **Step 1: Write the failing node test**

```js
test('a replaced record toasts again once its earlier toast ended', () => {
  const first = record(1, { revision: 0, updated_s: SHELL_START_S + 1 });
  const shown = new Set([toastKey(first)]);
  assert.deepEqual(queue([first], { shown }).show, []);
  const replaced = record(1, { revision: 1, updated_s: SHELL_START_S + 5 });
  assert.deepEqual(queue([replaced], { shown }).show, [1]);
});

test('a replacement of a record older than the shell toasts by its update time', () => {
  const old = record(1, { arrived_s: SHELL_START_S - 100, updated_s: SHELL_START_S + 5, revision: 1 });
  assert.deepEqual(queue([old]).show, [1]);
});
```

Give the file's `record()` helper `updated_s: SHELL_START_S + id` and `revision: 0`; change the `shown: new Set([1])` test to `new Set(['1:0'])`.

- [ ] **Step 2: Write the failing Rust tests**

```rust
#[test]
fn a_replacement_moves_updated_but_not_arrived() {
    let mut history = History::new();
    history.notify(incoming("a"), 5_000, false, LIMIT).unwrap();
    history.notify(Incoming { replaces_id: 1, ..incoming("b") }, 50_000, false, LIMIT).unwrap();
    let record = history.get(1).unwrap();
    assert_eq!((record.arrived_s, record.updated_s), (5, 50));
}

#[test]
fn old_history_files_read_updated_as_arrived() {
    // to_json of a history with one record, remove "updated_s" from the JSON, from_json -> updated_s == arrived_s
}
```

- [ ] **Step 3: Run both to see them fail**

Run: `node --test shell/tests/notifications.test.mjs && cargo test -p ghostd --lib notifications::history`
Expected: FAIL (`updated_s` undefined / unknown field).

- [ ] **Step 4: Implement**

Add the field and its doc ("the wall-clock second of the record's last write; `arrived_s` on a new record"). `record()` sets both; the rewrite in `notify` and `State::replace_passing` set `updated_s`; `from_json` maps `updated_s == 0` to `arrived_s`. Update the two `Notification { .. }` literals in `history.rs` tests (lines 558, 592) and the fake. Shell: `toastKey`, the filter, `Toasts.qml` `finish`.

- [ ] **Step 5: Run everything**

Run: `cargo test --workspace && node --test 'shell/tests/*.test.mjs' && GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-audit-t10.sock make smoke`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ghost-proto/src/snapshot.rs crates/ghostd/src/feeds/notifications/history.rs crates/ghostd/src/feeds/notifications/state.rs crates/ghostd/src/fake/notifications.rs shell/kit/GhostKit/lib/notifications.mjs shell/host/Toasts.qml shell/tests/notifications.test.mjs
git commit -m "feat(notifications): stamp a record's last write so a replacement toasts again"
```

### Task 11: A hung collector ends the daemon (finding 4, Decision 5)

**Files:**
- Modify: `crates/ghostd/src/server/sampling.rs:91-101`, `crates/ghostd/src/server/mod.rs:139-147` (`ServeError`)
- Test: `crates/ghostd/tests/server_sampling.rs`

**Interfaces:**
- Produces: `const SAMPLE_TIMEOUT_TICKS: u32 = 30;` in `sampling.rs`; `ServeError::SamplingHung { after: Duration }` with message `"sampling hung for {after:?}: a collector is stuck"`.

- [ ] **Step 1: Write the failing test**

```rust
/// Blocks for long enough to be a hang at the test tick, but returns, because a dropped runtime
/// waits for its blocking threads.
struct Hangs;

impl Source<CpuSnapshot> for Hangs {
    fn sample(&mut self) -> CpuSnapshot {
        std::thread::sleep(Duration::from_millis(u64::from(TICK_MS) * 60));
        CpuSnapshot::default()
    }
}

#[tokio::test]
async fn serving_stops_when_a_collector_hangs() {
    // as serving_stops_with_an_error_when_a_collector_panics, with Hangs as the cpu source
    // assert: matches!(error, ServeError::SamplingHung { .. }) within WAIT
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --test server_sampling hangs`
Expected: FAIL (timeout: the daemon kept serving).

- [ ] **Step 3: Implement**

Wrap `sampled.await` in `tokio::time::timeout(Duration::from_millis(u64::from(tick_ms.get()) * u64::from(SAMPLE_TIMEOUT_TICKS)), sampled)`; `Err(_elapsed)` returns `ServeError::SamplingHung { after }`. Doc on the variant: why the process must exit (clients would look served while every built-in topic is frozen).

- [ ] **Step 4: Run the server tests**

Run: `cargo test -p ghostd --test server_sampling --test server_handshake`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/server/sampling.rs crates/ghostd/src/server/mod.rs crates/ghostd/tests/server_sampling.rs
git commit -m "fix(server): exit when a collector hangs past thirty ticks so systemd restarts the daemon"
```

## Should fix

### Task 12: A config error clears when the file returns to its old content (finding 12)

**Files:**
- Modify: `crates/ghostd/src/config/watch.rs:105-158`
- Test: `crates/ghostd/tests/config_watch.rs`

**Interfaces:**
- `settle_pass(pass, &mut Recovery, config_tx, error_tx)` where `struct Recovery { removal_reported: bool, last_failed: bool }`; on `Ok` after `last_failed`, `config_tx.send_replace(config)`.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn recovering_the_same_content_republishes() {
    let mut watching = watch_config_with_panel_fill(INITIAL_PANEL_FILL);
    fs::write(watching.path(), "[appearance\npanel_fi").unwrap();
    watching.next_error().await;
    fs::write(watching.path(), panel_fill_toml(INITIAL_PANEL_FILL)).unwrap();
    assert_eq!(watching.next_config().await.appearance.panel_fill, INITIAL_PANEL_FILL);
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --test config_watch recovering`
Expected: FAIL ("no config published in time").

- [ ] **Step 3: Implement**

Replace the `removal_reported: bool` with `Recovery`; set `last_failed = true` on `Err` and on a removal; on `Ok`, `if recovery.last_failed { config_tx.send_replace(config); } else { publish_if_changed(..) }`, then clear it. `Reloader::apply` is idempotent, so the republish only clears the error.

- [ ] **Step 4: Run the config tests**

Run: `cargo test -p ghostd config`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/config/watch.rs crates/ghostd/tests/config_watch.rs
git commit -m "fix(config): republish after a failed save is undone so the error clears"
```

### Task 13: Transient pictures are pruned and images are bounded earlier (finding 13)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/images.rs:17,83-122` (`MAX_PIXELS = 1_024 * 1_024`; `pub fn check(&self) -> Result<(), ImageError>` pulled out of `packed`, which calls it)
- Modify: `crates/ghostd/src/feeds/notifications/server.rs:168-170` (`parse` drops an image whose `check` fails, with a `debug!`)
- Modify: `crates/ghostd/src/feeds/notifications/state.rs:266-275,475-482` (`image_written` marks dirty when the record is only passing; `expire` marks dirty when a removed record had an image)
- Test: `crates/ghostd/tests/notifications.rs`; `images.rs` tests (the size-limit case moves to the new cap)

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn a_transient_picture_is_removed_after_it_passes() {
    // Options { popup_timeout_s: 1 }; transient notify with the 1x1 image-data of
    // image_data_becomes_a_png_after_the_reply; eventually(|| png.exists()); then eventually(|| !png.exists())
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --test notifications transient_picture`
Expected: FAIL (the PNG stays).

- [ ] **Step 3: Implement the three changes**

Update the `MAX_PIXELS` comment: a toast shows 40 px, so a megapixel is already generous. Update the existing images test that asserts the old limit.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ghostd notifications`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ghostd/src/feeds/notifications/images.rs crates/ghostd/src/feeds/notifications/server.rs crates/ghostd/src/feeds/notifications/state.rs crates/ghostd/tests/notifications.rs
git commit -m "fix(notifications): prune a transient's picture after it passes and check images before queueing"
```

### Task 14: `recent` reads off the runtime thread (finding 14)

**Files:**
- Modify: `crates/ghostd/src/feeds/recent.rs:104-112`

- [ ] **Step 1: Implement**

Both `read(xbel, home)` calls in the watch loop become `spawn_blocking(move || read(&xbel, &home)).await.unwrap_or_default()` with cloned `PathBuf`s; keep the fallback publish at line 101 as is (it runs once). Comment: `exists()` on a hung mount would otherwise stop the whole single-thread runtime.

- [ ] **Step 2: Run the feed's tests**

Run: `cargo test -p ghostd --test recent`
Expected: PASS (behaviour unchanged; no new test, the change is where the call runs).

- [ ] **Step 3: Commit**

```bash
git add crates/ghostd/src/feeds/recent.rs
git commit -m "fix(recent): stat the recent files on a blocking thread"
```

### Task 15: A refused pin reaches the user (finding 15)

**Files:**
- Modify: `crates/ghostd/src/feeds/pins.rs:19-33,47-66`, `crates/ghostd/src/main.rs:329`
- Test: `crates/ghostd/tests/pins.rs:300-330`

**Interfaces:**
- `pins::feed(config_path: PathBuf, plasma_dir: PathBuf, on_warning: WarningHook) -> Feed`; `handle(config_path, call, on_warning: &WarningHook)` calls `on_warning("ghost-shell pin", &error.to_string())` on `Err`. `main.rs` passes `desktop_warning()` (the same hook the hub gets).

- [ ] **Step 1: Write the failing test**

In the existing feed-run test at `tests/pins.rs:300`, replace the hook with one that sends into an `mpsc::unbounded_channel`; after the `../bad` pin, assert the channel yields `("ghost-shell pin", body)` with `body.contains("bar.pinned")` or the `EditError` text the real error carries (read `config/edit.rs:89-117` for the message).

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --test pins`
Expected: compile error (wrong arity), then FAIL.

- [ ] **Step 3: Implement and run**

Run: `cargo test -p ghostd --test pins && cargo build -p ghostd`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/ghostd/src/feeds/pins.rs crates/ghostd/src/main.rs crates/ghostd/tests/pins.rs
git commit -m "fix(pins): warn the user when a pin change is refused"
```

### Task 16: Unknown tags stay as text (finding 16)

**Files:**
- Modify: `crates/ghostd/src/feeds/notifications/markup.rs:22-25,57-75`
- Test: `markup.rs` tests

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn unknown_tags_stay_as_escaped_text() {
    let (markup, text) = sanitise("From Bob <bob@x.com> <script>x</script>");
    assert_eq!(text, "From Bob <bob@x.com> <script>x</script>");
    assert_eq!(markup, "From Bob &lt;bob@x.com&gt; &lt;script&gt;x&lt;/script&gt;");
}
```

Update the existing test that expects `<script>` to vanish.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --lib notifications::markup`
Expected: FAIL (`"From Bob  x"`).

- [ ] **Step 3: Implement**

`emit_tag` returns `bool` (whether it consumed the tag); on `false`, `scan` pushes `<{inner}>` to `text` and its escaped form to `markup`. A closing tag that matches no open allowed tag is treated the same way.

- [ ] **Step 4: Run and commit**

Run: `cargo test -p ghostd notifications`

```bash
git add crates/ghostd/src/feeds/notifications/markup.rs
git commit -m "fix(notifications): keep an unknown tag as literal text instead of dropping it"
```

### Task 17: `gate.ini` is written only when it changes, off the loop (finding 17)

**Files:**
- Modify: `crates/ghostd/src/gate.rs:173-201`
- Test: `gate.rs` tests

**Interfaces:**
- Produces: `fn write_if_changed(paths: &[PathBuf], ini: &str, last: &mut Option<String>) -> bool` (false when `ini` equals `last`); `keep` calls it inside `spawn_blocking`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn an_unchanged_snapshot_is_not_written_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gate.ini");
    let mut last = None;
    assert!(write_if_changed(&[path.clone()], "a", &mut last));
    fs::remove_file(&path).unwrap();
    assert!(!write_if_changed(&[path.clone()], "a", &mut last));
    assert!(!path.exists());
    assert!(write_if_changed(&[path.clone()], "b", &mut last));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ghostd --lib gate`
Expected: compile error (no such fn).

- [ ] **Step 3: Implement and move the writes to `spawn_blocking`**

In `keep`, clone `paths` and `ini` into the closure; `last` lives in the loop. Comment: the two fsyncs ran on the runtime's only thread.

- [ ] **Step 4: Run and commit**

Run: `cargo test -p ghostd gate`

```bash
git add crates/ghostd/src/gate.rs
git commit -m "perf(gate): skip an unchanged snapshot and write the rest on a blocking thread"
```

### Task 18: Claude session files parse as written (finding 9)

**Files:**
- Modify: `crates/ghostd/src/feeds/agents/claude.rs:22-43,100-116`
- Modify: `crates/ghostd/tests/fixtures/agents/claude-session-daemon.json:3`, `claude-session-waiting.json:3`, `crates/ghostd/tests/agents.rs:293-303`
- Test: `crates/ghostd/tests/agents.rs:15-45`

- [x] **Step 1: Make the fixtures real and add the assertion**

Quote `procStart` in both fixtures (`"procStart": "123456"`) and in `claude_session`'s format string (`"procStart":"{proc_start}"`). In the first `parse_session` test add `assert_eq!(parsed.proc_start, 123_456);`.

- [x] **Step 2: Run to see it fail**

Run: `cargo test -p ghostd --test agents`
Expected: FAIL (`parse_session` returns `None`).

- [x] **Step 3: Implement**

`proc_start: String` with `#[serde(default)]`; `proc_start: file.proc_start.parse().unwrap_or(0)` in `parse_session`. Comment: Claude writes the number as a JSON string.

- [x] **Step 4: Run and commit**

Run: `cargo test -p ghostd --test agents`

```bash
git add crates/ghostd/src/feeds/agents/claude.rs crates/ghostd/tests/fixtures/agents/claude-session-daemon.json crates/ghostd/tests/fixtures/agents/claude-session-waiting.json crates/ghostd/tests/agents.rs
git commit -m "fix(agents): read procStart as the string Claude writes"
```

## Nice to have

### Task 19: Spectrum publishes a frame only when the bands change (finding 19)

**Files:**
- Modify: `crates/ghostd/src/feeds/spectrum/mod.rs:108-135`
- Test: `mod.rs` tests (`a_stall_goes_dark_once_until_frames_resume`)

- [ ] **Step 1: Change the test**

The `[LIT; LONG_SILENCE as usize]` assertion becomes `[LIT]`, with a line: "ten identical frames publish once". Add after it: send `Frame { bands: [100; BANDS], hold: MIN_HOLD }`, assert one more publish.

- [ ] **Step 2: Run to see it fail, implement, run**

`let mut last = DARK;` before the loop; publish in the frame arm only when `frame.bands != last`, always reset `stall`; the stall arm publishes `DARK` only when `last != DARK`; set `last` after each publish; the final `DARK` publish stays unconditional (the hub replays the last value).

Run: `cargo test -p ghostd --lib spectrum`

- [ ] **Step 3: Commit**

```bash
git add crates/ghostd/src/feeds/spectrum/mod.rs
git commit -m "perf(spectrum): publish a frame only when its bands differ from the last"
```

### Task 20: Dead code in the daemon and the shell (finding 20, Decision 2)

**Files:**
- Modify: `crates/ghost-proto/src/config.rs:25,418-452` (delete `Launcher`, `LauncherStyle`, the `launcher` field), `crates/ghostd/src/config/default.toml:185-187`, `crates/ghostd/tests/config_bar.rs:164-168`
- Modify: `crates/ghostd/src/feeds/mod.rs:35-37,39,106-118` (`next_topic` and its now-unused imports), `crates/ghostd/tests/topic_updates.rs:82-103` and its imports
- Modify: `crates/ghostd/src/server/feed.rs:86-90` (`ConfigFeed::current`)
- Modify: `crates/ghostd/src/widgets/manifest.rs:56-62,76-77`, `crates/ghostd/src/widgets/mod.rs:5` (`SizeHints`)
- Modify: `crates/ghostd/src/feeds/notifications/server.rs:439-441` (`release`)
- Modify: `crates/ghostd/src/feeds/notifications/history.rs:45-50,221,232` and its tests (`Outcome.replaced`)
- Modify: `crates/ghostd/src/collect/labels.rs:68-78`, `crates/ghostd/src/collect/hwmon.rs:315,350,356` (inline the three wrappers)
- Modify: `shell/kit/GhostKit/lib/ticker.mjs:80-82`, `shell/tests/ticker.test.mjs:3,19,58`; `shell/kit/GhostKit/lib/notifications.mjs:7,79-84` (`quietSince` uses `clockParts` from `format.mjs`)

- [ ] **Step 1: Confirm each is unused**

Run: `rg -n 'launcher\.style|LauncherStyle|next_topic|\.current\(\)|SizeHints|release\(|\.replaced|tickerText|fallback_label|drive_label|dimm_label' crates shell widgets scripts -g '!target'`
Expected: hits only at the lines listed above.

- [ ] **Step 2: Delete, inline, and fix the tests that named them**

`quietSince`: build the time from `clockParts(since)` if `format.mjs` exposes hours and minutes there; otherwise keep its two lines and drop the item. Delete `SizeHints` only with `deny_unknown_fields` in mind: a third-party `[size]` table is then refused, which is the contract's intent (no reader exists).

- [ ] **Step 3: Run everything**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && node --test 'shell/tests/*.test.mjs'`
Expected: PASS, including `default_file_equals_builtin_defaults`.

- [ ] **Step 4: Commit**

```bash
git add crates/ghost-proto/src/config.rs crates/ghostd/src/config/default.toml crates/ghostd/tests/config_bar.rs crates/ghostd/src/feeds/mod.rs crates/ghostd/tests/topic_updates.rs crates/ghostd/src/server/feed.rs crates/ghostd/src/widgets/manifest.rs crates/ghostd/src/widgets/mod.rs crates/ghostd/src/feeds/notifications/server.rs crates/ghostd/src/feeds/notifications/history.rs crates/ghostd/src/collect/labels.rs crates/ghostd/src/collect/hwmon.rs shell/kit/GhostKit/lib/ticker.mjs shell/tests/ticker.test.mjs shell/kit/GhostKit/lib/notifications.mjs
git commit -m "refactor: delete the unread launcher.style option and other code nothing calls"
```

### Task 21: `sky.rs` dates through jiff (finding 20)

**Files:**
- Modify: `crates/ghostd/src/feeds/sky.rs:42-47,207-239,338,419-434`

- [ ] **Step 1: Replace**

`local_date(at_s)` → `jiff::Timestamp::from_second(at_s).ok()?.to_zoned(jiff::tz::TimeZone::system())` then `(date.year(), date.month() as u8, date.day() as u8)`. The `utc()` test helper → `jiff::civil::date(y, m, d).at(h, min, 0, 0).to_zoned(TimeZone::UTC).unwrap().timestamp().as_second()`. Delete `days_from_civil`, the three constants, the `MaybeUninit`/`libc` imports and the `civil_dates_count_days_from_the_unix_epoch` test. Keep every sun and moon test unchanged: they pin the behaviour.

- [ ] **Step 2: Run and commit**

Run: `cargo test -p ghostd sky && cargo clippy -p ghostd --all-targets -- -D warnings`

```bash
git add crates/ghostd/src/feeds/sky.rs
git commit -m "refactor(sky): take the calendar from jiff instead of hand-rolled days and localtime_r"
```

### Task 22: `install.sh` prerequisite helpers are written once (finding 20)

**Files:**
- Modify: `scripts/install.sh:93-118` (`check_prerequisites` calls `require_tools "${required_tools[@]}"` and `require_shaders`)

- [ ] **Step 1: Replace the two loops with the two calls, run `bash -n scripts/install.sh`, and run `make smoke-lock` (it runs `install.sh --lock` in the sandbox)**

- [ ] **Step 2: Commit**

```bash
git add scripts/install.sh
git commit -m "refactor(install): reuse require_tools and require_shaders in the prerequisite check"
```

## Smaller items

### Task 23: Ids skip `u32::MAX`, zero and ids in use (finding 21)

**Files:** `crates/ghostd/src/feeds/notifications/history.rs:241-245`; tests there.

- [ ] Test `ids_wrap_past_max_and_skip_live_ones`: a `History` with `next_id = u32::MAX - 1` (add a `#[cfg(test)] fn with_next_id`) and a record with id 1; three notifies yield ids `u32::MAX - 1`, `2`, `3`. Run, see FAIL, implement `take_id` as a loop over `checked_add(1).unwrap_or(FIRST_ID)` skipping `u32::MAX` and any id in `items` or `passing` (pass the passing ids in, or check `items` only and let Task 7's `replace_passing` own passing: check `items` only, document it). Run `cargo test -p ghostd --lib notifications::history`. Commit `fix(notifications): never hand out u32::MAX or a live id`.

### Task 24: Reserved IPv4 ranges are not public (finding 22)

**Files:** `crates/ghostd/src/feeds/uplink/sockets.rs:131-140`; `crates/ghostd/tests/uplink.rs:120-133`.

- [ ] Add `"198.18.0.5"`, `"192.0.0.1"`, `"240.0.0.1"` to the not-public list in the test; see FAIL; add `|| ip.octets()[0] >= 240 || in_v4_prefix(ip, BENCHMARK)` with `const BENCHMARK = (Ipv4Addr::new(198, 18, 0, 0), 15)` and `IETF_PROTOCOL = (Ipv4Addr::new(192, 0, 0, 0), 24)`, matching the file's `CGNAT` form. Run `cargo test -p ghostd --test uplink`. Commit `fix(uplink): treat the benchmark, protocol-assignment and class E ranges as not public`.

### Task 25: HTTPS only, and a bounded geoip download (finding 23)

**Files:** `crates/ghostd/src/feeds/http.rs:29-33`, `crates/ghost-alerts-inmet/src/main.rs:141-145`, `scripts/fetch-geoip.sh:27-29`.

- [ ] Add `.https_only(true)` to both `Agent::config_builder()` chains (one WHY line: a redirect to cleartext would be followed otherwise). In `download`, add `--fail --proto '=https' --proto-redir '=https' --max-time 300` to `curl`; `--fail` makes a 404 exit non-zero, so capture the status with `|| true` and keep the existing status check. Run `cargo build --workspace && bash -n scripts/fetch-geoip.sh`. Commit `fix(net): refuse cleartext redirects and bound the geoip download`.

### Task 26: CPU topology follows the online set (finding 24)

**Files:** `crates/ghostd/src/collect/cpu.rs:63-93,139-165`; `crates/ghostd/tests/cpu.rs`.

- [ ] Test `an_offline_thread_leaves_its_core_and_ccd_means`: start from `fake_9950x3d()`, sample once, then rewrite `proc/stat` without `cpu16`..`cpu31` and remove their `topology`/`cache` dirs; sample again with cpu0 at 100 % and assert core 0's load is 100 (not 50) and CCD 0's mean counts 8 threads. See FAIL. Implement: in `sample_after`, when `current.cpus.keys()` differs from `self.cpus`, re-run `enumerate_cpus`, `read_topology`, `read_cores` and `thread_count` (log at `info!` once per change). Run `cargo test -p ghostd --test cpu`. Commit `fix(cpu): reread the topology when the set of online CPUs changes`.

### Task 27: `agents::watch` stops when its watcher dies (finding 25)

**Files:** `crates/ghostd/src/feeds/agents/mod.rs:191-202`.

- [x] Mirror `watch_usage` (`mod.rs:461-464`): in the first `select!` arm, `event = events.recv() => { if event.is_none() { return pending().await; } while let … }`. No test: the trigger is notify's thread panicking. Run `cargo test -p ghostd --test agents`. Commit `fix(agents): wait for ever instead of spinning when the session watcher closes`.

**Done differently (69d4fc7):** the agents merge had rewritten this loop, and `return pending()` would also stop the 30 s sweep and freeze the list, so the arm became `Some(_) = events.recv()`, which disables only itself.

### Task 28: A manifest default must match its type (finding 26)

**Files:** `crates/ghostd/src/widgets/manifest.rs:245-260`, `crates/ghostd/src/widgets/resolve.rs:215-227` (`accepts` becomes `pub(super)` in `manifest.rs`), `crates/ghostd/tests/widgets.rs:395-500`.

- [ ] Add a row to `SETTING_CASES` style tests: a manifest with `type = "int"`, `default = "3"` is refused by `discover` with `ManifestError::InvalidDefault { key, .. }` whose message names the key; an `enum` default outside `values` likewise. See FAIL; implement in `validate` with `accepts(setting, &setting.default)`. Run `cargo test -p ghostd --test widgets`. Commit `fix(widgets): refuse a manifest whose default does not match its declared type`.

### Task 29: Topic updates publish only when the flag changed (finding 27)

**Files:** `crates/ghostd/src/feeds/notifications/state.rs:160-209`; `crates/ghostd/tests/notifications.rs`.

- [ ] Test `a_windows_update_that_changes_nothing_publishes_nothing`: after start, `feed.update(topics::WINDOWS, dnd_windows(false))` twice, then `feed.nothing_published().await`. See FAIL. Implement: `topic` computes the new `fullscreen`/`locked` and calls `publish()` only when one changed (sources already publish through `mark_dirty` paths; keep their `publish`). Run `cargo test -p ghostd --test notifications`. Commit `perf(notifications): publish on a windows or session update only when it changes a flag`.

### Task 30: `Toasts.qml` forgets ids the history no longer has (finding 28)

**Files:** `shell/host/Toasts.qml:28,41-43,70-75`.

- [ ] In `sync()`, after `visibleIds = ids`, rebuild `shownIds`, `announced` and `endedIds` keeping only keys whose id is in `items` (`shownIds` keys are `id:revision` after Task 10; split on `:`). Comment: the daemon caps the history, so the sets are bounded by it. Verify with `GHOST_SMOKE_SOCKET=/tmp/ghost-smoke-audit-t30.sock make smoke` in a headless KWin. Commit `perf(shell): drop toast bookkeeping for records the history no longer holds`.

### Task 31: `make smoke` refuses the live compositor (finding 29, Decision 3)

**Files:** `scripts/qml-smoke.sh:66-70`, `scripts/measure-covered.sh:72-78`, `README.md:15-40`.

- [ ] After the bus guard in each script: `if [[ ${WAYLAND_DISPLAY:-} != ghost-sbx-* && ${GHOST_SMOKE_LIVE:-} != 1 ]]; then echo "smoke: FAIL (WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-} is not a headless KWin; set GHOST_SMOKE_LIVE=1 to draw on this display)" >&2; exit 1; fi` (`covered:` prefix in the other). README "Test": one bullet for `GHOST_SMOKE_LIVE=1` and a pointer to `scripts/headless-kwin.sh`. Run `bash -n` on both and one headless `make smoke`. Commit `fix(scripts): refuse to run the smoke shell on a display that is not a headless KWin`.

### Task 32: Kode Mono is pinned and checksummed (finding 30)

**Files:** `scripts/install.sh:74-79,152-169`.

- [ ] Replace `raw/main` with `raw/<commit sha>` of the current `ofl/kodemono` tree (`gh api 'repos/google/fonts/commits?path=ofl/kodemono&per_page=1' --jq '.[0].sha'`), add the two files' `sha256` to `font_files` as a third field, and verify with `sha256sum -c` (`printf '%s  %s\n' "$sum" "$font_dir/$name.part" | sha256sum -c --quiet`) before the `mv`. Run `bash -n scripts/install.sh`, then exercise only the download: in a subshell, source the variable and function block of `install.sh` with `font_dir=$(mktemp -d)` and run `install_kode_mono`; the two files must appear and a deliberately wrong checksum must make it return 1 and leave no `.part`. Commit `fix(install): pin the Kode Mono download to a commit and verify its checksum`.

### Task 33: NetworkManager address and speed changes are watched (finding 31)

**Files:** `crates/ghostd/src/feeds/links/nm.rs:143-204`; `crates/ghostd/tests/common/nm.rs`; `crates/ghostd/tests/links.rs`.

- [ ] Test `a_device_address_change_is_read_again`: after the first snapshot, make the stub change the device's `Ip4Config` address and emit `PropertiesChanged` on `org.freedesktop.NetworkManager.Device` for `Ip4Config` (add a `set_device_address` helper to the stub like `set_vpn_state`), then `snapshot_where(|s| s.lan.address == "…new…")`. See FAIL (timeout). Implement two more `MatchRule`s (`Device` and `Connection.Active` `PropertiesChanged`) and a `touches_addresses` filter for `Ip4Config` and `Speed`, in `Changes::listen`/`next`/`drain`. Run `cargo test -p ghostd --test links`. Commit `fix(links): reread NetworkManager when a device's address or speed changes`.

### Task 34: `RUST_LOG` selects the log level (finding 32)

**Files:** `Cargo.toml:29` (`tracing-subscriber` features add `"env-filter"`), `crates/ghostd/src/main.rs:133-136`, `crates/ghost-alerts-inmet/src/main.rs` (same init, if present), README "Run".

- [ ] `.with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))`. README "Run": one line, `RUST_LOG=ghostd=debug` in a drop-in or `systemctl --user edit ghostd` for a debugging session. Run `cargo build --workspace && cargo clippy --workspace -- -D warnings`. Commit `feat(ghostd): read the log level from RUST_LOG, info by default`.

---

## Self-review notes

- Spec coverage: findings 1–32 map to Tasks 1–34 (finding 20 spans Tasks 20–22; finding 8 is Task 9; finding 9 is Task 18).
- Types: `Outcome { id, kept, evicted }` after Tasks 8 and 20 (`replaced` deleted, `evicted` added; do Task 8 before Task 20, or delete `replaced` in Task 8). `toastKey` and `updated_s` are defined in Task 10 and used in Task 30. `WarningHook` is the existing `server::WarningHook`.
- Review Focus items are pinned in Tasks 9, 10, 1, 7 and 6.
- Open: the terminal `--lock` smoke needs `GHOST_LOCK_ASSEMBLE`, a test-only env override; if the owner prefers no override, Task 2's smoke step can instead rename a shader in the sandbox copy of the repo, at the cost of a copy of `shell/`.

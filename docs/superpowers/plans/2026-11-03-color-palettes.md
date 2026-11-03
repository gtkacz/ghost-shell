# Color palettes implementation plan

> Execute inline with superpowers:executing-plans; use tests before implementation.

**Goal:** Named Section Nine and Netrunner desktop/lock palettes, with Netrunner enabled locally.
**Architecture:** Typed preset selection in ghost-proto; resolve defaults and explicit overrides in ghostd's config loader. QML and gate snapshots consume the resolved colors already.
**Tech Stack:** Rust, serde, TOML, QML.
**Spec:** ../specs/2026-11-03-color-palettes-design.md

## Constraints and review focus

Section Nine remains byte-for-byte equivalent in colors. Terminal is excluded.
Explicit overrides win, including values equal to the old defaults. Unknown
names and malformed colors must fail. Serialized resolved configs must roundtrip.
Removing a preset or changing back must restore defaults without stale colors.

## Task 1: Resolve and document palettes

- [x] Add config-load tests for defaults, Netrunner anchors, explicit override precedence, invalid names/colors, reset, and resolved roundtrip; run to observe failure.
- [x] Add `Palette` enum and `Palette::colors() -> Colors` in `crates/ghost-proto/src/config.rs`; add `Appearance.palette` with Section Nine default.
- [x] In `crates/ghostd/src/config/load.rs`, resolve selected palette then retain only explicitly supplied color overrides before existing validation.
- [x] Document selection and override precedence in `default.toml` and README; mirror preset metadata in the QML fallback.
- [x] Run config tests, workspace tests, formatting, clippy, and shell tests.

## Task 2: Activate locally

- [x] Build release ghostd; back up local config, select Netrunner and remove old color overrides.
- [x] Install only the daemon binary and restart its user service; confirm running config and gate snapshot colors. Preserve terminal and other settings.

## Results

Initial test failed on unknown `appearance.palette`, then all 88 config tests
passed. Workspace: 861 tests passed; shell: 36 files passed; clippy, formatting,
and diff whitespace checks passed. Final independent review found no issues.
The running daemon restarted successfully and gate.ini contains Netrunner's
resolved tokens. Local backups have suffix `.pre-netrunner.20261103-013058`.

No commit or merge requested.

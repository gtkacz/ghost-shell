# Netrunner terminal implementation plan

**Goal:** Extend the existing terminal generator and installer with an explicit
Netrunner preset matching the approved desktop, keeping Section Nine the default.

**Architecture:** Share Netrunner's desktop tokens as embedded JSON in ghost-proto
and read that same file from the Python generator. Keep ANSI and terminal-only
settings in `term/netrunner.toml`. Add `--palette` to generator and installer.

**Validation:** Existing palette/renderer/backup tests plus preset generation,
simulation checks, invalid preset rejection, and installer dry-run selection.
Validate generated Ghostty config with the installed Ghostty executable.

- [x] Add regression tests and observe failure.
- [x] Extract shared tokens; implement preset loading and installer selection.
- [x] Add Netrunner ANSI colors and document generation/installation.
- [x] Run Python tests, Rust config tests, Ghostty validation, formatting and shell syntax checks.
- [x] Review and commit the terminal work separately.

Validation: 66 Python tests pass with TERM=xterm-256color, 88 Rust config tests
pass, Ghostty accepts the generated config, formatting and shell syntax pass.
Independent review found no issues. Installed through the verified-backup
installer; first backup: ~/.config/term-backup-20261103-015018.tar.gz.

The existing font, opacity, prompt structure and backup behavior remain in use.
Netrunner uses a violet cursor, plum ground and lilac text; yellow/red retain
their standard ANSI roles. Blue and magenta are separated in lightness under
the existing protan/deutan checks rather than copied blindly from the desktop.

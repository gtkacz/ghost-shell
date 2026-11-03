# Plan 3c: acceptance on the live desktop

2026-11-03 at `68d6b9a`, Plasma 6.8 on Wayland, NVIDIA, one KWin process throughout. The readings are in the README's "Plan 3c: the switcher".

The user reported in batches and not item by item, so "user" below means covered by a reply such as "looks perfect" or "all fine". "Log" means the user journal, where the switcher writes `switcher: open` at each opening and a flick writes nothing.

## The way back

Rehearsed before any hold, as spec §5.6 asks. The user ran `scripts/install.sh --remove-switcher` from a real text console on tty3:

```
==> restored [TabBox] in /home/gtkacz/.config/kwinrc as it was before --switcher
==> removing /home/gtkacz/.local/share/kwin/tabbox/org.ghostshell.switcher
==> done; the switcher is gone
```

There was no warning, so `reconfigure` reached KWin from the console. `kwinrc` was byte-identical to a copy taken before `--switcher`, with no `[$d]` marker, and the package and its record were gone. A second removal from the session at 15:52 gave the same result.

## Acceptance

| Item | Result | Evidence |
|---|---|---|
| First hold on NVIDIA | Pass, no hang | User: "looks perfect". Log: three openings, no warnings |
| The opening's look, option A | Pass | User: "looks perfect" |
| Flick | Pass | User, in a general reply; a flick logs nothing, so the log cannot confirm it |
| Speed against stock | Pass by feel | User, in the same reply; not timed on this GPU |
| App names, including `CODE` for VS Code | Accepted | I said I would take that reply as a yes for these and the user did not correct it; no explicit comment on `CODE` |
| Browse, Shift back | Pass | User: "all fine". Log: eight openings from 15:45:23 to 15:46:47 |
| Esc | Pass | Same reply |
| Click a row | Pass | Same reply |
| `` Alt+` `` | Pass | Same reply |
| More than 10 windows | Pass | Same reply, with the 11 extra `konsole` windows open |
| A minimised window | Pass | Same reply |
| A window closing while open | Pass, no hang | Same reply, on the real GPU |
| A palette change at the next Alt+Tab | Pass | User: "the palette changed". `gate.ini`'s `signal` went from `#38d9f5` to `#a3d4cc`. Netrunner restored; config identical to its backup |
| `journalctl --user` clean of `switcher` warnings | Pass | Log: the only lines KWin wrote besides `switcher: open` are `This plugin does not support setting window masks`, three per Breeze opening and none at the switcher's (none in 8 sandboxed openings of the switcher, 12 per 4 of Breeze) |
| `--remove-switcher`, a stock Alt+Tab, `--switcher` again | Pass | Removal checked as above, and done twice more after that. The log shows Breeze opening between them and the switcher opening again at 15:59:39 and 16:13:57, in the same KWin |
| Cost against stock | No worse | README: about 1 % of a core on both, held or closed, measured minutes apart; about 134 KiB per opening against stock's 125; the one memory jump reproduced on stock with no opening involved |

The first stock "held" reading turned out to have no opening in it, and one switcher reading was mis-cued. The user's reply had confirmed the sounds and not the hold, and I logged it as a hold anyway. Both were retaken, and the README keeps them marked as not counting. Each counted hold has its opening in the log, but the log cannot show a release, so their length is the user's word.

## The bar, unplanned

The console round trip also exercised the bar fix (d8ce5db). The log shows the bar built on a screen with no name at 15:27:36, while the user was on the console, and on DP-1 at 15:29:20 after the return, and the user confirmed it was there. That matches the sequence that hid the bar earlier that day, though the unnamed screen being KWin's placeholder is an inference. The switch-user route itself is still untested.

## Not checked

- **Opening time on NVIDIA:** timed only headless, so this GPU has only the user's impression.
- **A fresh login with the switcher:** every opening ran in a KWin that was already up, and that had already loaded Breeze.
- **An update to an installed switcher:** proof 7 found it takes a new KWin, and that has not been seen on this desktop.
- **The way back from a hung KWin:** reboot with `Alt+SysRq` then `S`, `U`, `B`, then removal after logging in. KWin never hung, so only the console route ran.
- **No windows at all:** the panel with zero windows, inside KWin.
- **App names beyond those on this desktop:** `GOOGLE-CHROME` and `STEAM_APP_570` were only reasoned about, and no explicit look at them was reported.
- **Whether the switcher adds to a burst of windows:** the first burst left 132,624 KiB with the switcher installed and the second left 53,348 KiB on stock, but the second may reuse memory the first left behind, so the two are not a like-for-like comparison.

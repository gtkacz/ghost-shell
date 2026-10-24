# Lock greeters spike: how both greeters load a theme, read the gate snapshot and switch layouts

Date: 2026-10-23. Lock plan A Task 1, answering design §11 spikes 1, 2 and 5 and the plan's (a)–(e). Tasks 5 and 6
cite it.

## Answers

| Question | Answer |
|---|---|
| **(a)** How is kscreenlocker 6.7.5 pointed at our lock QML without touching `/usr/share/plasma`? | **A user shell package plus `PLASMA_DEFAULT_SHELL` in KWin's environment only.** Install the package at `~/.local/share/plasma/shells/org.ghostshell.lock/` (`metadata.json` with `"X-Plasma-APIVersion": "2"`, QML at `contents/lockscreen/LockScreen.qml`), and add a systemd user drop-in `~/.config/systemd/user/plasma-kwin_wayland.service.d/ghost-shell-lock.conf` holding `[Service]` `Environment=PLASMA_DEFAULT_SHELL=org.ghostshell.lock`. It takes effect at the next login. Revert: delete the drop-in, `systemctl --user daemon-reload`, log in again. **Not** the `plasmashellrc` `[Shell] ShellPackage` key: plasmashell reads the same key and would switch shells. "Outdated" means the package's `X-Plasma-APIVersion` is missing or below 2 |
| **(b)** The lock greeter's QML API | Context properties `authenticator`, `kscreenlocker_userName`, `kscreenlocker_userImage`, `config`, `org_kde_plasma_screenlocker_greeter_interfaceVersion` (2), `org_kde_plasma_screenlocker_greeter_view`; root properties `locked` and `viewVisible`, written by the greeter. Flow: `startAuthenticating()`, then `promptForSecretChanged`, `respond(password)`, then `succeeded` or `failed(kind, authenticator)` (kind 0 is the password); after success `Qt.quit()` ends it. Switch user and suspend come from `org.kde.plasma.private.sessions` `SessionManagement`; Caps Lock from `org.kde.plasma.private.keyboardindicator` `KeyState`; layouts from `org.kde.plasma.workspace.keyboardlayout` `KeyboardLayout`. One window per screen. All of it seen working headlessly, layout switch and Caps Lock included |
| **(c)** Does `QtCore` `Settings { location }` read the INI? | **Lock greeter: yes**, from `$XDG_STATE_HOME/ghost-shell/gate.ini` (read `terminal`); a missing file gives the default. **`sddm-greeter-qt6`: yes** for the mechanism (read `idcheck` from a stand-in file; the absent `/var/lib/ghost-shell/gate.ini` gave the default). Neither reader wrote to the file. The real `/var/lib/ghost-shell` read, as user `sddm` under SELinux, is **unverified** (see "Open risks"). No replacement reader needed |
| **(d)** SDDM metadata and `keyboard` on 0.21 under Wayland | `metadata.desktop` `[SddmGreeterTheme]` needs `QtVersion=6` (default 5 picks the absent Qt 5 greeter, and SDDM falls back to its built-in theme); `MainScript` and `ConfigFile` default to `Main.qml` and `theme.conf`. **`X-SDDM-Theme-InterfaceVersion` isn't read by SDDM 0.21 at all.** **`keyboard` is dead on Wayland:** `layouts` is empty, `enabled` false, `capsLock` never changes, and writing `currentLayout` only changes the number. Plasma's `KeyboardLayout` and `KeyState` **do** work inside `sddm-greeter-qt6` (headless: `us`/`br` listed, switched 0→1, Caps Lock seen on and off) |
| **(e)** Is a module dir shipped in the theme/package found by `import "GhostGate"`? | **Yes, in both greeters**, with no `qmldir` and no `QML_IMPORT_PATH`, including an `import "gate.mjs" as Gate` inside it |

### What changes the plan

1. **Task 5's pointer is a systemd drop-in for KWin, and it needs a fresh login.**
   - `--lock` writes the drop-in and the package. `lock.previous` only needs to record whether the drop-in existed, since nothing of the user's is overwritten.
   - **Task 7 needs a logout and login between `--lock` and the first Meta+L.** Until then, Meta+L shows the stock screen.
   - `--lock` must refuse if `plasmashellrc` has a non-empty `[Shell] ShellPackage`, since that key beats the variable. Today the user's `plasmashellrc` has no `[Shell]` group.
2. **The greeter's option is `--shell <id>`, not `--theme`.** It works only with `--testing` (`main.cpp:129-161`). Task 5's smoke runs `kscreenlocker_greet --testing --shell org.ghostshell.lock` with the package in the sandbox's `XDG_DATA_HOME`.
3. **`LockAdapter` ends the greeter with `Qt.quit()`.** The greeter exits only if `authenticator.unlocked` is true; otherwise it logs a warning and stays locked (`greeterapp.cpp:330-341`).
4. **Task 6's adapter can't use `keyboard`.**
   - It takes layouts from `KeyboardLayout` and Caps Lock from `KeyState`, each in a `Loader`, so a missing Plasma module can't take the theme down.
   - If `layoutsList.length < 2`, the chip holds the current layout and doesn't cycle, as the plan already says.
   - **At the real login screen, two things decide whether this works, and neither was testable here:**
     - SDDM's KWin runs with `--locale1`, so its layouts come from systemd-localed, which today has only `us` (`localectl`: "X11 Layout: us"). The plan's `localectl set-x11-keymap us,br` question stays.
     - `KeyboardLayout` reaches KWin over the session bus, and whether SDDM's greeter session has KWin on its session bus is unverified.
5. **Key injection for Task 5's smoke works in `headless-kwin.sh`'s KWin without changing the script.**
   - `headless-kwin.sh` uses `env -i`, so `KWIN_WAYLAND_NO_PERMISSION_CHECKS` can't reach KWin.
   - Instead, a desktop file in the sandbox's `$XDG_DATA_HOME/applications/` with `Exec=<canonical path of the python running fake_chord.py>` and `X-KDE-Wayland-Interfaces=org_kde_kwin_fake_input` grants fake input (`kwin/src/utils/serviceutils.h:27-49`).
   - `spikes/bar-bridge/fake_chord.py` then typed keys that both greeters' roots received: Caps Lock, A, Caps Lock.

## Setup

- Fedora 44, kscreenlocker 6.7.5, KWin 6.7.5, SDDM 0.21.0, Qt 6.11.2. SELinux is enforcing.
- **Sources:** shallow clones at their tags in `/tmp`, read only, none built:
  - kscreenlocker `v6.7.5`;
  - sddm `v0.21.0`;
  - libplasma `v6.7.5` (the shell package structure);
  - plasma-workspace `v6.7.5`, `shell/` only (plasmashell's shell choice);
  - kwin `v6.7.5`, `src/` only (the greeter's environment, fake-input permission).
- **Runs:** `spikes/lock-greeters/run.sh` starts `scripts/headless-kwin.sh lockspike`.
  - KWin's private session bus is found from its sibling `dbus-daemon`'s listening socket, because KWin isn't dumpable and its `/proc/<pid>/environ` is closed.
  - Each greeter runs for 6 s under `env -i` with every XDG dir, `HOME` and `XDG_RUNTIME_DIR` in `/tmp/gs-sbx-lockspike`, `QT_FORCE_STDERR_LOGGING=1` (without it, Qt logs to the journal), and that bus. The system bus is the real one; nothing calls a logind action on it.
  - The user's `kxkbrc` is copied in before KWin starts.
  - Every `SPIKE` line comes from `console.log` in the probes.
  - KWin is killed on exit. No window reached `wayland-0`.
- **No PAM was started.** The lock probe never calls `startAuthenticating()`, and SDDM's `--test-mode` has no daemon to log in to. The injected keys went to probe roots with no password field.

## (a) Pointing kscreenlocker at our QML

**How the greeter picks its QML** (kscreenlocker `v6.7.5`):
- `UnlockApp::initialize()` calls `setShell(m_shellIntegration->defaultShell())` (`greeter/greeterapp.cpp:178`).
- `defaultShell()` reads `plasmashellrc` `[Shell] ShellPackage`, with `qEnvironmentVariable("PLASMA_DEFAULT_SHELL", "org.kde.plasma.desktop")` as its default (`settings/shell_integration.cpp:35-44`).
  - The key wins whenever it's present.
  - An empty value falls back to the variable.
- `setShell()` loads that `Plasma/Shell` package (`greeterapp.cpp:519-538`).
  - If `verifyPackageApi()` fails, it logs "Lockscreen QML outdated, falling back to default" and uses `org.kde.plasma.desktop`.
  - The QML is the package's `lockscreenmainscript`, `contents/lockscreen/LockScreen.qml` (libplasma `src/plasma/packagestructure/shell/shellpackage.cpp:63-64`).
- **"Outdated":** `verifyPackageApi()` (`greeterapp.cpp:95-111`) wants `X-Plasma-APIVersion` ≥ 2 in the package's metadata, where a missing key counts as 1.
  - If the package lacks `LockScreen.qml` and the file came from its fallback package, the fallback's version is checked instead.
  - A shell package other than `org.kde.plasma.desktop` gets `X-Plasma-FallbackPackage`, by default `org.kde.plasma.desktop`, as its fallback for every missing file (`shellpackage.cpp:69-84`).
- **The `Greeter` `Theme` key in `kscreenlockerrc` is dead.** It's declared (`settings/kscreenlockersettings.kcfg:42`), but nothing in the greeter reads it.
- **`--shell <id>`** sets the package, but only with `--testing` (`greeter/main.cpp:129-161`). `kscreenlocker_greet --help` lists `--shell`, and no `--theme`.
- **Load failure:** if the QML doesn't reach `Ready`, the greeter logs "Failed to load lockscreen QML, falling back to built-in locker" and loads `qrc:/fallbacktheme/LockScreen.qml`. If that fails too, it calls `std::terminate()`, which ksld treats as a crash and keeps the screen locked (`greeterapp.cpp:388-408`).

**Why the variable and not the key.**
- plasmashell's `ShellCorona::defaultShell()` is the same code reading the same key and variable (plasma-workspace `shell/shellcorona.cpp:3110-3119`).
- plasmashell names its layout file after the shell, `plasma-<shell>-appletsrc` (`shellcorona.cpp:701`, `811`).
- So `ShellPackage=org.ghostshell.lock` would restart the desktop on a new, empty applet layout.
- The variable can be given to KWin alone:
  - ksld runs the greeter with `m_greeterEnv` (kscreenlocker `ksldapp.cpp:639`);
  - KWin sets that to its own startup environment (kwin `src/wayland_server.cpp:648`);
  - which is `QProcessEnvironment::systemEnvironment()` at KWin's start (`src/main_wayland.cpp:292`, `658`).
- A drop-in on `plasma-kwin_wayland.service`, the unit running `kwin_wayland_wrapper --xwayland` on this machine, reaches KWin and nothing else.
  - plasmashell is its own systemd unit and doesn't inherit it.
  - `systemctl --user show-environment` doesn't contain the variable today.
- **Don't use `~/.config/plasma-workspace/env/*.sh`:** startplasma exports those to the whole session, plasmashell included.

**Headless runs** (`run.sh`, logs in `/tmp/gs-sbx-lockspike/logs/`):

| Run | Result |
|---|---|
| `--testing --shell org.ghostshell.lockspike` | probe loaded (`SPIKE package = "org.ghostshell.lockspike"`) |
| `--testing`, `PLASMA_DEFAULT_SHELL=org.ghostshell.lockspike` | probe loaded: **the production route** |
| `--testing`, `PLASMA_DEFAULT_SHELL=<absolute package path>` | probe loaded: an absolute path works too, with no install under `XDG_DATA_HOME` |
| `--testing`, `plasmashellrc` `ShellPackage=org.ghostshell.lockspike`, variable naming the outdated package | probe loaded: **the key beats the variable** |
| `--testing --shell org.ghostshell.lockspike-old` (no `X-Plasma-APIVersion`) | `Lockscreen QML outdated, falling back to default`; stock screen |
| `--testing`, `PLASMA_DEFAULT_SHELL=org.ghostshell.notinstalled` | `Lockscreen QML outdated, falling back to default`: **uninstalling the package without removing the drop-in degrades to the stock screen** |
| `--testing --shell org.ghostshell.lockspike-broken` (unknown import) | `Failed to load lockscreen QML, falling back to built-in locker` |

## (b) The lock greeter's QML API

**Context properties**, set on the engine's root context for each screen's view (`greeterapp.cpp:343-348`):

| Name | Value seen |
|---|---|
| `authenticator` | `PamAuthenticators` (below) |
| `kscreenlocker_userName` | `"Gabriel Mitelman Tkacz"`: the full name, or the login name when that's empty (`greeterapp.cpp:183-187`) |
| `kscreenlocker_userImage` | `""` here: the face icon path |
| `org_kde_plasma_screenlocker_greeter_interfaceVersion` | `2` |
| `org_kde_plasma_screenlocker_greeter_view` | the view (`PlasmaQuick::QuickViewSharedEngine`) |
| `config` | `KConfigPropertyMap` of `kscreenlockerrc` `[Greeter][LnF]`, using the package's `contents/lockscreen/config.xml` if one exists (`settings/shell_integration.cpp:46-61`) |
| `wallpaper`, `wallpaperIntegration` | the lock wallpaper plugin's root, set per view (`greeterapp.cpp:234-236`) |

**Root properties the greeter writes:**
- `locked`, after loading (`greeterapp.cpp:415-416`), and again when the grace time ends (`:493-496`). It was `true` with `--testing`.
- `viewVisible = true`, after the first frame (`:444-445`).

The stock root also declares `notification`, `clearPassword()` and `notificationRepeated()`. The greeter doesn't need them.

**`authenticator`** (`greeter/pamauthenticators.h:17-94`). The members the probe listed match the header:

| Kind | Members |
|---|---|
| Properties | `busy`, `prompt`, `promptForSecret`, `infoMessage`, `errorMessage` (all from the interactive PAM conversation); `unlocked`; `authenticatorTypes` (flags of available non-interactive authenticators: `Fingerprint` = 1, `Smartcard` = 2, `pamauthenticator.h:31-35`; 0 here); `state` (`Idle` 0 / `Authenticating` 1); `hadPrompt` |
| Methods | `startAuthenticating()`, `stopAuthenticating()`, `respond(QByteArray)` (QML passes a string), `cancel()` |
| Signals | `succeeded()`, `failed(kind, authenticator)` (kind 0 is the interactive one: `pamauthenticators.cpp:50-54`), `noninteractiveError` and `noninteractiveInfo(kind, authenticator)`, `loginFailedDelayStarted(kind, authenticator, uSecDelay)`, plus the `*Changed` notifiers |

There is **no `graceLocked` property**: the probe read `undefined`, though stock `MainBlock.qml:67` binds to it.

**The flow, as the stock `LockScreenUi.qml` drives it:**
1. Call `authenticator.startAuthenticating()` when the UI shows (`:136`), and again 3 s after a failure (`:180-185`). It returns early while authenticating or grace-locked (`pamauthenticators.cpp:142-154`), and it starts the fingerprint and smartcard PAM services as well.
2. `onPromptForSecretChanged` focuses the field (`:80-83`).
3. `authenticator.respond(password)` (`:294`).
4. `onFailed(kind)` shows the failure when `kind == 0` (`:44-53`). `prompt`, `infoMessage` and `errorMessage` are PAM's text (`:69-79`).
5. `onSucceeded`: `Qt.quit()` if `hadPrompt`; otherwise a "no password" unlock screen (`:55-67`).

**What `Qt.quit()` does:**
- With `authenticator.unlocked` true, the greeter prints "Unlocked" and `_exit(0)`s (`greeterapp.cpp:330-341`).
- Otherwise it logs "Greeter tried to quit without being unlocked" and stays.

**Actions:** `import org.kde.plasma.private.sessions`, then `SessionManagement {}` (stock `LockScreenUi.qml:86-101`, `:300-316`):
- `canSuspend` and `suspend()`;
- `canHibernate` and `hibernate()`;
- `canSwitchUser` and `switchUser()`;
- the signals `aboutToSuspend` (stock clears the password then) and `resumingFromSuspend`.

Seen: `canSuspend` true, `canSwitchUser` true, `canHibernate` false. No action was called.

**Caps Lock:** `import org.kde.plasma.private.keyboardindicator`, then `KeyState { key: Qt.Key_CapsLock }`, reading `locked` (stock `:90-93`). Seen: `false`, then `true` and `false` again as Caps Lock was tapped twice.

**Layouts:** `import org.kde.plasma.workspace.keyboardlayout`, then `KeyboardLayout {}`:
- `layoutsList` is an array of `{shortName, displayName, longName}`; `layout` is the current index;
- `switchToNextLayout()` and `switchToPreviousLayout()` switch;
- it uses KWin's `org.kde.keyboard` D-Bus service;
- stock wraps it in `org.kde.plasma.workspace.components` `KeyboardLayoutSwitcher`.

Seen: `[{"shortName":"us","longName":"English (US)"},{"shortName":"br","longName":"Portuguese (Brazil)"}]`, and `layout` went 0 → 1 after `switchToNextLayout()`. KWin keeps the layout between greeter runs.

**Screens:**
- The greeter makes one `QuickViewSharedEngine` per `QScreen` and adds or removes them as screens come and go (`greeterapp.cpp:272-292`).
- Each view is a layer-shell surface on its screen, at `LayerTop` with exclusive keyboard (`:421-428`). The root is sized to the view.
- In QML, identify the screen with `Screen.name`, which read `"Virtual-0"`, and the window with `Window.window`.
- **From source, untested with two outputs:** the context properties are set on `view->engine()->rootContext()` (`:329`), and the engine is shared, so `org_kde_plasma_screenlocker_greeter_view` may hold the last view made. Don't rely on it.
- The headless KWin has one output, so the multi-screen case wasn't run.

**Other greeter behaviour:**
- On **Escape release**, when not in `--testing`, the greeter turns the screens off through DPMS and swallows the key (`greeterapp.cpp:578-598`).
- An `objectName: "onScreenDisplay"` child receives plasmashell's OSD (`:648-677`).
- The engine's network access is blocked (`:309-312`). Local file reads aren't.

**Input:** keys injected through KWin's fake input reached the root's `Keys.onPressed`: Caps Lock (16777252), A (65), Caps Lock.

## (c) Reading `gate.ini` with `QtCore` `Settings`

The probe declares no properties on the `Settings` objects and reads with `value("gate/style", default)`. A declared property would be written back to the file whenever it changes.

| Greeter | `location` | Result |
|---|---|---|
| lock | `StandardPaths.writableLocation(StandardPaths.GenericStateLocation) + "/ghost-shell/gate.ini"`, which resolved to `file:///tmp/gs-sbx-lockspike/state/ghost-shell/gate.ini` | `"terminal"` (the file's value) |
| lock | same directory, `absent.ini` | `"DEFAULT"` (the fallback) |
| SDDM | `file:///var/lib/ghost-shell/gate.ini` (absent on this machine) | `"DEFAULT"` |
| SDDM | stand-in `file:///tmp/gs-sbx-lockspike/var-lib-ghost-shell/gate.ini` | `"idcheck"` (the file's value) |

Both files had the same sha256 before and after all runs.

- **SDDM's `config` and `theme.conf.user`:** SDDM reads `<theme>/<ConfigFile>`, then `<ConfigFile>.user`, where a non-empty user value overrides (`src/common/ThemeConfig.cpp:35-74`).
  - Seen: `config.probe` came from `theme.conf` and `config.fromUserFile` from `theme.conf.user`.
  - So the plan's fallback reader would work, but nothing needs it.

## (d) SDDM 0.21 theme metadata and `keyboard`

**Metadata:** `[SddmGreeterTheme]` in `metadata.desktop`:
- `MainScript` (default `Main.qml`), `ConfigFile` (default `theme.conf`), `TranslationsDirectory` (default `.`) and `QtVersion` (default **5**) (`src/common/ThemeMetadata.cpp:61-64`).
- The daemon runs `sddm-greeter-qt<N>`, with no suffix for 5. If that greeter is missing, it falls back to the built-in theme on its own Qt version (`src/daemon/Greeter.cpp:83-101`). Only `sddm-greeter-qt6` is installed, so **`QtVersion=6` is required**.
- `X-SDDM-Theme-InterfaceVersion` appears nowhere in SDDM's source. The installed `glyph` theme carries it, and the stock `breeze` theme carries `QtVersion=6`.
- Shipping both does no harm.
- **Load failure:** "Fallback to embedded theme" (`src/greeter/GreeterApp.cpp:205-219`), seen with the broken theme.

**Context properties** (`GreeterApp.cpp:187-194`):
- `sessionModel`, `screenModel`, `userModel`, `config`, `sddm`, `keyboard`, `primaryScreen`, `__sddm_errors`.
- **`sddm`** (`src/greeter/GreeterProxy.h:35-84`):
  - properties `hostName`, `canPowerOff`, `canReboot`, `canSuspend`, `canHibernate`, `canHybridSleep`;
  - slots `powerOff()`, `reboot()`, `suspend()`, `hibernate()`, `hybridSleep()`, `login(user, password, sessionIndex)`;
  - signals `loginSucceeded()`, `loginFailed()`, `informationMessage(message)`.
  - In test mode everything is empty or false, because there's no daemon.
- **`userModel`:** `lastUser`, `lastIndex`, `count`; roles `name`, `realName`, `homeDir`, `icon`, `needsPassword` (`UserModel.h:33-45`).
- **`sessionModel`:** `lastIndex`, `count`; roles `directory`, `file`, `type`, `name`, `exec`, `comment` (`SessionModel.h:36-46`).
- A theme can set `needsFullUserModel=false` in `theme.conf` (`GreeterApp.cpp:117`).
- The installed user's real `userModel` wasn't seen. Test mode gave `count` 1 and an empty `lastUser`.

**`keyboard` under Wayland** (`src/greeter/KeyboardModel.h:36-43`):
- It has `numLock`, `capsLock`, `currentLayout` (writable), `layouts` (`KeyboardLayout` objects with `shortName`/`longName`) and `enabled`.
- Under Wayland, `KeyboardModel` uses `WaylandKeyboardBackend` (`KeyboardModel.cpp:38-41`), whose `init()` deliberately leaves `layouts` empty, "We can't actually switch keyboard layout yet". `sendChanges()` and `dispatchEvents()` are empty (`waylandkeyboardbackend.cpp:93-110`).

Seen:

| Probe | Result |
|---|---|
| `keyboard.enabled` | `false` |
| `keyboard.layouts` | `[]` |
| `keyboard.capsLock` | `false`, and no change notification while Caps Lock was tapped twice |
| `keyboard.currentLayout = 1` | reads back `1`; nothing switched |
| Plasma `KeyboardLayout` in a `Loader` | loaded; `us`/`br` listed; `switchToNextLayout()` moved `layout` 0 → 1 |
| Plasma `KeyState` in a `Loader` | loaded; Caps Lock seen `true`, then `false` |
| root `Keys.onPressed` | Caps Lock, A, Caps Lock received |

**This machine's real login screen:**
- `/usr/lib/sddm/sddm.conf.d/plasma-wayland.conf` sets `DisplayServer=wayland` and `CompositorCommand=kwin_wayland --no-global-shortcuts --no-lockscreen --inputmethod plasma-keyboard --locale1`, and `GreeterEnvironment=QT_WAYLAND_SHELL_INTEGRATION=layer-shell`.
- `--locale1` gives KWin the systemd-localed keymap, which today is `us` only.

## (e) A module directory inside each package

- `import "GhostGate"` resolves against the importing file's URL. A directory import needs no `qmldir`.
- Seen in both greeters:
  - `GhostGate/Probe.qml` loaded;
  - its own `import "gate.mjs" as Gate` loaded;
  - `Gate.greet()` returned "GhostGate gate.mjs loaded in the lock greeter" and "… in sddm-greeter-qt6".
- Neither greeter lets us set `QML_IMPORT_PATH`. SDDM adds only its own `IMPORTS_INSTALL_DIR` (`GreeterApp.cpp:170`). None was needed.
- The packages should hold a **copy** of `GhostGate`, not a symlink: the SDDM theme lives under `/usr/share/sddm/themes`, and user `sddm` can't follow a link into the home directory.
- The Plasma modules the adapters need are system-wide in `/usr/lib64/qt6/qml`, so both greeters can import them:
  - `org.kde.plasma.private.sessions`;
  - `org.kde.plasma.private.keyboardindicator`;
  - `org.kde.plasma.workspace.keyboardlayout`.

## Open risks

- **SELinux at the login screen (unverified).** SELinux is enforcing.
  - SDDM runs as `xdm_t`, and `sddm-greeter-qt6` is `bin_t`, so it probably stays in `xdm_t`.
  - A new `/var/lib/ghost-shell` gets `var_lib_t` (`matchpathcon`). SDDM's own directory is `xdm_var_lib_t`.
  - Whether `xdm_t` may read `var_lib_t` files wasn't checked: `sesearch` isn't installed.
  - Task 6 or 7 should check for an AVC denial (`ausearch -m avc -ts recent`) on the first real login screen.
  - If it's denied, label the directory: `semanage fcontext -a -t xdm_var_lib_t '/var/lib/ghost-shell(/.*)?'` then `restorecon -R /var/lib/ghost-shell`.
  - The `theme.conf.user` symlink fallback wouldn't help, since the target keeps its label.
  - Either way the gate falls back to its defaults, so the screen still shows.
- **The plasma `private` modules aren't a stable API.** `LockAdapter` is the only place that touches them, so a Plasma update breaks one file, and the greeter falls back to KDE's built-in screen rather than unlocking.
- **SDDM layouts at the real login** depend on two things checked only on the live machine (Task 7):
  - localed holding `us,br`;
  - KWin's `org.kde.keyboard` being on the greeter's session bus.
- **Not run:** two outputs, a real PAM conversation, `--ksldfd` (the real ksld path), and SDDM with a daemon.

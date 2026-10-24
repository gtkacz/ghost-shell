const DEFAULT_DATA_DIRS = '/usr/local/share:/usr/share';
const FLATPAK_EXPORTS = '/flatpak/exports/share';
const SCRIPT_NAME = 'ghost-launch';

// DesktopEntry exposes no path, so the shell looks the id up in the same directories Quickshell scanned and hands the
// first hit to kioclient. A miss is reported on stderr (the journal) rather than launching something else.
const FIND_AND_EXEC =
  'id=$1; shift; for d; do f=$d/applications/$id.desktop; if [ -f "$f" ]; then exec kioclient exec "$f"; fi; done; ' +
  'echo "ghost-shell: no desktop file for $id" >&2; exit 1';

export function searchDirs(dataHome, dataDirs) {
  const dirs = [dataHome, dataHome + FLATPAK_EXPORTS, ...(dataDirs || DEFAULT_DATA_DIRS).split(':')];
  return [...new Set(dirs.filter((dir) => dir !== ''))];
}

// KIO starts the app in its own app-<id>@ scope with a startup notification and activation token, as Plasma does. The
// id and directories travel as positional parameters, so nothing from them is parsed as shell.
export function appCommand(id, dirs) {
  return ['sh', '-c', FIND_AND_EXEC, SCRIPT_NAME, id, ...dirs];
}

// Like an app, a file opens in KIO's own scope, so it outlives the shell. ghostd only lists absolute paths, which
// kioclient reads as paths rather than options.
export function fileCommand(path) {
  return ['kioclient', 'exec', path];
}

// kioclient cannot run a desktop action, so it gets a transient service of the user manager, which is where KDE puts
// apps too. The command is Quickshell's own parse of the action's Exec.
export function actionCommand(action, home) {
  return [
    'systemd-run',
    '--user',
    '--collect',
    '--quiet',
    '--slice=app.slice',
    `--description=${action.name}`,
    `--working-directory=${home}`,
    '--',
    ...action.command,
  ];
}

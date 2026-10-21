use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghost_proto::ResolvedConfig;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::load::{ConfigError, load_path};

const DEBOUNCE: Duration = Duration::from_millis(150);
const REMOVED_MESSAGE: &str = "config file removed; keeping last configuration";

/// Keeps the watcher and its reload task running; dropping it stops both.
pub struct WatchHandle {
    _watcher: RecommendedWatcher,
    reload_task: JoinHandle<()>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.reload_task.abort();
    }
}

/// Watches the directory holding `path` and republishes the config whenever the file changes.
///
/// A save that fails to parse, or a removal of the file, is reported on the error channel and the
/// last good config stays published. The error channel is unbounded so that reporting never
/// stalls the reload task or notify's thread.
///
/// Must be called from within a Tokio runtime with the time driver enabled, because the debounce
/// runs as a spawned task that waits on `tokio::time::timeout`; without the driver that task
/// panics on the first event.
pub fn spawn_watcher(
    path: PathBuf,
    initial: ResolvedConfig,
) -> notify::Result<(
    WatchHandle,
    watch::Receiver<ResolvedConfig>,
    mpsc::UnboundedReceiver<ConfigError>,
)> {
    let (Some(directory), Some(file_name)) = (path.parent(), path.file_name()) else {
        return Err(notify::Error::generic(
            "config path must name a file inside a directory",
        ));
    };
    let file_name = file_name.to_owned();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (error_tx, error_rx) = mpsc::unbounded_channel();
    let (config_tx, config_rx) = watch::channel(initial);

    let watcher_error_tx = error_tx.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        // A send only fails once its receiver is gone, and then nobody is left to notify.
        match result {
            Ok(event) if names_file(&event, &file_name) => {
                let _ = event_tx.send(());
            }
            Ok(_) => {}
            Err(error) => {
                let _ = watcher_error_tx.send(ConfigError {
                    message: format!("config watcher: {error}"),
                });
            }
        }
    })?;
    // Watching the directory rather than the file keeps working across atomic-rename saves, which
    // replace the file's inode.
    watcher.watch(directory, RecursiveMode::NonRecursive)?;

    let reload_task = tokio::spawn(reload_when_quiet(path, event_rx, config_tx, error_tx));
    Ok((
        WatchHandle {
            _watcher: watcher,
            reload_task,
        },
        config_rx,
        error_rx,
    ))
}

fn names_file(event: &Event, file_name: &OsStr) -> bool {
    // inotify reports opens too, so without this our own read would retrigger a reload forever.
    !matches!(event.kind, EventKind::Access(_))
        && event
            .paths
            .iter()
            .any(|path| path.file_name() == Some(file_name))
}

async fn reload_when_quiet(
    path: PathBuf,
    mut events: mpsc::UnboundedReceiver<()>,
    config_tx: watch::Sender<ResolvedConfig>,
    error_tx: mpsc::UnboundedSender<ConfigError>,
) {
    let mut removal_reported = false;
    while events.recv().await.is_some() {
        while let Ok(Some(())) = timeout(DEBOUNCE, events.recv()).await {}

        // load_path maps a missing file to the defaults, so existence is checked on both sides of
        // the read: a removal racing the read must not publish those defaults, and neither must a
        // creation racing it.
        let missing_before_load = is_missing(&path);
        let loaded = load_path(&path);
        if is_missing(&path) {
            if !removal_reported {
                // A dropped error receiver only means nobody wants reports; config updates still
                // publish.
                let _ = error_tx.send(ConfigError {
                    message: REMOVED_MESSAGE.to_owned(),
                });
            }
            removal_reported = true;
            continue;
        }
        if missing_before_load {
            // The creation's own event starts another pass that reads the real contents.
            continue;
        }
        removal_reported = false;
        match loaded {
            Ok(config) => publish_if_changed(&config_tx, config),
            Err(error) => {
                let _ = error_tx.send(error);
            }
        }
    }
}

fn is_missing(path: &Path) -> bool {
    matches!(path.try_exists(), Ok(false))
}

fn publish_if_changed(config_tx: &watch::Sender<ResolvedConfig>, config: ResolvedConfig) {
    config_tx.send_if_modified(|current| {
        let changed = *current != config;
        if changed {
            *current = config;
        }
        changed
    });
}

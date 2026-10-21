use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghost_proto::ResolvedConfig;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::load::{ConfigError, load_path};

/// How long a watched path must stay quiet before a burst of events is acted on.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(150);
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
                    detail: None,
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
        let pass = Pass {
            missing_before_load,
            loaded,
            missing_after_load: is_missing(&path),
        };
        settle_pass(pass, &mut removal_reported, &config_tx, &error_tx);
    }
}

/// What one debounce pass observed: the file's existence on either side of its read, and the read.
struct Pass {
    missing_before_load: bool,
    loaded: Result<ResolvedConfig, ConfigError>,
    missing_after_load: bool,
}

fn settle_pass(
    pass: Pass,
    removal_reported: &mut bool,
    config_tx: &watch::Sender<ResolvedConfig>,
    error_tx: &mpsc::UnboundedSender<ConfigError>,
) {
    if pass.missing_after_load {
        if !*removal_reported {
            // A dropped error receiver only means nobody wants reports; config updates still
            // publish.
            let _ = error_tx.send(ConfigError {
                message: REMOVED_MESSAGE.to_owned(),
                detail: None,
            });
        }
        *removal_reported = true;
        return;
    }
    // Cleared before the skip below, because the file existed at the end of this pass: a removal
    // after it is a new one and must be reported even if the pass skipped its own creation.
    *removal_reported = false;
    if pass.missing_before_load {
        // The creation's own event starts another pass that reads the real contents.
        return;
    }
    match pass.loaded {
        Ok(config) => publish_if_changed(config_tx, config),
        Err(error) => {
            let _ = error_tx.send(error);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn gone() -> Pass {
        Pass {
            missing_before_load: false,
            loaded: Ok(ResolvedConfig::default()),
            missing_after_load: true,
        }
    }

    fn present(missing_before_load: bool) -> Pass {
        Pass {
            missing_before_load,
            loaded: Ok(ResolvedConfig::default()),
            missing_after_load: false,
        }
    }

    #[test]
    fn a_removal_after_a_creation_that_raced_the_read_is_reported_again() {
        let (config_tx, _config_rx) = watch::channel(ResolvedConfig::default());
        let (error_tx, mut errors) = mpsc::unbounded_channel();
        let mut removal_reported = false;

        settle_pass(gone(), &mut removal_reported, &config_tx, &error_tx);
        settle_pass(present(true), &mut removal_reported, &config_tx, &error_tx);
        settle_pass(gone(), &mut removal_reported, &config_tx, &error_tx);

        let reported: Vec<String> = std::iter::from_fn(|| errors.try_recv().ok())
            .map(|error| error.message)
            .collect();
        assert_eq!(reported, [REMOVED_MESSAGE, REMOVED_MESSAGE]);
    }
}

use std::path::PathBuf;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::task::spawn_blocking;
use tokio::time::timeout;
use tracing::{error, warn};

use crate::config::DEBOUNCE;

/// Watches the widget search dirs, so installing, editing or removing a widget re-resolves the
/// layout without waiting for a config change.
pub(super) struct WidgetWatch {
    // Held so the channel never closes, even while no dir is watched.
    changes_tx: mpsc::UnboundedSender<()>,
    changes: mpsc::UnboundedReceiver<()>,
    watcher: Option<RecommendedWatcher>,
}

impl WidgetWatch {
    pub(super) fn new() -> Self {
        let (changes_tx, changes) = mpsc::unbounded_channel();
        Self {
            changes_tx,
            changes,
            watcher: None,
        }
    }

    /// Replaces the watched set with every dir in `dirs` that exists now.
    pub(super) async fn watch(&mut self, dirs: Vec<PathBuf>) {
        // Dropped first, so the old and new watchers never report the same change twice.
        self.watcher = None;
        let changes = self.changes_tx.clone();
        // A recursive watch walks every subfolder, which may sit on a slow mount.
        match spawn_blocking(move || watch_dirs(&dirs, changes)).await {
            Ok(watcher) => self.watcher = watcher,
            Err(error) => error!(%error, "cannot watch the widget dirs"),
        }
    }

    /// Resolves at the first change after the last call. Cancel-safe, so it can be a `select!`
    /// branch; follow it with [`WidgetWatch::settle`].
    pub(super) async fn changed(&mut self) {
        // `changes_tx` lives in `self`, so the channel cannot close: this only resolves on a
        // change.
        self.changes.recv().await;
    }

    /// Waits until the dirs stay quiet for the debounce period, so copying a widget folder in
    /// triggers one rescan rather than one per file.
    pub(super) async fn settle(&mut self) {
        while let Ok(Some(())) = timeout(DEBOUNCE, self.changes.recv()).await {}
    }
}

fn watch_dirs(dirs: &[PathBuf], changes: mpsc::UnboundedSender<()>) -> Option<RecommendedWatcher> {
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        // Discovery opens every manifest, which inotify reports as access; reacting to that would
        // rescan forever. An error may mean lost events, so it rescans to be safe.
        let is_change = result.map_or(true, |event| !matches!(event.kind, EventKind::Access(_)));
        if is_change {
            // A send only fails once the daemon is shutting down, when nobody needs the change.
            let _ = changes.send(());
        }
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            warn!(
                %error,
                "cannot watch the widget dirs; widget changes apply at the next config reload"
            );
            return None;
        }
    };
    for dir in dirs {
        // A dir that does not exist has nothing to watch, and watching its parents instead would
        // wake the daemon for unrelated files. The next config reload rebuilds this set, which
        // picks the dir up once it has been created.
        if !dir.is_dir() {
            continue;
        }
        if let Err(error) = watcher.watch(dir, RecursiveMode::Recursive) {
            warn!(%error, dir = %dir.display(), "cannot watch a widget dir");
        }
    }
    Some(watcher)
}

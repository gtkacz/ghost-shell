use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use ghost_proto::ResolvedConfig;
use ghostd::config::{ConfigError, WatchHandle, load_path, spawn_watcher};
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(2);
const INITIAL_PANEL_FILL: u32 = 55;
const CONFIG_FILE: &str = "config.toml";

struct Watching {
    dir: TempDir,
    config: watch::Receiver<ResolvedConfig>,
    errors: mpsc::UnboundedReceiver<ConfigError>,
    _handle: WatchHandle,
}

impl Watching {
    fn path(&self) -> PathBuf {
        self.dir.path().join(CONFIG_FILE)
    }

    async fn next_config(&mut self) -> ResolvedConfig {
        timeout(WAIT, self.config.changed())
            .await
            .expect("no config published in time")
            .expect("watcher stopped");
        self.config.borrow_and_update().clone()
    }

    async fn next_error(&mut self) -> ConfigError {
        timeout(WAIT, self.errors.recv())
            .await
            .expect("no error reported in time")
            .expect("watcher stopped")
    }

    fn panel_fill(&self) -> u32 {
        self.config.borrow().appearance.panel_fill
    }
}

fn panel_fill_toml(percent: u32) -> String {
    format!("[appearance]\npanel_fill = {percent}\n")
}

fn watch_config_with_panel_fill(percent: u32) -> Watching {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(CONFIG_FILE);
    fs::write(&path, panel_fill_toml(percent)).unwrap();
    let initial = load_path(&path).unwrap();
    let (handle, config, errors) = spawn_watcher(path, initial).unwrap();
    Watching {
        dir,
        config,
        errors,
        _handle: handle,
    }
}

#[tokio::test]
async fn plain_save_publishes() {
    let mut watching = watch_config_with_panel_fill(INITIAL_PANEL_FILL);

    fs::write(watching.path(), panel_fill_toml(40)).unwrap();

    assert_eq!(watching.next_config().await.appearance.panel_fill, 40);
}

#[tokio::test]
async fn atomic_rename_publishes() {
    let mut watching = watch_config_with_panel_fill(INITIAL_PANEL_FILL);
    let temporary = watching.dir.path().join("config.toml.tmp");

    fs::write(&temporary, panel_fill_toml(30)).unwrap();
    fs::rename(&temporary, watching.path()).unwrap();

    assert_eq!(watching.next_config().await.appearance.panel_fill, 30);
}

#[tokio::test]
async fn half_written_keeps_last_good() {
    let mut watching = watch_config_with_panel_fill(INITIAL_PANEL_FILL);

    fs::write(watching.path(), "[appearance\npanel_fi").unwrap();

    watching.next_error().await;
    assert!(!watching.config.has_changed().unwrap());
    assert_eq!(watching.panel_fill(), INITIAL_PANEL_FILL);

    fs::write(watching.path(), panel_fill_toml(35)).unwrap();

    assert_eq!(watching.next_config().await.appearance.panel_fill, 35);
}

#[tokio::test]
async fn removed_file_keeps_last_good() {
    let mut watching = watch_config_with_panel_fill(INITIAL_PANEL_FILL);

    fs::remove_file(watching.path()).unwrap();

    assert_eq!(
        watching.next_error().await.message,
        "config file removed; keeping last configuration"
    );
    assert!(!watching.config.has_changed().unwrap());
    assert_eq!(watching.panel_fill(), INITIAL_PANEL_FILL);
}

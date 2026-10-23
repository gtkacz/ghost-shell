//! Keeps `bar.pinned` in the user's config file: imports the Plasma panel's launchers once at
//! start, then pins and unpins on request. The config watcher reloads the file like any save.

use std::future::pending;
use std::path::PathBuf;

use ghost_proto::calls;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::config::edit::{PinChange, import_plasma_launchers, set_pinned};
use crate::server::{Call, Feed};

const APPLETSRC_FILE: &str = "plasma-org.kde.plasma.desktop-appletsrc";
const MIMEAPPS_FILE: &str = "mimeapps.list";
const APP_ARG: &str = "app";

/// `plasma_dir` holds the Plasma panel's `…-appletsrc` and `mimeapps.list`.
pub fn feed(config_path: PathBuf, plasma_dir: PathBuf) -> Feed {
    Feed::new("pins", move |context| async move {
        match import_plasma_launchers(
            &config_path,
            &plasma_dir.join(APPLETSRC_FILE),
            &plasma_dir.join(MIMEAPPS_FILE),
        ) {
            Ok(Some(imported)) => info!(count = imported.len(), "imported the Plasma launchers"),
            Ok(None) => {}
            Err(error) => warn!(%error, "cannot import the Plasma launchers"),
        }
        run(config_path, context.calls).await;
    })
}

/// Never returns: calls keep coming for as long as the daemon runs.
async fn run(config_path: PathBuf, mut incoming: broadcast::Receiver<Call>) {
    loop {
        match incoming.recv().await {
            Ok(call) => handle(&config_path, call),
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                warn!(missed, "pin changes were dropped: the feed fell behind");
            }
            Err(broadcast::error::RecvError::Closed) => pending().await,
        }
    }
}

fn handle(config_path: &std::path::Path, call: Call) {
    let app = call
        .args
        .get(APP_ARG)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let change = match (call.method.as_str(), app) {
        (calls::BAR_PIN, Some(app)) => PinChange::Add(app),
        (calls::BAR_UNPIN, Some(app)) => PinChange::Remove(app),
        (calls::BAR_PIN | calls::BAR_UNPIN, None) => {
            warn!(method = call.method, "refused a pin change without an app");
            return;
        }
        _ => return,
    };
    if let Err(error) = set_pinned(config_path, change) {
        warn!(method = call.method, %error, "refused a pin change");
    }
}

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use ghost_proto::ProviderSpec;
use tokio::process::Command;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep};
use tracing::{info, warn};

/// The environment variable that tells a provider where the daemon's socket is.
pub const SOCKET_ENV: &str = "GHOST_SOCKET";

const FIRST_RESTART_DELAY: Duration = Duration::from_secs(1);
const MAX_RESTART_DELAY: Duration = Duration::from_secs(30);
const RESTART_DELAY_GROWTH: u32 = 2;
// A provider that ran this long was healthy, so its next exit starts the backoff over.
const HEALTHY_RUN: Duration = Duration::from_secs(30);

/// Keeps the configured providers running; dropping it kills them.
pub struct Supervisor {
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // Aborting a task drops its child handle, and `kill_on_drop` then kills the process.
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Starts each provider once with `GHOST_SOCKET=<socket>`. A provider with `restart` set is
/// started again whenever it exits, after 1, 2, 4… seconds up to 30.
///
/// Must be called from within a Tokio runtime.
pub fn supervise(specs: &[ProviderSpec], socket: &Path) -> Supervisor {
    Supervisor {
        tasks: specs
            .iter()
            .map(|spec| tokio::spawn(keep_running(spec.clone(), socket.to_owned())))
            .collect(),
    }
}

async fn keep_running(spec: ProviderSpec, socket: PathBuf) {
    let mut delay = FIRST_RESTART_DELAY;
    loop {
        let started = Instant::now();
        run_once(&spec, &socket).await;
        if !spec.restart {
            return;
        }
        if started.elapsed() >= HEALTHY_RUN {
            delay = FIRST_RESTART_DELAY;
        }
        info!(provider = %spec.name, "restarting in {delay:?}");
        sleep(delay).await;
        delay = (delay * RESTART_DELAY_GROWTH).min(MAX_RESTART_DELAY);
    }
}

async fn run_once(spec: &ProviderSpec, socket: &Path) {
    let Some((program, args)) = spec.exec.split_first() else {
        warn!(provider = %spec.name, "provider has an empty exec");
        return;
    };
    let spawned = Command::new(program)
        .args(args)
        .env(SOCKET_ENV, socket)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            warn!(provider = %spec.name, %error, "cannot start provider");
            return;
        }
    };
    info!(provider = %spec.name, pid = child.id(), "provider started");
    match child.wait().await {
        Ok(status) => warn!(provider = %spec.name, %status, "provider exited"),
        Err(error) => warn!(provider = %spec.name, %error, "lost track of provider"),
    }
}

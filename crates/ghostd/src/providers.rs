use std::collections::BTreeMap;
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
    socket: PathBuf,
    running: BTreeMap<String, Running>,
}

struct Running {
    spec: ProviderSpec,
    task: JoinHandle<()>,
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // Aborting a task drops its child handle, and `kill_on_drop` then kills the process.
        for running in self.running.values() {
            running.task.abort();
        }
    }
}

/// Starts each provider once with `GHOST_SOCKET=<socket>`. A provider with `restart` set is
/// started again whenever it exits, after 1, 2, 4… seconds up to 30.
///
/// Must be called from within a Tokio runtime.
pub fn supervise(specs: &[ProviderSpec], socket: &Path) -> Supervisor {
    let mut supervisor = Supervisor {
        socket: socket.to_owned(),
        running: BTreeMap::new(),
    };
    for spec in specs {
        supervisor.start(spec.clone());
    }
    supervisor
}

impl Supervisor {
    /// Brings the running providers in line with `specs`: starts the new ones, kills the removed
    /// ones, restarts those whose spec changed, and leaves the rest, backoff included, alone.
    pub async fn reconcile(&mut self, specs: &[ProviderSpec]) {
        let running: Vec<&ProviderSpec> = self.running.values().map(|run| &run.spec).collect();
        let plan = plan(&running, specs);
        for name in plan.stop {
            if let Some(stopped) = self.running.remove(&name) {
                stopped.task.abort();
                // Awaited so a restarted provider never runs alongside the one it replaces.
                let _cancelled = stopped.task.await;
                info!(provider = %name, "provider stopped");
            }
        }
        for spec in plan.start {
            self.start(spec.clone());
        }
    }

    fn start(&mut self, spec: ProviderSpec) {
        let task = tokio::spawn(keep_running(spec.clone(), self.socket.clone()));
        let name = spec.name.clone();
        if let Some(replaced) = self.running.insert(name, Running { spec, task }) {
            replaced.task.abort();
        }
    }
}

#[derive(Debug, PartialEq)]
struct Plan<'a> {
    stop: Vec<String>,
    start: Vec<&'a ProviderSpec>,
}

/// A spec that differs in any field is stopped and started again rather than patched in place.
fn plan<'a>(running: &[&ProviderSpec], wanted: &'a [ProviderSpec]) -> Plan<'a> {
    Plan {
        stop: running
            .iter()
            .filter(|spec| !wanted.contains(spec))
            .map(|spec| spec.name.clone())
            .collect(),
        start: wanted
            .iter()
            .filter(|spec| !running.contains(spec))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, program: &str) -> ProviderSpec {
        ProviderSpec {
            name: name.to_owned(),
            exec: vec![program.to_owned()],
            restart: true,
        }
    }

    #[test]
    fn plan_starts_added_stops_removed_and_keeps_unchanged() {
        let kept = spec("kept", "kept-bin");
        let removed = spec("removed", "removed-bin");
        let added = spec("added", "added-bin");

        let wanted = [kept.clone(), added.clone()];

        let plan = plan(&[&kept, &removed], &wanted);

        assert_eq!(
            plan,
            Plan {
                stop: vec!["removed".to_owned()],
                start: vec![&added],
            }
        );
    }

    #[test]
    fn plan_restarts_a_provider_whose_spec_changed() {
        let before = spec("inmet", "inmet-v1");
        let after = ProviderSpec {
            restart: false,
            ..spec("inmet", "inmet-v2")
        };

        let plan = plan(&[&before], std::slice::from_ref(&after));

        assert_eq!(
            plan,
            Plan {
                stop: vec!["inmet".to_owned()],
                start: vec![&after],
            }
        );
    }
}

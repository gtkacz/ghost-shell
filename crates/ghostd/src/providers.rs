use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use ghost_proto::{ProviderSpec, ResolvedConfig, topics};
use nix::errno::Errno;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep, timeout};
use tracing::{info, warn};

/// The environment variable that tells a provider where the daemon's socket is.
pub const SOCKET_ENV: &str = "GHOST_SOCKET";
/// Names the provider `location.alerts` adds, so configured providers may not use it.
pub const ALERTS_NAME_PREFIX: &str = "alerts-";
/// Names the alerts provider's program, so another country's plugs in as `ghost-alerts-<name>`.
const ALERTS_PROGRAM_PREFIX: &str = "ghost-alerts-";
const ALERTS_AREA_FLAG: &str = "--area";

const FIRST_RESTART_DELAY: Duration = Duration::from_secs(1);
const MAX_RESTART_DELAY: Duration = Duration::from_secs(30);
const RESTART_DELAY_GROWTH: u32 = 2;
// A provider that ran this long was healthy, so its next exit starts the backoff over.
const HEALTHY_RUN: Duration = Duration::from_secs(30);
// How long a provider being stopped may take to exit after SIGTERM before its group is killed.
const STOP_GRACE: Duration = Duration::from_secs(2);
// How long to wait for a provider to be gone after SIGKILL before giving up on reaping it.
const REAP_LIMIT: Duration = Duration::from_secs(1);

/// Keeps the configured providers running; dropping it kills them and everything they started.
pub struct Supervisor {
    socket: PathBuf,
    running: BTreeMap<String, Running>,
}

struct Running {
    spec: ProviderSpec,
    task: JoinHandle<()>,
    stop: oneshot::Sender<()>,
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        // Aborting a task drops its `ProcessGroup`, which kills the provider and its children.
        for running in self.running.values() {
            running.task.abort();
        }
    }
}

/// The configured providers, followed by `alerts-<provider>` when `location.alerts` names one.
///
/// That one runs `ghost-alerts-<provider> --area <area>`, from `own_dir` when the program is there
/// and from PATH otherwise.
pub fn effective_providers(config: &ResolvedConfig, own_dir: Option<&Path>) -> Vec<ProviderSpec> {
    let alerts = &config.location.alerts;
    let derived = (!alerts.provider.is_empty()).then(|| {
        let program = format!("{ALERTS_PROGRAM_PREFIX}{}", alerts.provider);
        ProviderSpec {
            name: format!("{ALERTS_NAME_PREFIX}{}", alerts.provider),
            exec: vec![
                installed_beside(own_dir, &program).unwrap_or(program),
                ALERTS_AREA_FLAG.to_owned(),
                alerts.area.clone(),
            ],
            restart: true,
        }
    });
    config.providers.iter().cloned().chain(derived).collect()
}

/// `program`'s path in `dir`, if it is there and the path is valid UTF-8.
fn installed_beside(dir: Option<&Path>, program: &str) -> Option<String> {
    let path = dir?.join(program);
    if !path.is_file() {
        return None;
    }
    path.into_os_string().into_string().ok()
}

/// The topics of `specs` that are known before their provider claims them: only the alerts
/// provider's, since a configured provider names its topics only once it connects.
pub fn expected_topics(specs: &[ProviderSpec]) -> Vec<String> {
    specs
        .iter()
        .filter(|spec| spec.name.starts_with(ALERTS_NAME_PREFIX))
        .map(|_| topics::WEATHER_ALERTS.to_owned())
        .collect()
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
    /// Brings the running providers in line with `specs`: starts the new ones, stops the removed
    /// ones, restarts those whose spec changed, and leaves the rest, backoff included, alone.
    ///
    /// A provider is stopped with SIGTERM to its whole process group, then SIGKILL once
    /// `STOP_GRACE` has passed, and this returns once the provider has exited, or `REAP_LIMIT`
    /// after the SIGKILL if a stuck process has not. All the stops run at once.
    pub async fn reconcile(&mut self, specs: &[ProviderSpec]) {
        let running: Vec<&ProviderSpec> = self.running.values().map(|run| &run.spec).collect();
        let plan = plan(&running, specs);
        // Every stop is requested before any is awaited, so providers that hold out for the whole
        // grace period delay this by one period between them, not one each.
        let mut stopping = Vec::new();
        for name in plan.stop {
            if let Some(stopped) = self.running.remove(&name) {
                // A task that already finished has dropped its receiver, and has nothing to stop.
                let _ = stopped.stop.send(());
                stopping.push((name, stopped.task));
            }
        }
        for (name, task) in stopping {
            // Awaited so a replacement never runs, or holds topics, alongside the process it
            // replaces.
            let _exited = task.await;
            info!(provider = %name, "provider stopped");
        }
        for spec in plan.start {
            self.start(spec.clone());
        }
    }

    fn start(&mut self, spec: ProviderSpec) {
        let (stop, stop_requested) = oneshot::channel();
        let task = tokio::spawn(keep_running(
            spec.clone(),
            self.socket.clone(),
            stop_requested,
        ));
        let name = spec.name.clone();
        if let Some(replaced) = self.running.insert(name, Running { spec, task, stop }) {
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

enum RunEnd {
    Exited,
    Stopped,
}

async fn keep_running(
    spec: ProviderSpec,
    socket: PathBuf,
    mut stop_requested: oneshot::Receiver<()>,
) {
    let mut delay = FIRST_RESTART_DELAY;
    loop {
        let started = Instant::now();
        if let RunEnd::Stopped = run_once(&spec, &socket, &mut stop_requested).await {
            return;
        }
        if !spec.restart {
            return;
        }
        if started.elapsed() >= HEALTHY_RUN {
            delay = FIRST_RESTART_DELAY;
        }
        info!(provider = %spec.name, "restarting in {delay:?}");
        tokio::select! {
            () = sleep(delay) => {}
            _ = &mut stop_requested => return,
        }
        delay = (delay * RESTART_DELAY_GROWTH).min(MAX_RESTART_DELAY);
    }
}

async fn run_once(
    spec: &ProviderSpec,
    socket: &Path,
    stop_requested: &mut oneshot::Receiver<()>,
) -> RunEnd {
    let Some((program, args)) = spec.exec.split_first() else {
        warn!(provider = %spec.name, "provider has an empty exec");
        return RunEnd::Exited;
    };
    let spawned = Command::new(program)
        .args(args)
        .env(SOCKET_ENV, socket)
        .stdin(Stdio::null())
        // A group of its own, so stopping the provider can reach what it started, such as the
        // program an `sh -c` wrapper runs, which `kill_on_drop` alone would leave behind.
        .process_group(0)
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => {
            warn!(provider = %spec.name, %error, "cannot start provider");
            return RunEnd::Exited;
        }
    };
    info!(provider = %spec.name, pid = child.id(), "provider started");
    let mut group = ProcessGroup::of(&child);
    tokio::select! {
        waited = child.wait() => {
            match waited {
                Ok(status) => warn!(provider = %spec.name, %status, "provider exited"),
                Err(error) => warn!(provider = %spec.name, %error, "lost track of provider"),
            }
            // Whatever the provider left running would keep its topics and connection after a
            // restart has started another copy, and nothing else will ever stop it. The leader is
            // reaped, but its group id stays taken while any member lives, so this reaches exactly
            // those leftovers. SIGKILL straight away, because without the leader nothing is
            // left to report that they have exited.
            group.kill();
            RunEnd::Exited
        }
        _ = stop_requested => {
            group.terminate(&mut child).await;
            RunEnd::Stopped
        }
    }
}

/// The process group a provider leads, which holds the provider and whatever it started.
///
/// Dropping it kills the group, so a task that is aborted takes the provider's children down with
/// it. Once the group has been killed on purpose it lets go, so that the drop does not signal an
/// id that may have been handed to something else by then.
struct ProcessGroup(Option<Pid>);

impl ProcessGroup {
    /// A provider is its group's leader, so the group's id is the provider's pid.
    fn of(child: &Child) -> Self {
        Self(
            child
                .id()
                .and_then(|pid| i32::try_from(pid).ok())
                .map(Pid::from_raw),
        )
    }

    /// Kills whatever is left in the group, once, and lets go of it.
    fn kill(&mut self) {
        self.signal(Signal::SIGKILL);
        self.0 = None;
    }

    fn signal(&self, signal: Signal) {
        let Some(group) = self.0 else {
            return;
        };
        match killpg(group, signal) {
            // Nothing left in the group is what stopping wanted.
            Ok(()) | Err(Errno::ESRCH) => {}
            Err(error) => warn!(%error, %signal, "cannot signal a provider's process group"),
        }
    }

    /// Asks the group to exit, kills it when `STOP_GRACE` runs out, and reaps the provider.
    async fn terminate(&mut self, child: &mut Child) {
        self.signal(Signal::SIGTERM);
        // Only the leader is waited for, but the kill below follows either way, so a child that
        // outlived it does not outlive the stop.
        let _ = timeout(STOP_GRACE, child.wait()).await;
        self.kill();
        // Bounded, because a process stuck in uninterruptible sleep outlasts SIGKILL, and
        // waiting for it would hold up every config and widget change behind this stop.
        match timeout(REAP_LIMIT, child.kill()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!(%error, "lost track of a stopped provider"),
            Err(_) => warn!("a stopped provider has not exited after SIGKILL"),
        }
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        self.signal(Signal::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use ghost_proto::Alerts;

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

    fn config_with_alerts(provider: &str, area: &str) -> ResolvedConfig {
        let mut config = ResolvedConfig::default();
        config.location.alerts = Alerts {
            provider: provider.to_owned(),
            area: area.to_owned(),
        };
        config.providers = vec![spec("other", "other-bin")];
        config
    }

    #[test]
    fn alerts_provider_is_derived_from_location() {
        let specs = effective_providers(&config_with_alerts("inmet", "3550308"), None);

        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0], spec("other", "other-bin"));
        assert_eq!(
            specs[1],
            ProviderSpec {
                name: "alerts-inmet".to_owned(),
                exec: vec![
                    "ghost-alerts-inmet".to_owned(),
                    "--area".to_owned(),
                    "3550308".to_owned()
                ],
                restart: true,
            }
        );
        assert_eq!(
            effective_providers(&config_with_alerts("", ""), None),
            [spec("other", "other-bin")]
        );
    }

    #[test]
    fn the_alerts_provider_runs_from_the_daemons_dir_when_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with_alerts("inmet", "3550308");
        let program =
            |own_dir: Option<&Path>| effective_providers(&config, own_dir)[1].exec[0].clone();
        assert_eq!(program(Some(dir.path())), "ghost-alerts-inmet");

        let installed = dir.path().join("ghost-alerts-inmet");
        std::fs::write(&installed, "").unwrap();

        assert_eq!(program(Some(dir.path())), installed.to_str().unwrap());
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

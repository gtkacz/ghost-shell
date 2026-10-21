use std::collections::BTreeMap;
use std::convert::Infallible;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ghost_proto::{LayoutConfig, ResolvedConfig, ResolvedLayout, ServerMsg};
use tokio::sync::{mpsc, watch};
use tokio::task::spawn_blocking;
use tracing::{error, info, warn};

use super::state::{LayoutLines, State};
use super::widget_watch::WidgetWatch;
use super::{ProviderSpecs, Shared, WarningHook, WidgetDirs, line, problem_line};
use crate::config::{ConfigError, WatchHandle, load_path, load_str, spawn_watcher};
use crate::providers::{Supervisor, expected_topics, supervise};
use crate::widgets::{discover, resolve};

const CONFIG_WARNING_SUMMARY: &str = "ghost-shell config";

/// The live config: what is in effect now, why a reload was refused, and what keeps both coming.
pub struct ConfigFeed {
    pub(super) updates: watch::Receiver<ResolvedConfig>,
    errors: mpsc::UnboundedReceiver<ConfigError>,
    pub(super) startup_error: Option<ConfigError>,
    _source: FeedSource,
}

/// Held only so that dropping the feed stops the watcher, or closes the channels it stands in for.
enum FeedSource {
    Watcher {
        _handle: WatchHandle,
    },
    // Holding the senders keeps both channels open and quiet when the file cannot be watched.
    Fixed {
        _updates: watch::Sender<ResolvedConfig>,
        _errors: mpsc::UnboundedSender<ConfigError>,
    },
}

impl ConfigFeed {
    /// Loads `path` and watches it for changes, degrading rather than failing: an unreadable file
    /// gives the defaults (reported as a config error), and a file that cannot be watched is
    /// served without live reload.
    ///
    /// Must be called from within a Tokio runtime with the time driver enabled.
    pub fn open(path: &Path) -> Self {
        let (initial, startup_error) = match load_path(path) {
            Ok(config) => (config, None),
            Err(error) => {
                warn!(%error, "config not loaded; starting from the defaults");
                (load_str("").unwrap_or_default(), Some(error))
            }
        };
        // The watcher watches the file's directory, which must exist before anything is saved.
        if let Some(dir) = path.parent()
            && let Err(error) = fs::create_dir_all(dir)
        {
            warn!(%error, dir = %dir.display(), "cannot create the config directory");
        }
        match spawn_watcher(path.to_owned(), initial.clone()) {
            Ok((handle, updates, errors)) => Self {
                updates,
                errors,
                startup_error,
                _source: FeedSource::Watcher { _handle: handle },
            },
            Err(error) => {
                warn!(%error, "config live reload is off");
                let (update_tx, updates) = watch::channel(initial);
                let (error_tx, errors) = mpsc::unbounded_channel();
                Self {
                    updates,
                    errors,
                    startup_error,
                    _source: FeedSource::Fixed {
                        _updates: update_tx,
                        _errors: error_tx,
                    },
                }
            }
        }
    }

    /// The config in effect now.
    pub fn current(&self) -> ResolvedConfig {
        self.updates.borrow().clone()
    }

    /// A receiver that sees every config from now on.
    pub fn subscribe(&self) -> watch::Receiver<ResolvedConfig> {
        self.updates.clone()
    }
}

/// Turns config loads, refusals and widget changes into messages for UI clients and warnings for
/// the user, and keeps the providers in line with the config.
pub(super) struct Reloader {
    shared: Shared,
    widget_dirs: WidgetDirs,
    widgets: WidgetWatch,
    provider_specs: ProviderSpecs,
    providers: Supervisor,
    config: ResolvedConfig,
    dirs: Vec<PathBuf>,
    config_error: Option<String>,
    layout_errors: Vec<String>,
    reporter: ProblemReporter,
}

impl Reloader {
    /// Starts watching the widget dirs, then resolves the first layout, builds the state every
    /// UI client is greeted from, and starts the providers with `GHOST_SOCKET=<socket>`.
    pub(super) async fn start(
        config: ResolvedConfig,
        widget_dirs: WidgetDirs,
        provider_specs: ProviderSpecs,
        socket: &Path,
        reporter: ProblemReporter,
        host: String,
        step_ms: u32,
    ) -> Self {
        let dirs = widget_dirs(&config);
        let mut widgets = WidgetWatch::new();
        // Watched before discovery, so a change made while the widgets are being read is reported
        // and rescanned instead of falling between the two. A dir that does not exist yet is the
        // exception: it is only picked up by the next config reload.
        widgets.watch(dirs.clone()).await;
        let layout = resolve_layout(dirs.clone(), &config.layout).await;
        let layout_errors = layout.errors.clone();
        let shared = Shared::new(State::new(
            host,
            step_ms,
            config_line(&config),
            layout_lines(layout),
        ));
        let specs = provider_specs(&config);
        // Before the first start, so no provider can claim a topic before it is expected.
        shared.lock().expect(&expected_topics(&specs));
        let providers = supervise(&specs, socket);
        Self {
            shared,
            widget_dirs,
            widgets,
            provider_specs,
            providers,
            config,
            dirs,
            config_error: None,
            layout_errors,
            reporter,
        }
    }

    pub(super) fn shared(&self) -> Shared {
        self.shared.clone()
    }

    async fn apply(&mut self, config: ResolvedConfig) {
        self.dirs = (self.widget_dirs)(&config);
        // Rebuilt on every reload, because the dir list may have changed or a missing dir may have
        // been created since, and before discovery for the same reason as at start.
        self.widgets.watch(self.dirs.clone()).await;
        let layout = resolve_layout(self.dirs.clone(), &config.layout).await;
        self.config_error = None;
        self.layout_errors = layout.errors.clone();
        self.shared
            .lock()
            .set_config(config_line(&config), layout_lines(layout));
        let specs = (self.provider_specs)(&config);
        self.shared.lock().expect(&expected_topics(&specs));
        self.providers.reconcile(&specs).await;
        self.config = config;
    }

    async fn rescan_widgets(&mut self) {
        let layout = resolve_layout(self.dirs.clone(), &self.config.layout).await;
        self.layout_errors = layout.errors.clone();
        self.shared.lock().set_layout(layout_lines(layout));
    }

    pub(super) fn refuse(&mut self, error: ConfigError) {
        self.shared
            .lock()
            .set_config_error(problem_line(&error.message));
        self.config_error = Some(error.message);
    }

    pub(super) fn report(&mut self) {
        let problems = self
            .config_error
            .iter()
            .chain(&self.layout_errors)
            .cloned()
            .collect();
        self.reporter.report(problems);
    }
}

/// Raises a warning whenever a problem appears that the previous report did not have.
pub(super) struct ProblemReporter {
    on_warning: WarningHook,
    reported: Vec<String>,
}

impl ProblemReporter {
    pub(super) fn new(on_warning: WarningHook) -> Self {
        Self {
            on_warning,
            reported: Vec::new(),
        }
    }

    /// Warns with every current problem in the body, but only when one of them is new: a reload
    /// that repeats known problems, or only fixes some, stays quiet.
    fn report(&mut self, problems: Vec<String>) {
        let fresh: Vec<&String> = problems
            .iter()
            .filter(|problem| !self.reported.contains(problem))
            .collect();
        for problem in &fresh {
            warn!(problem = %problem, "config problem");
        }
        if !fresh.is_empty() {
            (self.on_warning)(CONFIG_WARNING_SUMMARY, &problems.join("\n"));
        }
        self.reported = problems;
    }
}

fn config_line(config: &ResolvedConfig) -> Arc<str> {
    line(&ServerMsg::Config {
        data: config.clone(),
    })
}

fn layout_lines(layout: ResolvedLayout) -> LayoutLines {
    LayoutLines {
        errors: layout
            .errors
            .iter()
            .map(|error| problem_line(error))
            .collect(),
        layout: line(&ServerMsg::Layout { data: layout }),
    }
}

/// Discovers the widgets in `dirs` anew and checks `layout` against them, on a blocking thread
/// because the dirs are user-configured and may sit on a slow mount.
async fn resolve_layout(dirs: Vec<PathBuf>, layout: &LayoutConfig) -> ResolvedLayout {
    let wanted = layout.clone();
    let resolved = spawn_blocking(move || {
        let (manifests, problems) = discover(&dirs);
        let mut resolved = resolve(&wanted, &manifests);
        resolved.errors.extend(problems);
        resolved
    })
    .await;
    resolved.unwrap_or_else(|error| {
        error!(%error, "widget discovery failed");
        ResolvedLayout {
            gutter: layout.gutter,
            margin: layout.margin,
            views: BTreeMap::new(),
            errors: vec![format!("widget discovery failed: {error}")],
        }
    })
}

pub(super) async fn follow_config(mut reloader: Reloader, mut feed: ConfigFeed) -> Infallible {
    loop {
        tokio::select! {
            Ok(()) = feed.updates.changed() => {
                let config = feed.updates.borrow_and_update().clone();
                info!("config reloaded");
                reloader.apply(config).await;
            }
            Some(error) = feed.errors.recv() => {
                warn!(%error, "config not applied; keeping the last good one");
                reloader.refuse(error);
            }
            () = reloader.widgets.changed() => {
                reloader.widgets.settle().await;
                info!("widgets changed");
                reloader.rescan_widgets().await;
            }
        }
        reloader.report();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc as std_mpsc;

    use super::*;

    fn recording_reporter() -> (ProblemReporter, std_mpsc::Receiver<String>) {
        let (sent, received) = std_mpsc::channel();
        let reporter = ProblemReporter::new(Box::new(move |summary, body| {
            assert_eq!(summary, CONFIG_WARNING_SUMMARY);
            sent.send(body.to_owned()).unwrap();
        }));
        (reporter, received)
    }

    fn problems(list: &[&str]) -> Vec<String> {
        list.iter().map(|&problem| problem.to_owned()).collect()
    }

    #[test]
    fn a_problem_set_warns_once_with_every_problem_in_the_body() {
        let (mut reporter, warnings) = recording_reporter();

        reporter.report(problems(&["bad toml", "layout.v1: missing"]));
        reporter.report(problems(&["bad toml", "layout.v1: missing"]));

        assert_eq!(
            warnings.try_iter().collect::<Vec<_>>(),
            ["bad toml\nlayout.v1: missing"]
        );
    }

    #[test]
    fn fixing_a_problem_is_quiet_and_a_relapse_warns_again() {
        let (mut reporter, warnings) = recording_reporter();
        reporter.report(problems(&["bad toml", "layout.v1: missing"]));

        reporter.report(problems(&["layout.v1: missing"]));
        reporter.report(problems(&["bad toml", "layout.v1: missing"]));

        assert_eq!(warnings.try_iter().count(), 2);
    }

    #[test]
    fn no_problems_never_warn() {
        let (mut reporter, warnings) = recording_reporter();

        reporter.report(Vec::new());

        assert_eq!(warnings.try_iter().count(), 0);
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use ghost_proto::{ProviderSpec, ResolvedConfig, ResolvedLayout, ServerMsg};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use super::state::ConfigLines;
use super::{Shared, WarningHook, line, problem_line};
use crate::config::{ConfigError, WatchHandle, load_path, load_str, spawn_watcher};
use crate::widgets::{default_dirs, discover, resolve};

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

/// Turns config loads and refusals into messages for UI clients and warnings for the user.
pub(super) struct Reloader {
    shared: Shared,
    widgets_dir: Option<PathBuf>,
    providers: Vec<ProviderSpec>,
    config_error: Option<String>,
    layout_errors: Vec<String>,
    reporter: ProblemReporter,
}

impl Reloader {
    pub(super) fn new(
        shared: Shared,
        widgets_dir: Option<PathBuf>,
        initial: &ResolvedConfig,
        layout_errors: Vec<String>,
        reporter: ProblemReporter,
    ) -> Self {
        Self {
            shared,
            widgets_dir,
            providers: initial.providers.clone(),
            config_error: None,
            layout_errors,
            reporter,
        }
    }

    fn apply(&mut self, config: &ResolvedConfig) {
        if config.providers != self.providers {
            warn!("provider changes take effect when ghostd restarts");
            self.providers = config.providers.clone();
        }
        let layout = resolve_layout(config, self.widgets_dir.as_deref());
        self.config_error = None;
        self.layout_errors = layout.errors.clone();
        self.shared.lock().set_config(config_lines(config, layout));
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

pub(super) fn config_lines(config: &ResolvedConfig, layout: ResolvedLayout) -> ConfigLines {
    ConfigLines {
        config: line(&ServerMsg::Config {
            data: config.clone(),
        }),
        layout_errors: layout
            .errors
            .iter()
            .map(|error| problem_line(error))
            .collect(),
        layout: line(&ServerMsg::Layout { data: layout }),
    }
}

/// Discovers the widgets anew, so a widget installed since the last load is picked up.
pub(super) fn resolve_layout(
    config: &ResolvedConfig,
    widgets_dir: Option<&Path>,
) -> ResolvedLayout {
    let mut dirs = default_dirs(&config.widgets.paths);
    if let (Some(builtin), Some(replacement)) = (dirs.first_mut(), widgets_dir) {
        *builtin = replacement.to_owned();
    }
    let (manifests, problems) = discover(&dirs);
    let mut layout = resolve(&config.layout, &manifests);
    layout.errors.extend(problems);
    layout
}

pub(super) async fn follow_config(mut reloader: Reloader, mut feed: ConfigFeed) {
    loop {
        tokio::select! {
            Ok(()) = feed.updates.changed() => {
                let config = feed.updates.borrow_and_update().clone();
                info!("config reloaded");
                reloader.apply(&config);
            }
            Some(error) = feed.errors.recv() => {
                warn!(%error, "config not applied; keeping the last good one");
                reloader.refuse(error);
            }
            else => return,
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

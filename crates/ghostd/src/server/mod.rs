//! The Unix-socket server.
//!
//! A connection's first line decides its role. A UI client (`hello`) gets the config and layout,
//! then the topics it subscribes to. A provider (`provide`) publishes topics of its own, which are
//! relayed to subscribers and cached so a late subscriber still gets the last value.

mod feed;
mod sampling;
mod session;
mod state;
mod widget_watch;

use std::num::NonZeroU32;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::{fs, io};

use ghost_proto::{ResolvedConfig, ServerMsg, encode_line};
use nix::unistd::gethostname;
use tokio::net::UnixListener;

pub use feed::ConfigFeed;
use feed::{ProblemReporter, Reloader, follow_config};
use sampling::{Collectors, sample_forever};
use session::accept_forever;
use state::State;

use crate::providers::Supervisor;
use crate::sampler::Sampler;

const OWNER_ONLY_MODE: u32 = 0o600;

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("another ghostd is already listening on {}", .0.display())]
    InUse(PathBuf),
    #[error("{} exists and is not a socket", .0.display())]
    NotASocket(PathBuf),
    #[error("cannot listen on {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// Listens on `path` with a socket only its owner can use, replacing a socket file that nothing
/// answers on any more.
///
/// Must be called from within a Tokio runtime.
pub fn bind(path: &Path) -> Result<UnixListener, BindError> {
    if StdUnixStream::connect(path).is_ok() {
        return Err(BindError::InUse(path.to_owned()));
    }
    let io_error = |source| BindError::Io {
        path: path.to_owned(),
        source,
    };
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => {
            fs::remove_file(path).map_err(io_error)?;
        }
        Ok(_) => return Err(BindError::NotASocket(path.to_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(error)),
    }
    let listener = UnixListener::bind(path).map_err(io_error)?;
    // Narrowed after bind rather than through the umask, which is process-wide and would race
    // with files other threads create. Until then the umask's default (022) already keeps other
    // users out, since connecting needs write permission on the socket.
    fs::set_permissions(path, fs::Permissions::from_mode(OWNER_ONLY_MODE)).map_err(io_error)?;
    Ok(listener)
}

/// Called with a summary and a body whenever a config or layout problem appears that was not
/// reported before.
pub type WarningHook = Box<dyn Fn(&str, &str) + Send>;

/// The widget search dirs for a config, in discovery order.
pub type WidgetDirs = Box<dyn Fn(&ResolvedConfig) -> Vec<PathBuf> + Send>;

/// Everything [`serve`] runs.
pub struct Hub {
    pub config: ConfigFeed,
    pub sampler: Sampler,
    pub tick_ms: NonZeroU32,
    pub widget_dirs: WidgetDirs,
    /// Reconciled with `providers` on every config reload.
    pub providers: Supervisor,
    pub on_warning: WarningHook,
}

/// Serves UI clients and providers on `listener`, samples the built-in topics every tick and
/// follows the config, until the returned future is dropped.
pub async fn serve(listener: UnixListener, hub: Hub) {
    let Hub {
        mut config,
        sampler,
        tick_ms,
        widget_dirs,
        providers,
        on_warning,
    } = hub;
    let initial = config.updates.borrow_and_update().clone();
    let collectors = Collectors::new(sampler, &initial, tick_ms);
    let mut reloader = Reloader::start(
        initial,
        widget_dirs,
        providers,
        ProblemReporter::new(on_warning),
        host_name(),
        tick_ms.get(),
    )
    .await;
    if let Some(error) = config.startup_error.take() {
        reloader.refuse(error);
    }
    reloader.report();
    let shared = reloader.shared();

    tokio::join!(
        accept_forever(listener, shared.clone()),
        sample_forever(shared, collectors, tick_ms, config.subscribe()),
        follow_config(reloader, config),
    );
}

fn host_name() -> String {
    gethostname()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn line(message: &ServerMsg) -> Arc<str> {
    encode_line(message).into()
}

fn problem_line(message: &str) -> Arc<str> {
    line(&ServerMsg::ConfigError {
        message: message.to_owned(),
    })
}

fn invalid_topic(topic: &str) -> String {
    format!("invalid topic name {topic:?}")
}

#[derive(Clone)]
struct Shared(Arc<Mutex<State>>);

impl Shared {
    fn new(state: State) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Every update leaves the state usable, so a panic elsewhere must not stop the server.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

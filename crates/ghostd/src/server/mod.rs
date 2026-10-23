//! The Unix-socket server.
//!
//! A connection's first line decides its role. A UI client (`hello`) gets the config and layout,
//! then the topics it subscribes to. A provider (`provide`) publishes topics of its own, which are
//! relayed to subscribers and cached so a late subscriber still gets the last value.

mod feed;
mod publish;
mod sampling;
mod session;
mod state;
mod widget_watch;

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::num::NonZeroU32;
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream as StdUnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::{fs, io};

use ghost_proto::{ProviderSpec, ResolvedConfig, ServerMsg, encode_line};
use nix::errno::Errno;
use nix::fcntl::{Flock, FlockArg};
use nix::unistd::gethostname;
use tokio::net::UnixListener;
use tokio::task::{self, JoinError, JoinSet};

pub use feed::ConfigFeed;
use feed::{ProblemReporter, Reloader, follow_config};
pub use publish::{Call, Demand, Feed, FeedContext, Publisher};
use sampling::{Collectors, sample_forever};
use session::accept_forever;
use state::State;

use crate::sampler::Sampler;

const OWNER_ONLY_MODE: u32 = 0o600;
const LOCK_SUFFIX: &str = ".lock";

#[derive(Debug, thiserror::Error)]
pub enum BindError {
    #[error("another ghostd is already listening on {}", .0.display())]
    InUse(PathBuf),
    #[error("{} exists and is not a socket", .0.display())]
    NotASocket(PathBuf),
    #[error("cannot listen on {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// The lock that keeps a second daemon from claiming a socket path; held for as long as this is.
pub struct SocketLock {
    _flock: Flock<File>,
}

/// Listens on `path` with a socket only its owner can use, replacing a socket file that nothing
/// answers on any more.
///
/// Also returns the lock on `<path>.lock`, which must be kept for as long as the socket is in use.
/// It is taken before the path is looked at, because connecting to see whether another daemon
/// answers cannot tell a daemon that is still starting from none at all, and two daemons started
/// together could both pass that check. The lock file itself is left in place when the daemon
/// stops, since removing it would let a daemon that already opened it lock a file nobody else sees.
///
/// Must be called from within a Tokio runtime.
pub fn bind(path: &Path) -> Result<(UnixListener, SocketLock), BindError> {
    let lock = lock_beside(path)?;
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
    Ok((listener, lock))
}

fn lock_beside(socket: &Path) -> Result<SocketLock, BindError> {
    let mut lock_path = socket.as_os_str().to_owned();
    lock_path.push(LOCK_SUFFIX);
    let lock_path = PathBuf::from(lock_path);
    let io_error = |source| BindError::Io {
        path: lock_path.clone(),
        source,
    };
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(OWNER_ONLY_MODE)
        .open(&lock_path)
        .map_err(io_error)?;
    match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(flock) => Ok(SocketLock { _flock: flock }),
        Err((_, Errno::EWOULDBLOCK)) => Err(BindError::InUse(socket.to_owned())),
        Err((_, errno)) => Err(io_error(errno.into())),
    }
}

/// Called with a summary and a body whenever a config or layout problem appears that was not
/// reported before.
pub type WarningHook = Box<dyn Fn(&str, &str) + Send>;

/// The widget search dirs for a config, in discovery order.
pub type WidgetDirs = Box<dyn Fn(&ResolvedConfig) -> Vec<PathBuf> + Send>;

/// The providers to keep running for a config.
pub type ProviderSpecs = Box<dyn Fn(&ResolvedConfig) -> Vec<ProviderSpec> + Send>;

/// Everything [`serve`] runs.
pub struct Hub {
    pub config: ConfigFeed,
    pub sampler: Sampler,
    pub tick_ms: NonZeroU32,
    pub widget_dirs: WidgetDirs,
    /// Started once the hub's state exists, and reconciled on every config reload.
    pub providers: ProviderSpecs,
    /// The path the listener is bound to, which every provider is told to connect to.
    pub socket: PathBuf,
    pub on_warning: WarningHook,
    /// Each is started once and must run for as long as the daemon does.
    pub feeds: Vec<Feed>,
}

/// Why [`serve`] stopped while nothing asked it to.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    // Clients would still connect and look served, but every built-in topic would stay frozen.
    #[error("sampling stopped: {0}")]
    Sampling(#[from] JoinError),
    // Feeds never end by design, so one that did has left its topics frozen just the same.
    #[error("feed {name:?} stopped: {end}")]
    Feed { name: &'static str, end: FeedEnd },
}

/// How a feed ended.
#[derive(Debug, thiserror::Error)]
pub enum FeedEnd {
    #[error("it returned")]
    Returned,
    #[error("{0}")]
    Failed(JoinError),
}

/// Serves UI clients and providers on `listener`, samples the subscribed built-in topics every
/// tick, runs the feeds and follows the config, until the returned future is dropped or the
/// sampling task or a feed stops.
///
/// A stopped sampling task or feed is returned rather than carried on without, so that the daemon
/// can exit and be restarted instead of looking healthy with topics frozen.
pub async fn serve(listener: UnixListener, hub: Hub) -> ServeError {
    let Hub {
        mut config,
        sampler,
        tick_ms,
        widget_dirs,
        providers,
        socket,
        on_warning,
        feeds,
    } = hub;
    let initial = config.updates.borrow_and_update().clone();
    let collectors = Collectors::new(sampler, &initial, tick_ms);
    let mut reloader = Reloader::start(
        initial,
        widget_dirs,
        providers,
        &socket,
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
    let updates = config.subscribe();
    let demand = shared.lock().demand();
    let mut running_feeds = RunningFeeds::start(feeds, &shared, &config);

    let clients = {
        let shared = shared.clone();
        async move {
            let (never, _) = tokio::join!(
                accept_forever(listener, shared),
                follow_config(reloader, config),
            );
            never
        }
    };
    tokio::select! {
        error = sample_forever(shared, collectors, tick_ms, updates, demand) => error,
        Some(error) = running_feeds.ended() => error,
        never = clients => match never {},
    }
}

/// The feeds' tasks, which are aborted when this is dropped, and the feed each one runs.
struct RunningFeeds {
    tasks: JoinSet<()>,
    names: HashMap<task::Id, &'static str>,
}

impl RunningFeeds {
    /// Spawns every feed with a publisher into the hub.
    fn start(feeds: Vec<Feed>, shared: &Shared, config: &ConfigFeed) -> Self {
        let publisher = {
            let shared = shared.clone();
            Publisher::new(move |topic, data| shared.lock().publish(topic, data))
        };
        let mut tasks = JoinSet::new();
        let mut names = HashMap::new();
        for feed in feeds {
            let name = feed.name();
            let (demand, calls) = {
                let state = shared.lock();
                (state.demand(), state.calls())
            };
            let running = tasks.spawn(feed.run(FeedContext {
                publisher: publisher.clone(),
                demand,
                config: config.subscribe(),
                calls,
            }));
            names.insert(running.id(), name);
        }
        Self { tasks, names }
    }

    /// Waits for a feed to end, or returns `None` at once when there are none.
    async fn ended(&mut self) -> Option<ServeError> {
        let (id, end) = match self.tasks.join_next_with_id().await? {
            Ok((id, ())) => (id, FeedEnd::Returned),
            Err(error) => (error.id(), FeedEnd::Failed(error)),
        };
        Some(ServeError::Feed {
            name: self.names[&id],
            end,
        })
    }
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

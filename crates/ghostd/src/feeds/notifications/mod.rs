//! The `notifications` topic: the history of what apps sent through `org.freedesktop.Notifications`,
//! the shell's calls on it, and the decision whether ghostd serves the name.
//!
//! The name is requested only when the marker file exists at start. Without it, Plasma (or nobody)
//! serves, and the feed only reports which. The topic is published at start and after every
//! change, wanted or not: the bell and the gate need it from the first moment.
//!
//! The loop never waits on the disk or on a bus question: files are written by a task of their
//! own, and a `Notify` is answered before anything else is done for it.

pub mod history;
pub mod images;
pub mod inhibit;
pub mod markup;
pub mod name;
pub mod server;
mod sources;
mod state;
mod store;

use std::future::pending;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, future};

use ghost_proto::topics;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep_until};
use tracing::{debug, info, warn};
use zbus::Connection;

pub use name::Buses;

use self::name::NameWatch;
use self::server::{Claim, Request};
use self::sources::{Sources, read_boot_id};
use self::state::{Clock, Outputs, State};
use crate::sdnotify;
use crate::server::{Feed, FeedContext};

const STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const RUNTIME_DIR_ENV: &str = "XDG_RUNTIME_DIR";
const HOME_ENV: &str = "HOME";
const STATE_HOME_SUBDIR: &str = ".local/state";
const APP_DIR: &str = "ghost-shell";
const MARKER_FILE: &str = "notifications-server";
const HISTORY_FILE: &str = "notifications.json";
const IMAGES_DIR: &str = "notify";
const BOOT_ID_FILE: &str = "/proc/sys/kernel/random/boot_id";
/// Requests wait here for the loop; the server's dispatcher waits on each one's answer.
const INBOX: usize = 16;

pub struct Paths {
    /// Its presence at start makes ghostd request the name.
    pub marker: PathBuf,
    pub history: PathBuf,
    /// Empty when there is no runtime dir, which turns pictures off.
    pub images: PathBuf,
    /// Where the kernel names this boot, which keeps a drive that died in an earlier one from
    /// silencing this one.
    pub boot_id: PathBuf,
}

pub fn default_paths() -> Paths {
    let state = env::var_os(STATE_HOME_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os(HOME_ENV).unwrap_or_default()).join(STATE_HOME_SUBDIR)
        })
        .join(APP_DIR);
    let images = env::var_os(RUNTIME_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join(APP_DIR).join(IMAGES_DIR))
        .unwrap_or_default();
    Paths {
        marker: state.join(MARKER_FILE),
        history: state.join(HISTORY_FILE),
        images,
        boot_id: PathBuf::from(BOOT_ID_FILE),
    }
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

/// The feed on the session bus, which is connected to twice once the daemon starts: the second
/// connection asks the bus the questions the first one's handlers must not.
pub fn feed(paths: Paths, signals: watch::Sender<Option<u32>>) -> Feed {
    Feed::new(topics::NOTIFICATIONS, |context| async move {
        let buses = connect().await;
        run(
            buses,
            paths,
            signals,
            Box::new(unix_now_ms),
            sdnotify::ready,
            context,
        )
        .await;
    })
}

/// As [`feed`], on `buses`, with the clock (milliseconds since the epoch) and the readiness
/// notification passed in.
pub fn feed_on(
    buses: Option<Buses>,
    paths: Paths,
    signals: watch::Sender<Option<u32>>,
    now: impl Fn() -> u64 + Send + 'static,
    ready: impl FnOnce() + Send + 'static,
) -> Feed {
    Feed::new(topics::NOTIFICATIONS, move |context| {
        run(buses, paths, signals, Box::new(now), ready, context)
    })
}

async fn connect() -> Option<Buses> {
    let session = || async {
        Connection::session().await.inspect_err(|error| {
            warn!(%error, "cannot connect to the session bus; notifications have no server and their calls only change the history");
        })
    };
    Some(Buses {
        bus: session().await.ok()?,
        helper: session().await.ok()?,
    })
}

/// Registers the server and asks for the name. Returns whether inhibitions are held, or `None`
/// when the server could not be registered.
async fn claim_name(
    buses: &Buses,
    names: Option<&NameWatch>,
    inbox: mpsc::Sender<Request>,
) -> Option<watch::Receiver<bool>> {
    let inhibited = server::serve(&buses.bus, &buses.helper, inbox)
        .await
        .inspect_err(|error| warn!(%error, "cannot register the notification server"))
        .ok()?;
    match server::claim(&buses.bus).await {
        Ok(Claim::Owned) => info!("notifications: owning {}", server::NAME),
        Ok(Claim::Queued { owner }) => {
            let pid = match names {
                Some(names) => names.pid_of(&owner).await,
                None => 0,
            };
            info!("notifications: queued behind {pid}");
        }
        Ok(Claim::NotRequested) => {}
        Err(error) => warn!(%error, "cannot request {}", server::NAME),
    }
    Some(inhibited)
}

async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => sleep_until(at).await,
        None => pending().await,
    }
}

/// The next change of the inhibitions held; `None` once the server is gone.
async fn inhibition_change(inhibited: &mut Option<watch::Receiver<bool>>) -> Option<bool> {
    let Some(receiver) = inhibited else {
        return future::pending().await;
    };
    receiver.changed().await.ok()?;
    let held = *receiver.borrow_and_update();
    Some(held)
}

async fn name_change(names: &mut Option<NameWatch>) {
    match names {
        Some(names) => names.changed().await,
        None => pending().await,
    }
}

async fn run(
    buses: Option<Buses>,
    paths: Paths,
    signals: watch::Sender<Option<u32>>,
    now: Clock,
    ready: impl FnOnce(),
    context: FeedContext,
) {
    let FeedContext {
        publisher,
        config,
        mut calls,
        mut topics,
        ..
    } = context;
    let images = !paths.images.as_os_str().is_empty();
    if !images {
        warn!("{RUNTIME_DIR_ENV} is not set; notification images are skipped");
    }
    let (written_sender, mut written) = mpsc::unbounded_channel();
    let writer = store::spawn_writer(paths.history.clone(), paths.images.clone(), written_sender);
    let (sent_sender, mut sent) = mpsc::unbounded_channel();
    let sources = Sources::new(
        buses.as_ref().map(|buses| buses.helper.clone()),
        read_boot_id(&paths.boot_id),
        sent_sender,
    );
    let mut state = State::new(
        store::load(&paths.history),
        sources,
        buses.as_ref().map(|buses| buses.bus.clone()),
        writer,
        images,
        Outputs { publisher, signals },
        now,
    );
    // Held for as long as the loop runs, so that a feed that serves nothing waits on it for ever
    // instead of seeing it closed.
    let (inbox_sender, mut inbox) = mpsc::channel(INBOX);
    let mut names = None;
    let mut inhibition = None;
    if let Some(buses) = &buses {
        names = NameWatch::open(buses)
            .await
            .inspect_err(|error| warn!(%error, "cannot watch who serves notifications"))
            .ok();
        if paths.marker.exists() {
            inhibition = claim_name(buses, names.as_ref(), inbox_sender.clone()).await;
        } else {
            info!("notifications: plasma serving (no marker)");
        }
        if let Some(names) = &names {
            state.set_server(names.server().await);
        }
    }
    state.publish();
    // Whatever happened above, systemd is waiting for this: a missed one gets ghostd killed.
    ready();

    let (mut calls_open, mut topics_open) = (true, true);
    loop {
        tokio::select! {
            Some(request) = inbox.recv() => {
                let settings = config.borrow().notifications.clone();
                state.request(request, &settings).await;
            }
            call = calls.recv(), if calls_open => match call {
                Ok(call) => state.call(call).await,
                Err(RecvError::Lagged(missed)) => {
                    warn!(missed, "notification calls were dropped: the feed fell behind");
                }
                Err(RecvError::Closed) => calls_open = false,
            },
            update = topics.recv(), if topics_open => match update {
                Ok(update) => state.topic(&update),
                Err(RecvError::Lagged(missed)) => {
                    debug!(missed, "topic updates lost; keeping the last fullscreen and locked state");
                }
                Err(RecvError::Closed) => topics_open = false,
            },
            change = inhibition_change(&mut inhibition) => match change {
                Some(held) => state.set_inhibited(held),
                None => inhibition = None,
            },
            () = name_change(&mut names) => {
                if let Some(names) = &names {
                    state.set_server(names.server().await);
                }
            }
            Some(done) = written.recv() => state.image_written(done),
            Some(answer) = sent.recv() => state.sent(answer),
            () = until(state.next_expiry()) => state.expire(),
            () = until(state.save_deadline()) => state.save(),
            else => pending::<()>().await,
        }
    }
}

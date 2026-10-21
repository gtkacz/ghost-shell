//! The `desktop` topic: which outputs windows cover, whether the desktop has focus, and the view
//! actions the shortcuts ask for. KWin's desktop script reports the first two over D-Bus.

use std::collections::BTreeSet;
use std::future::pending;
use std::sync::{Arc, Mutex, PoisonError};

use ghost_proto::{DesktopSnapshot, ViewAction, topics};
use tokio::sync::watch;
use tracing::warn;
use zbus::message::Header;
use zbus::names::{BusName, OwnedUniqueName};
use zbus::proxy::CacheProperties;
use zbus::{Connection, connection, fdo, interface};

use super::next;
use super::shortcuts::{self, Registration};
use crate::server::{Feed, Publisher};

const BUS_NAME: &str = "org.ghostshell.Ghostd";
const OBJECT_PATH: &str = "/org/ghostshell/Desktop";
const OUTPUT_SEPARATOR: char = ',';
/// The names `Activate` takes, which are also the ids the shortcuts are registered under, so a
/// pressed shortcut can be passed on as it is.
pub(crate) const MONITOR: &str = "monitor";
pub(crate) const SWAP: &str = "swap";

#[derive(Debug, Default)]
pub struct DesktopState {
    snapshot: DesktopSnapshot,
}

impl DesktopState {
    /// Returns whether anything changed. The names are trimmed, sorted and deduplicated first, so
    /// the same outputs listed in another order are no change.
    pub fn report(&mut self, covered_outputs: Vec<String>, active: bool) -> bool {
        let covered_outputs: Vec<String> = covered_outputs
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let changed =
            covered_outputs != self.snapshot.covered_outputs || active != self.snapshot.active;
        self.snapshot.covered_outputs = covered_outputs;
        self.snapshot.active = active;
        changed
    }

    /// Takes `action` only while the desktop has focus, so a shortcut pressed over a window does
    /// nothing. Returns whether it was taken.
    pub fn activate(&mut self, action: ViewAction) -> bool {
        if !self.snapshot.active {
            return false;
        }
        self.snapshot.action = Some(action);
        self.snapshot.action_seq += 1;
        true
    }

    pub fn snapshot(&self) -> DesktopSnapshot {
        self.snapshot.clone()
    }
}

/// Serves `org.ghostshell.Desktop1` and publishes `desktop` on every change.
#[derive(Clone)]
pub struct DesktopService {
    state: Arc<Mutex<DesktopState>>,
    /// The bus connection that sent the last report; changed only while `state` is locked.
    reporter: Arc<watch::Sender<Option<OwnedUniqueName>>>,
    publisher: Publisher,
}

impl DesktopService {
    pub fn new(publisher: Publisher) -> Self {
        Self {
            state: Arc::default(),
            reporter: Arc::new(watch::Sender::new(None)),
            publisher,
        }
    }

    fn update(&self, change: impl FnOnce(&mut DesktopState) -> bool) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if change(&mut state) {
            // Published under the lock, so two quick changes can never reach clients swapped.
            self.publisher.publish(topics::DESKTOP, &state.snapshot());
        }
    }

    /// Goes back to no covered outputs and no focus if `reporter` sent the last report. The
    /// action count stays, since clients compare against it.
    fn forget(&self, reporter: &OwnedUniqueName) {
        self.update(|state| {
            let was_last = self.reporter.send_if_modified(|last| {
                let is_reporter = last.as_ref() == Some(reporter);
                if is_reporter {
                    *last = None;
                }
                is_reporter
            });
            was_last && state.report(Vec::new(), false)
        });
    }
}

// One call at a time, in arrival order: a spawned task per call could let an older report land
// after a newer one.
#[interface(name = "org.ghostshell.Desktop1", spawn = false)]
impl DesktopService {
    /// `covered_outputs` holds output names joined by `,`, and `""` means none.
    // A string rather than `as`, because KWin's `callDBus` sends a script's array as `av`.
    fn report(&self, #[zbus(header)] header: Header<'_>, covered_outputs: &str, active: bool) {
        let covered_outputs = covered_outputs
            .split(OUTPUT_SEPARATOR)
            .map(str::to_owned)
            .collect();
        self.update(|state| {
            // A peer-to-peer caller has no name, so it cannot be watched.
            if let Some(sender) = header.sender() {
                self.reporter.send_if_modified(|last| {
                    let is_new = last.as_deref() != Some(sender);
                    if is_new {
                        *last = Some(sender.to_owned().into());
                    }
                    is_new
                });
            }
            state.report(covered_outputs, active)
        });
    }

    /// Asks for the `monitor` or `swap` view, which happens only while the desktop has focus.
    pub fn activate(&self, action: &str) -> fdo::Result<()> {
        let action = match action {
            MONITOR => ViewAction::Monitor,
            SWAP => ViewAction::Swap,
            unknown => {
                return Err(fdo::Error::InvalidArgs(format!(
                    "unknown view action {unknown:?}"
                )));
            }
        };
        self.update(|state| state.activate(action));
        Ok(())
    }
}

/// Serves the desktop service on the session bus and registers the view shortcuts, noting in
/// `registration` that it did.
///
/// Without a session bus, or while another ghostd owns the name, `desktop` keeps its default: the
/// dashboard still renders, and shortcuts stay off.
pub fn feed(registration: Registration) -> Feed {
    feed_on(connection::Builder::session(), registration)
}

/// As [`feed`], on the bus that `bus` connects to.
pub fn feed_on(bus: connection::Builder<'static>, registration: Registration) -> Feed {
    Feed::new(topics::DESKTOP, move |context| async move {
        context
            .publisher
            .publish(topics::DESKTOP, &DesktopSnapshot::default());
        let desktop = DesktopService::new(context.publisher);
        // Kept until the daemon stops, since the service is served only while its connection lives.
        let connection = serve(bus, desktop.clone()).await;
        match &connection {
            Ok(connection) => {
                tokio::join!(
                    shortcuts::run(
                        connection.clone(),
                        context.config,
                        desktop.clone(),
                        registration
                    ),
                    forget_departed_reporters(connection, &desktop),
                );
            }
            Err(error) => warn!(
                %error,
                "cannot serve {BUS_NAME} on the session bus; the desktop topic keeps its defaults"
            ),
        }
        pending::<()>().await;
    })
}

async fn serve(
    bus: connection::Builder<'static>,
    desktop: DesktopService,
) -> zbus::Result<Connection> {
    bus
        // A second ghostd must leave the desktop to the one that already serves it.
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .name(BUS_NAME)
        .serve_at(OBJECT_PATH, desktop)
        .build()
        .await
}

/// Resets the desktop when the connection that sent the last report leaves the bus, as when KWin
/// exits, so that a report nobody will update cannot keep the shortcuts on.
async fn forget_departed_reporters(conn: &Connection, desktop: &DesktopService) {
    if let Err(error) = watch_reporters(conn, desktop).await {
        warn!(%error, "cannot watch the desktop's reporter; its last report stands until the next");
    }
}

async fn watch_reporters(conn: &Connection, desktop: &DesktopService) -> zbus::Result<()> {
    let bus = fdo::DBusProxy::builder(conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let mut reporters = desktop.reporter.subscribe();
    loop {
        let reporter = reporters.borrow_and_update().clone();
        let departed = async {
            match &reporter {
                Some(reporter) => until_departed(&bus, reporter).await,
                None => pending().await,
            }
        };
        tokio::select! {
            departed = departed => {
                departed?;
                if let Some(reporter) = &reporter {
                    desktop.forget(reporter);
                }
            }
            changed = reporters.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
            }
        }
    }
}

/// Resolves once `reporter` has left the bus.
async fn until_departed(bus: &fdo::DBusProxy<'_>, reporter: &OwnedUniqueName) -> zbus::Result<()> {
    let mut owner_changes = bus
        .receive_name_owner_changed_with_args(&[(0, reporter.as_str())])
        .await?;
    // It may have left before the match above was in place, and a unique name is never reused.
    if !bus
        .name_has_owner(BusName::Unique(reporter.as_ref().clone()))
        .await?
    {
        return Ok(());
    }
    // A unique name changes owner only by leaving.
    if next(&mut owner_changes).await.is_none() {
        pending::<()>().await;
    }
    Ok(())
}

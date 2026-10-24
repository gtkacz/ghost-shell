//! The `desktop` topic: which outputs windows cover, whether the desktop has focus, and the view
//! actions the shortcuts ask for. KWin's desktop script reports the first two over D-Bus.

use std::collections::BTreeSet;
use std::future::pending;
use std::sync::{Arc, Mutex, PoisonError};

use ghost_proto::{DesktopSnapshot, ViewAction, WindowsSnapshot, topics};
use tracing::warn;
use zbus::fdo::{NameOwnerChangedStream, RequestNameFlags};
use zbus::proxy::CacheProperties;
use zbus::{Connection, connection, fdo, interface};

use super::shortcuts::{self, Registration};
use super::windows::{Bridge, Timings};
use super::{kwin_script, next};
use crate::server::{Feed, Publisher};

const BUS_NAME: &str = "org.ghostshell.Ghostd";
pub(crate) const KWIN: &str = "org.kde.KWin";
const OBJECT_PATH: &str = "/org/ghostshell/Desktop";
const OUTPUT_SEPARATOR: char = ',';
/// The names `Activate` takes, which are also the ids the shortcuts are registered under, so a
/// pressed shortcut can be passed on as it is.
pub(crate) const MONITOR: &str = "monitor";
pub(crate) const SWAP: &str = "swap";
pub(crate) const LAUNCHER: &str = "launcher";
pub(crate) const NOTIFICATIONS: &str = "notifications";

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

    /// Counts a launcher toggle, which needs no focus: the bar's launcher button works over windows.
    pub fn toggle_launcher(&mut self) -> bool {
        self.snapshot.launcher_seq += 1;
        true
    }

    /// Counts a press of the notification-centre shortcut, which needs no focus: the bell shows over
    /// windows too.
    pub fn open_notifications(&mut self) -> bool {
        self.snapshot.notifications_seq += 1;
        true
    }

    /// Passes on a task shortcut whatever has focus, since the bar shows over windows too.
    pub fn press_task(&mut self, index: u8) -> bool {
        self.snapshot.task = Some(index);
        self.snapshot.task_seq += 1;
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
    publisher: Publisher,
}

impl DesktopService {
    pub fn new(publisher: Publisher) -> Self {
        Self {
            state: Arc::default(),
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

    pub fn press_task(&self, index: u8) {
        self.update(|state| state.press_task(index));
    }

    pub fn open_notifications(&self) {
        self.update(DesktopState::open_notifications);
    }

    /// Goes back to no covered outputs and no focus. The counts stay, since clients compare
    /// against it.
    fn reset(&self) {
        self.update(|state| state.report(Vec::new(), false));
    }
}

// One call at a time, in arrival order: a spawned task per call could let an older report land
// after a newer one.
#[interface(name = "org.ghostshell.Desktop1", spawn = false)]
impl DesktopService {
    /// `covered_outputs` holds output names joined by `,`, and `""` means none.
    // A string rather than `as`, because KWin's `callDBus` sends a script's array as `av`.
    fn report(&self, covered_outputs: &str, active: bool) {
        let covered_outputs = covered_outputs
            .split(OUTPUT_SEPARATOR)
            .map(str::to_owned)
            .collect();
        self.update(|state| state.report(covered_outputs, active));
    }

    /// Opens or closes the launcher, whatever has focus.
    pub fn toggle_launcher(&self) {
        self.update(DesktopState::toggle_launcher);
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

/// Whether the feed asks KWin's desktop script to report again once it serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptReload {
    Request,
    /// For a daemon that must leave the user's KWin alone.
    Skip,
}

/// Serves the desktop service on the session bus and registers the view shortcuts, noting in
/// `registration` that it did.
///
/// Without a session bus, or while another ghostd owns the name, `desktop` keeps its default: the
/// dashboard still renders, and shortcuts stay off.
pub fn feed(registration: Registration, reload: ScriptReload) -> Feed {
    feed_on(connection::Builder::session(), registration, reload)
}

/// As [`feed`], on the bus that `bus` connects to.
pub fn feed_on(
    bus: connection::Builder<'static>,
    registration: Registration,
    reload: ScriptReload,
) -> Feed {
    Feed::new(topics::DESKTOP, move |context| async move {
        context
            .publisher
            .publish(topics::DESKTOP, &DesktopSnapshot::default());
        context
            .publisher
            .publish(topics::WINDOWS, &WindowsSnapshot::default());
        let bridge = Bridge::new(context.publisher.clone(), Timings::default());
        let desktop = DesktopService::new(context.publisher);
        // Kept until the daemon stops, since the service is served only while its connection lives.
        let mut served = serve(bus, desktop.clone(), &bridge).await;
        match &mut served {
            Ok((connection, kwin_owners)) => {
                tokio::join!(
                    shortcuts::run(
                        connection.clone(),
                        context.config,
                        desktop.clone(),
                        registration
                    ),
                    reset_when_kwin_leaves(kwin_owners, &desktop),
                    bridge.forward(context.calls),
                    async {
                        // Only now, and not when KWin appears later: a KWin that starts loads
                        // its enabled scripts itself, and they report as they load.
                        if reload == ScriptReload::Request {
                            kwin_script::reload(connection).await;
                        }
                    },
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

/// Serves `desktop` and takes the bus name, returning KWin's owner changes too.
async fn serve(
    bus: connection::Builder<'static>,
    desktop: DesktopService,
    bridge: &Bridge,
) -> zbus::Result<(Connection, NameOwnerChangedStream)> {
    let connection = bridge
        .serve(bus.serve_at(OBJECT_PATH, desktop))
        .build()
        .await?;
    // Watched before the name is taken, since no report can arrive before that, so no KWin exit
    // after a report can be missed.
    let kwin_owners = fdo::DBusProxy::builder(&connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .receive_name_owner_changed_with_args(&[(0, KWIN)])
        .await?;
    // A second ghostd must leave the desktop to the one that already serves it.
    connection
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DO_NOT_QUEUE)
        .await?;
    Ok((connection, kwin_owners))
}

/// Resets the desktop whenever KWin leaves the bus. Its desktop script reports over KWin's own
/// connection, so once KWin is gone nobody is left to correct the last report; a caller that
/// reports once and disconnects, like `busctl`, must not count as gone.
async fn reset_when_kwin_leaves(
    kwin_owners: &mut NameOwnerChangedStream,
    desktop: &DesktopService,
) {
    while let Some(change) = next(kwin_owners).await {
        if change.args().is_ok_and(|args| args.new_owner().is_none()) {
            desktop.reset();
        }
    }
}

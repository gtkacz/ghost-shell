//! The `desktop` topic: which outputs windows cover, whether the desktop has focus, and the view
//! actions the shortcuts ask for. KWin's desktop script reports the first two over D-Bus.

use std::future::pending;
use std::sync::{Arc, Mutex, PoisonError};

use ghost_proto::{DesktopSnapshot, ViewAction, topics};
use tracing::warn;
use zbus::{Connection, connection, fdo, interface};

use super::shortcuts;
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
    /// Returns whether anything changed.
    pub fn report(&mut self, covered_outputs: Vec<String>, active: bool) -> bool {
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
}

// One call at a time, in arrival order: a spawned task per call could let an older report land
// after a newer one.
#[interface(name = "org.ghostshell.Desktop1", spawn = false)]
impl DesktopService {
    /// `covered_outputs` holds output names joined by `,`, and `""` means none.
    // A string rather than `as`, because KWin's `callDBus` sends a script's array as `av`.
    pub fn report(&self, covered_outputs: &str, active: bool) {
        let covered_outputs = covered_outputs
            .split(OUTPUT_SEPARATOR)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        self.update(|state| state.report(covered_outputs, active));
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

/// Serves the desktop service on the session bus and registers the view shortcuts.
///
/// Without a session bus, or while another ghostd owns the name, `desktop` keeps its default: the
/// dashboard still renders, and shortcuts stay off.
pub fn feed() -> Feed {
    Feed::new(topics::DESKTOP, |context| async move {
        context
            .publisher
            .publish(topics::DESKTOP, &DesktopSnapshot::default());
        let desktop = DesktopService::new(context.publisher);
        // Kept until the daemon stops, since the service is served only while its connection lives.
        let connection = serve(desktop.clone()).await;
        match &connection {
            Ok(connection) => shortcuts::run(connection.clone(), context.config, desktop).await,
            Err(error) => warn!(
                %error,
                "cannot serve {BUS_NAME} on the session bus; the desktop topic keeps its defaults"
            ),
        }
        pending::<()>().await;
    })
}

async fn serve(desktop: DesktopService) -> zbus::Result<Connection> {
    connection::Builder::session()
        // A second ghostd must leave the desktop to the one that already serves it.
        .allow_name_replacements(false)
        .replace_existing_names(false)
        .name(BUS_NAME)
        .serve_at(OBJECT_PATH, desktop)
        .build()
        .await
}

//! The view shortcuts, registered with KDE's global shortcut service, KGlobalAccel.
//!
//! The keys always come from `shortcuts.*` in the config, never from what KGlobalAccel stored, and
//! a press takes the same path as a D-Bus `Activate`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ghost_proto::{ResolvedConfig, Shortcuts};
use tokio::sync::watch;
use tokio::time::timeout;
use tracing::warn;
use zbus::fdo::{self, NameOwnerChanged};
use zbus::proxy::CacheProperties;
use zbus::{Connection, MatchRule, Message, MessageStream, message, proxy};

use super::desktop::{DesktopService, LAUNCHER, MONITOR, SWAP};
use super::next;
use crate::config::MAX_TASK_SHORTCUTS;
use crate::keyseq;

const KGLOBALACCEL: &str = "org.kde.kglobalaccel";
const COMPONENT: &str = "ghost-shell";
// KGlobalAccel names a component's object after its id, with `-` replaced by `_`.
const COMPONENT_PATH: &str = "/component/ghost_shell";
const COMPONENT_INTERFACE: &str = "org.kde.kglobalaccel.Component";
const PRESSED_SIGNAL: &str = "globalShortcutPressed";
const MONITOR_ID: [&str; 4] = [COMPONENT, MONITOR, COMPONENT, "Toggle the monitor view"];
const SWAP_ID: [&str; 4] = [COMPONENT, SWAP, COMPONENT, "Next main view"];
const LAUNCHER_ID: [&str; 4] = [COMPONENT, LAUNCHER, COMPONENT, "Open the launcher"];
const TASK_PREFIX: &str = "task-";
// `SetShortcutFlag` in kglobalaccel 6.31's src/kglobalaccel_p.h.
const SET_PRESENT: u32 = 2;
const NO_AUTOLOADING: u32 = 4;
// `KGlobalAccel::MatchType::Equal` in kglobalaccel 6.31's src/kglobalaccel.h.
const MATCH_EQUAL: i32 = 0;
// A Qt key sequence holds four keys, and KGlobalAccel's own client always sends all four.
const NO_KEY: i32 = 0;
const RELEASE_TIMEOUT: Duration = Duration::from_millis(500);

/// The id of the shortcut that focuses the nth pinned app, counted from 1.
struct TaskId {
    action: String,
    label: String,
}

impl TaskId {
    fn new(number: usize) -> Self {
        Self {
            action: format!("{TASK_PREFIX}{number}"),
            label: format!("Activate pinned app {number}"),
        }
    }

    fn all() -> impl Iterator<Item = Self> {
        (1..=MAX_TASK_SHORTCUTS).map(Self::new)
    }

    fn as_ids(&self) -> [&str; 4] {
        [COMPONENT, &self.action, COMPONENT, &self.label]
    }
}

/// An action that holds a key: its id and name, its component's, its context's, then its keys and
/// default keys.
type ShortcutInfo = (
    String,
    String,
    String,
    String,
    String,
    String,
    Vec<i32>,
    Vec<i32>,
);

#[proxy(
    interface = "org.kde.KGlobalAccel",
    default_service = "org.kde.kglobalaccel",
    default_path = "/kglobalaccel"
)]
trait KGlobalAccel {
    #[zbus(name = "doRegister")]
    fn do_register(&self, action_id: &[&str]) -> zbus::Result<()>;

    /// Returns the keys the action ends up with.
    #[zbus(name = "setShortcutKeys")]
    fn set_shortcut_keys(
        &self,
        action_id: &[&str],
        keys: &[(&[i32],)],
        flags: u32,
    ) -> zbus::Result<Vec<(Vec<i32>,)>>;

    #[zbus(name = "globalShortcutsByKey")]
    fn global_shortcuts_by_key(
        &self,
        key: (&[i32],),
        match_type: (i32,),
    ) -> zbus::Result<Vec<ShortcutInfo>>;

    /// Stops grabbing the action's keys, keeping them stored.
    #[zbus(name = "setInactive")]
    fn set_inactive(&self, action_id: &[&str]) -> zbus::Result<()>;
}

/// Whether this daemon registered the view shortcuts, so that a clean stop releases them, and only
/// then: a ghostd that never got the bus name must leave the shortcuts of the one that did alone.
#[derive(Debug, Clone, Default)]
pub struct Registration(Arc<AtomicBool>);

impl Registration {
    fn mark(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn is_marked(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// Marks the shortcuts inactive over a new session-bus connection, so that KGlobalAccel stops
    /// grabbing their keys while ghostd is stopped. Gives up after `RELEASE_TIMEOUT`, so that a
    /// stuck bus cannot hold up the stop.
    pub async fn release(&self) {
        if !self.is_marked() {
            return;
        }
        let released = timeout(RELEASE_TIMEOUT, async {
            match Connection::session().await {
                Ok(conn) => self.release_on(&conn).await,
                Err(error) => warn!(%error, "cannot release the view shortcuts"),
            }
        })
        .await;
        if released.is_err() {
            warn!(
                "KGlobalAccel did not answer within {RELEASE_TIMEOUT:?}; the view shortcuts stay \
                 grabbed"
            );
        }
    }

    /// As [`Self::release`], over `conn` and without a time limit. Each action is released even
    /// when the other one fails.
    pub async fn release_on(&self, conn: &Connection) {
        if !self.is_marked() {
            return;
        }
        let kglobalaccel = match kglobalaccel(conn).await {
            Ok(kglobalaccel) => kglobalaccel,
            Err(error) => {
                warn!(%error, "cannot release the view shortcuts");
                return;
            }
        };
        let tasks: Vec<TaskId> = TaskId::all().collect();
        let task_ids = tasks.iter().map(TaskId::as_ids);
        for action_id in [MONITOR_ID, SWAP_ID, LAUNCHER_ID]
            .into_iter()
            .chain(task_ids)
        {
            if let Err(error) = kglobalaccel.set_inactive(&action_id).await {
                warn!(%error, "cannot release the {} shortcut", action_id[1]);
            }
        }
    }
}

/// Registers the view shortcuts on `conn`, noting it in `registration`, and activates a shortcut's
/// view when it is pressed. Registers them again whenever `shortcuts.*` changes, and whenever
/// KGlobalAccel appears: it lives inside KWin, which can start after ghostd or restart.
///
/// Without KGlobalAccel it logs once and keeps waiting; `Activate` still works. Returns once the
/// connection is closed and the config can no longer change, or at once when it cannot listen.
pub async fn run(
    conn: Connection,
    mut config: watch::Receiver<ResolvedConfig>,
    desktop: DesktopService,
    registration: Registration,
) {
    // Listening starts before registering, so that neither a press nor KGlobalAccel's arrival can
    // slip in between.
    let (kglobalaccel, mut presses, mut arrivals) = match connect(&conn).await {
        Ok(connected) => connected,
        Err(error) => {
            warn!(%error, "cannot listen for global shortcuts; only Activate changes views");
            return;
        }
    };
    let mut shortcuts = config.borrow_and_update().shortcuts.clone();
    let mut failing = register_or_log(&kglobalaccel, &shortcuts, &registration, false).await;
    loop {
        tokio::select! {
            Some(press) = next(&mut presses) => activate(&desktop, press),
            Some(change) = next(&mut arrivals) => {
                if has_new_owner(&change) {
                    failing =
                        register_or_log(&kglobalaccel, &shortcuts, &registration, failing).await;
                }
            }
            Ok(()) = config.changed() => {
                let latest = config.borrow_and_update().shortcuts.clone();
                if latest != shortcuts {
                    shortcuts = latest;
                    failing =
                        register_or_log(&kglobalaccel, &shortcuts, &registration, failing).await;
                }
            }
            else => return,
        }
    }
}

async fn connect(
    conn: &Connection,
) -> zbus::Result<(
    KGlobalAccelProxy<'static>,
    MessageStream,
    fdo::NameOwnerChangedStream,
)> {
    // The bus, not zbus, applies a rule's well-known sender, following the name to its owner.
    let pressed = MatchRule::builder()
        .msg_type(message::Type::Signal)
        .sender(KGLOBALACCEL)
        .path(COMPONENT_PATH)
        .interface(COMPONENT_INTERFACE)
        .member(PRESSED_SIGNAL)
        .build()?;
    let presses = MessageStream::for_match_rule(pressed, conn, None).await?;
    let arrivals = fdo::DBusProxy::builder(conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .receive_name_owner_changed_with_args(&[(0, KGLOBALACCEL)])
        .await?;
    Ok((kglobalaccel(conn).await?, presses, arrivals))
}

async fn kglobalaccel(conn: &Connection) -> zbus::Result<KGlobalAccelProxy<'static>> {
    KGlobalAccelProxy::builder(conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

/// Whether KGlobalAccel has just appeared, rather than left.
fn has_new_owner(change: &NameOwnerChanged) -> bool {
    change.args().is_ok_and(|args| args.new_owner().is_some())
}

/// Logs a failure only after a success or at the start, so a session without KGlobalAccel logs
/// once. Returns whether this attempt failed.
async fn register_or_log(
    kglobalaccel: &KGlobalAccelProxy<'_>,
    shortcuts: &Shortcuts,
    registration: &Registration,
    failed_before: bool,
) -> bool {
    match register(kglobalaccel, shortcuts, registration).await {
        Ok(()) => false,
        Err(error) => {
            if !failed_before {
                warn!(%error, "cannot register the view shortcuts; only Activate changes views");
            }
            true
        }
    }
}

async fn register(
    kglobalaccel: &KGlobalAccelProxy<'_>,
    shortcuts: &Shortcuts,
    registration: &Registration,
) -> zbus::Result<()> {
    let tasks: Vec<TaskId> = TaskId::all().collect();
    let configured_tasks = shortcuts.tasks.iter().zip(&tasks);
    let held = [
        (MONITOR_ID, &shortcuts.monitor),
        (SWAP_ID, &shortcuts.swap),
        (LAUNCHER_ID, &shortcuts.launcher),
    ]
    .into_iter()
    .chain(configured_tasks.map(|(sequence, task)| (task.as_ids(), sequence)));
    for (action_id, sequence) in held {
        // Loading the config already rejected sequences that do not parse.
        let key = match keyseq::parse(sequence) {
            Ok(key) => key,
            Err(error) => {
                warn!(%error, "the {} shortcut is not registered", action_id[1]);
                continue;
            }
        };
        let keys: &[i32] = &[key, NO_KEY, NO_KEY, NO_KEY];
        kglobalaccel.do_register(&action_id).await?;
        kglobalaccel
            .set_shortcut_keys(&action_id, &[(keys,)], SET_PRESENT | NO_AUTOLOADING)
            .await?;
        registration.mark();
        warn_if_shared(kglobalaccel, sequence, keys).await;
    }
    // A shortened list must give the keys of its dropped entries back.
    for task in tasks.iter().skip(shortcuts.tasks.len()) {
        if let Err(error) = kglobalaccel.set_inactive(&task.as_ids()).await {
            warn!(%error, "cannot free the {} shortcut", task.action);
        }
    }
    Ok(())
}

/// KGlobalAccel accepts a key that another component already holds, and a press then runs only
/// one of the two.
async fn warn_if_shared(kglobalaccel: &KGlobalAccelProxy<'_>, sequence: &str, keys: &[i32]) {
    match kglobalaccel
        .global_shortcuts_by_key((keys,), (MATCH_EQUAL,))
        .await
    {
        Ok(holders) => {
            for (action, _, component, ..) in holders
                .iter()
                .filter(|(_, _, component, ..)| component != COMPONENT)
            {
                // Until the Plasma panel is retired, plasmashell holds a lone Meta for its launcher.
                warn!(
                    "{sequence} is also bound to {component}'s {action:?}; pressing it may run \
                     that instead of the ghost-shell action"
                );
            }
        }
        Err(error) => warn!(%error, "cannot check whether another shortcut holds {sequence}"),
    }
}

/// The 0-based index of the pinned app that a `task-N` shortcut stands for.
fn task_index(shortcut: &str) -> Option<u8> {
    let number: u8 = shortcut.strip_prefix(TASK_PREFIX)?.parse().ok()?;
    number.checked_sub(1)
}

fn activate(desktop: &DesktopService, press: zbus::Result<Message>) {
    let activated = press.and_then(|press| {
        let body = press.body();
        let (_component, shortcut, _timestamp): (&str, &str, i64) = body.deserialize()?;
        match shortcut {
            LAUNCHER => desktop.toggle_launcher(),
            _ => match task_index(shortcut) {
                Some(index) => desktop.press_task(index),
                None => desktop.activate(shortcut)?,
            },
        }
        Ok(())
    });
    if let Err(error) = activated {
        warn!(%error, "cannot act on a pressed shortcut");
    }
}

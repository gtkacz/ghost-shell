//! The `ticker` topic: failed systemd units, whether firewalld runs, pending updates and the
//! NVIDIA driver, for the bar's ticker.
//!
//! Nothing runs until some client wants the topic, and everything stops once none does. Each
//! value is read where it changes: systemd's `NFailedUnits` after a job ends or the manager
//! reloads, firewalld's state when its bus name changes owner, and PackageKit's cached update list
//! when it says the list changed. A value that cannot be read stays `None`, which the shell leaves
//! out.
//!
//! No call may start a service, except PackageKit's transaction: firewalld and the systemd
//! managers are asked with `NO_AUTO_START`, and `RefreshCache` is never called, so the update count
//! never reaches out to the network.

use std::future::pending;
use std::sync::Mutex;
use std::time::Duration;

use ghost_proto::{TickerSnapshot, topics};
use tokio::task::spawn_blocking;
use tokio::time::{sleep, timeout};
use tracing::{debug, warn};
use zbus::fdo;
use zbus::proxy::{self, CacheProperties, MethodFlags, SignalStream};
use zbus::{Connection, Message, OwnedObjectPath, OwnedVariant, Proxy};

use super::{drain, next, until_unwanted, until_wanted};
use crate::collect::read_text;
use crate::server::{Feed, FeedContext, Publisher};
use crate::sysroot::SysRoot;

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";
const MANAGER_INTERFACE: &str = "org.freedesktop.systemd1.Manager";
const N_FAILED_UNITS: &str = "NFailedUnits";
const SUBSCRIBE: &str = "Subscribe";
const UNSUBSCRIBE: &str = "Unsubscribe";
const JOB_REMOVED: &str = "JobRemoved";
const RELOADING: &str = "Reloading";
const FIREWALLD: &str = "org.fedoraproject.FirewallD1";
const FIREWALLD_PATH: &str = "/org/fedoraproject/FirewallD1";
const FIREWALLD_STATE: &str = "state";
const FIREWALLD_RUNNING: &str = "RUNNING";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const PACKAGEKIT: &str = "org.freedesktop.PackageKit";
const PACKAGEKIT_PATH: &str = "/org/freedesktop/PackageKit";
const TRANSACTION_INTERFACE: &str = "org.freedesktop.PackageKit.Transaction";
const CREATE_TRANSACTION: &str = "CreateTransaction";
const GET_UPDATES: &str = "GetUpdates";
const UPDATES_CHANGED: &str = "UpdatesChanged";
const PACKAGE: &str = "Package";
const FINISHED: &str = "Finished";
const NO_FILTER: u64 = 0;
/// PackageKit's `PK_EXIT_ENUM_SUCCESS`.
const EXIT_SUCCESS: u32 = 1;
const NVIDIA_VERSION: &str = "sys/module/nvidia/version";
/// A burst of job signals, as a package upgrade causes, costs one read of the property.
const COALESCE: Duration = Duration::from_millis(500);
/// PackageKit answers from its cache in seconds; one that is silent for longer is stuck.
const UPDATES_TIMEOUT: Duration = Duration::from_secs(30);

/// Reads the system and session buses, which are connected to only once the topic is first wanted.
pub fn feed() -> Feed {
    Feed::new(topics::TICKER, |mut context| async move {
        until_wanted(&mut context.demand, topics::TICKER).await;
        match (Connection::system().await, Connection::session().await) {
            (Ok(system), Ok(session)) => run(system, session, SysRoot::host(), context).await,
            (system, session) => {
                warn!(
                    system = ?system.err(),
                    session = ?session.err(),
                    "cannot connect to the buses; the ticker has no values"
                );
                context
                    .publisher
                    .publish(topics::TICKER, &TickerSnapshot::default());
                pending::<()>().await;
            }
        }
    })
}

/// As [`feed`], with the system and session buses given and `root` for the driver file.
pub fn feed_on(system: Connection, session: Connection, root: SysRoot) -> Feed {
    Feed::new(topics::TICKER, move |context| {
        run(system, session, root, context)
    })
}

async fn run(system: Connection, session: Connection, root: SysRoot, mut context: FeedContext) {
    loop {
        until_wanted(&mut context.demand, topics::TICKER).await;
        let ticker = Ticker::new(context.publisher.clone());
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::TICKER) => {}
            _ = async {
                tokio::join!(
                    failed_units(&system, &session, &ticker),
                    firewalld(&system, &ticker),
                    updates(&system, &ticker),
                    nvidia_driver(&root, &ticker),
                )
            } => {}
        }
        for bus in [&system, &session] {
            // The connections outlive the want, and a subscribed manager keeps signalling them.
            if let Err(error) = unsubscribe(bus).await {
                warn!(%error, "systemd would not drop the ticker's subscription");
            }
        }
    }
}

/// The snapshot so far, published each time a source changes it.
struct Ticker {
    snapshot: Mutex<TickerSnapshot>,
    publisher: Publisher,
}

impl Ticker {
    /// Publishes the all-unknown snapshot, so a value from before the topic was last unwanted does
    /// not outlive its refresh.
    fn new(publisher: Publisher) -> Self {
        publisher.publish(topics::TICKER, &TickerSnapshot::default());
        Self {
            snapshot: Mutex::default(),
            publisher,
        }
    }

    fn update(&self, change: impl FnOnce(&mut TickerSnapshot)) {
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = snapshot.clone();
        change(&mut snapshot);
        if *snapshot != before {
            self.publisher.publish(topics::TICKER, &*snapshot);
        }
    }
}

async fn proxy(
    bus: &Connection,
    destination: &'static str,
    path: impl Into<String>,
    interface: &'static str,
) -> zbus::Result<Proxy<'static>> {
    proxy::Builder::<Proxy<'_>>::new(bus)
        .destination(destination)
        .path(path.into())
        .interface(interface)
        .cache_properties(CacheProperties::No)
        .build()
        .await
}

/// A property of a service the bus must not start: it is read, or the service is absent.
async fn property(
    bus: &Connection,
    destination: &'static str,
    path: &str,
    interface: &str,
    name: &str,
) -> zbus::Result<OwnedVariant> {
    let reply = proxy(bus, destination, path, PROPERTIES_INTERFACE)
        .await?
        .call_with_flags("Get", MethodFlags::NO_AUTO_START, &(interface, name))
        .await?;
    Ok(reply.expect("a call not flagged NO_REPLY_EXPECTED always has a reply"))
}

async fn failed_units(system: &Connection, session: &Connection, ticker: &Ticker) {
    let (mut system_signals, mut user_signals) = match (
        manager_signals(system).await,
        manager_signals(session).await,
    ) {
        (Ok(system_signals), Ok(user_signals)) => (system_signals, user_signals),
        (system, session) => {
            warn!(
                system = ?system.err(),
                session = ?session.err(),
                "cannot watch systemd; the ticker has no failed units"
            );
            return;
        }
    };
    for bus in [system, session] {
        // Without Subscribe the manager sends no JobRemoved at all.
        if let Err(error) = subscribe(bus).await {
            debug!(%error, "systemd would not take a subscription; failed units update on reloads only");
        }
    }
    loop {
        let total = failed_total(system, session).await;
        ticker.update(|snapshot| snapshot.failed_units = total);
        tokio::select! {
            () = refresh_signal(&mut system_signals) => {}
            () = refresh_signal(&mut user_signals) => {}
        }
        sleep(COALESCE).await;
        drain(&mut system_signals);
        drain(&mut user_signals);
    }
}

async fn manager_signals(bus: &Connection) -> zbus::Result<SignalStream<'static>> {
    proxy(bus, SYSTEMD, SYSTEMD_PATH, MANAGER_INTERFACE)
        .await?
        .receive_all_signals()
        .await
}

async fn unsubscribe(bus: &Connection) -> zbus::Result<()> {
    proxy(bus, SYSTEMD, SYSTEMD_PATH, MANAGER_INTERFACE)
        .await?
        .call_with_flags::<_, _, ()>(UNSUBSCRIBE, MethodFlags::NO_AUTO_START, &())
        .await?;
    Ok(())
}

async fn subscribe(bus: &Connection) -> zbus::Result<()> {
    proxy(bus, SYSTEMD, SYSTEMD_PATH, MANAGER_INTERFACE)
        .await?
        .call_with_flags::<_, _, ()>(SUBSCRIBE, MethodFlags::NO_AUTO_START, &())
        .await?;
    Ok(())
}

/// The sum over both managers, unknown unless both answer.
async fn failed_total(system: &Connection, session: &Connection) -> Option<u32> {
    let (system, user) = tokio::join!(failed_of(system), failed_of(session));
    Some(system? + user?)
}

async fn failed_of(bus: &Connection) -> Option<u32> {
    property(
        bus,
        SYSTEMD,
        SYSTEMD_PATH,
        MANAGER_INTERFACE,
        N_FAILED_UNITS,
    )
    .await
    .inspect_err(|error| debug!(%error, "cannot read NFailedUnits"))
    .ok()?
    .downcast_ref()
    .ok()
}

/// Resolves on a job ending or a reload finishing, the moments the failed count can change.
async fn refresh_signal(signals: &mut SignalStream<'static>) {
    while let Some(message) = next(signals).await {
        if changes_failed_units(&message) {
            return;
        }
    }
    pending().await
}

fn changes_failed_units(message: &Message) -> bool {
    match message.header().member().map(|member| member.as_str()) {
        Some(JOB_REMOVED) => true,
        Some(RELOADING) => message.body().deserialize::<bool>() == Ok(false),
        _ => false,
    }
}

async fn firewalld(system: &Connection, ticker: &Ticker) {
    let bus = match fdo::DBusProxy::builder(system)
        .cache_properties(CacheProperties::No)
        .build()
        .await
    {
        Ok(bus) => bus,
        Err(error) => {
            warn!(%error, "cannot watch firewalld; the ticker has no firewall state");
            return;
        }
    };
    // Listening before the first read, so that no change of owner is missed.
    let Ok(mut owners) = bus
        .receive_name_owner_changed_with_args(&[(0, FIREWALLD)])
        .await
    else {
        warn!("cannot watch firewalld; the ticker has no firewall state");
        return;
    };
    loop {
        drain(&mut owners);
        let state = firewalld_running(system, &bus).await;
        ticker.update(|snapshot| snapshot.firewalld = state);
        if next(&mut owners).await.is_none() {
            return pending().await;
        }
    }
}

/// Without an owner firewalld is not running, which is a state and not an unknown.
async fn firewalld_running(system: &Connection, bus: &fdo::DBusProxy<'_>) -> Option<bool> {
    let owned = bus
        .name_has_owner(FIREWALLD.try_into().expect("a well-known bus name"))
        .await
        .inspect_err(|error| debug!(%error, "cannot ask who owns firewalld"))
        .ok()?;
    if !owned {
        return Some(false);
    }
    let state = property(
        system,
        FIREWALLD,
        FIREWALLD_PATH,
        FIREWALLD,
        FIREWALLD_STATE,
    )
    .await
    .inspect_err(|error| debug!(%error, "cannot read firewalld's state"))
    .ok()?;
    Some(state.downcast_ref::<&str>().ok()? == FIREWALLD_RUNNING)
}

async fn updates(system: &Connection, ticker: &Ticker) {
    let mut changes = match proxy(system, PACKAGEKIT, PACKAGEKIT_PATH, PACKAGEKIT).await {
        Ok(packagekit) => packagekit.receive_signal(UPDATES_CHANGED).await.ok(),
        Err(_) => None,
    };
    if changes.is_none() {
        warn!("cannot watch PackageKit; the ticker counts updates once");
    }
    loop {
        let count = count_updates(system).await;
        ticker.update(|snapshot| snapshot.updates = count);
        let Some(changes) = &mut changes else {
            return pending().await;
        };
        if next(changes).await.is_none() {
            return pending().await;
        }
        drain(changes);
    }
}

async fn count_updates(bus: &Connection) -> Option<u32> {
    match timeout(UPDATES_TIMEOUT, list_updates(bus)).await {
        Ok(Ok(count)) => Some(count),
        Ok(Err(error)) => {
            debug!(%error, "PackageKit would not list updates");
            None
        }
        Err(_) => {
            warn!("PackageKit did not list updates within {UPDATES_TIMEOUT:?}");
            None
        }
    }
}

/// Counts the `Package` signals of one `GetUpdates`, which lists the cache and refreshes nothing.
async fn list_updates(bus: &Connection) -> zbus::Result<u32> {
    let path: OwnedObjectPath = proxy(bus, PACKAGEKIT, PACKAGEKIT_PATH, PACKAGEKIT)
        .await?
        .call(CREATE_TRANSACTION, &())
        .await?;
    let transaction = proxy(bus, PACKAGEKIT, path.as_str(), TRANSACTION_INTERFACE).await?;
    // Listening before the call: the signals can follow its reply at once.
    let mut signals = transaction.receive_all_signals().await?;
    transaction
        .call::<_, _, ()>(GET_UPDATES, &(NO_FILTER,))
        .await?;
    let mut packages = 0;
    while let Some(message) = next(&mut signals).await {
        match message.header().member().map(|member| member.as_str()) {
            Some(PACKAGE) => packages += 1,
            Some(FINISHED) => {
                let (exit, _runtime_s): (u32, u32) = message.body().deserialize()?;
                return if exit == EXIT_SUCCESS {
                    Ok(packages)
                } else {
                    Err(zbus::Error::Failure(format!(
                        "PackageKit exited with {exit}"
                    )))
                };
            }
            _ => {}
        }
    }
    Err(zbus::Error::Failure(
        "the bus closed before PackageKit finished".into(),
    ))
}

async fn nvidia_driver(root: &SysRoot, ticker: &Ticker) {
    let root = root.clone();
    let version = spawn_blocking(move || read_text(&root, NVIDIA_VERSION))
        .await
        .expect("reading the driver file never panics");
    ticker.update(|snapshot| snapshot.nvidia_driver = version);
}

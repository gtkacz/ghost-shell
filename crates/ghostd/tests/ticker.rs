mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use common::FakeRoot;
use common::actions::assert_private;
use common::bus::{BUS_WAIT, PrivateBus};
use common::calls::Calls;
use ghost_proto::{ResolvedConfig, TickerSnapshot, topics};
use ghostd::feeds::ticker;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use zbus::message::Header;
use zbus::{Connection, OwnedObjectPath, interface};

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";
const MANAGER: &str = "org.freedesktop.systemd1.Manager";
const FIREWALLD: &str = "org.fedoraproject.FirewallD1";
const FIREWALLD_PATH: &str = "/org/fedoraproject/FirewallD1";
const PACKAGEKIT: &str = "org.freedesktop.PackageKit";
const PACKAGEKIT_PATH: &str = "/org/freedesktop/PackageKit";
const TRANSACTION: &str = "org.freedesktop.PackageKit.Transaction";
const TRANSACTION_PATH: &str = "/org/freedesktop/PackageKit/Transactions/1";
const NVIDIA_VERSION_FILE: &str = "sys/module/nvidia/version";
const NVIDIA_VERSION: &str = "580.95.05";
const SYSTEM_FAILED: u32 = 1;
const USER_FAILED: u32 = 2;
const PENDING_UPDATES: u32 = 16;
const EXIT_SUCCESS: u32 = 1;
const BURST_SIGNALS: u32 = 20;
const BURST_GAP: Duration = Duration::from_millis(5);
const COALESCE_SETTLE: Duration = Duration::from_millis(1_500);
const QUIET: Duration = Duration::from_millis(400);

struct FakeManager {
    failed: Arc<AtomicU32>,
    calls: Calls,
}

#[interface(name = "org.freedesktop.systemd1.Manager")]
impl FakeManager {
    fn subscribe(&self, #[zbus(header)] header: Header<'_>) {
        self.calls.method(&header, "Subscribe");
    }

    #[zbus(property, name = "NFailedUnits")]
    fn failed_units(&self, #[zbus(header)] header: Option<Header<'_>>) -> u32 {
        self.calls.property(header.as_ref(), "NFailedUnits");
        self.failed.load(Ordering::SeqCst)
    }
}

/// A systemd manager serving `NFailedUnits`, counting every call it gets.
struct Systemd {
    connection: Connection,
    failed: Arc<AtomicU32>,
    calls: Calls,
}

impl Systemd {
    async fn start(bus: &PrivateBus, failed: u32) -> Self {
        assert_private(bus);
        let connection = bus.connect().await;
        let failed = Arc::new(AtomicU32::new(failed));
        let calls = Calls::default();
        connection
            .object_server()
            .at(
                SYSTEMD_PATH,
                FakeManager {
                    failed: Arc::clone(&failed),
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        connection.request_name(SYSTEMD).await.unwrap();
        Self {
            connection,
            failed,
            calls,
        }
    }

    async fn job_removed(&self) {
        let path = OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/1").unwrap();
        self.connection
            .emit_signal(
                None::<&str>,
                SYSTEMD_PATH,
                MANAGER,
                "JobRemoved",
                &(1u32, path, "unit.service", "done"),
            )
            .await
            .unwrap();
    }
}

struct FakeFirewalld {
    calls: Calls,
}

#[interface(name = "org.fedoraproject.FirewallD1")]
impl FakeFirewalld {
    #[zbus(property, name = "state")]
    fn state(&self, #[zbus(header)] header: Option<Header<'_>>) -> String {
        self.calls.property(header.as_ref(), "state");
        "RUNNING".to_owned()
    }
}

struct FakePackageKit {
    fail: bool,
    calls: Calls,
}

#[interface(name = "org.freedesktop.PackageKit")]
impl FakePackageKit {
    fn create_transaction(
        &self,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.calls.method(&header, "CreateTransaction");
        if self.fail {
            return Err(zbus::fdo::Error::Failed("no transactions".into()));
        }
        Ok(OwnedObjectPath::try_from(TRANSACTION_PATH).unwrap())
    }
}

struct FakeTransaction {
    packages: u32,
    calls: Calls,
}

#[interface(name = "org.freedesktop.PackageKit.Transaction")]
impl FakeTransaction {
    async fn get_updates(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        _filter: u64,
    ) {
        self.calls.method(&header, "GetUpdates");
        let connection = connection.clone();
        let packages = self.packages;
        tokio::spawn(async move {
            for index in 0..packages {
                let id = format!("pkg{index};1.0;x86_64;fedora");
                connection
                    .emit_signal(
                        None::<&str>,
                        TRANSACTION_PATH,
                        TRANSACTION,
                        "Package",
                        &(8u32, id, "a package"),
                    )
                    .await
                    .unwrap();
            }
            connection
                .emit_signal(
                    None::<&str>,
                    TRANSACTION_PATH,
                    TRANSACTION,
                    "Finished",
                    &(EXIT_SUCCESS, 0u32),
                )
                .await
                .unwrap();
        });
    }

    fn refresh_cache(&self, #[zbus(header)] header: Header<'_>, _force: bool) {
        self.calls.method(&header, "RefreshCache");
    }
}

/// PackageKit with `packages` pending updates, or one whose `CreateTransaction` fails.
struct PackageKit {
    connection: Connection,
    calls: Calls,
}

impl PackageKit {
    async fn start(bus: &PrivateBus, packages: u32, fail: bool) -> Self {
        assert_private(bus);
        let connection = bus.connect().await;
        let calls = Calls::default();
        let server = connection.object_server();
        server
            .at(
                PACKAGEKIT_PATH,
                FakePackageKit {
                    fail,
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        server
            .at(
                TRANSACTION_PATH,
                FakeTransaction {
                    packages,
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        connection.request_name(PACKAGEKIT).await.unwrap();
        Self { connection, calls }
    }

    async fn updates_changed(&self) {
        self.connection
            .emit_signal(
                None::<&str>,
                PACKAGEKIT_PATH,
                PACKAGEKIT,
                "UpdatesChanged",
                &(),
            )
            .await
            .unwrap();
    }
}

/// Serves firewalld, which reports `RUNNING`, and keeps its connection alive.
async fn start_firewalld(bus: &PrivateBus) -> (Connection, Calls) {
    assert_private(bus);
    let connection = bus.connect().await;
    let calls = Calls::default();
    connection
        .object_server()
        .at(
            FIREWALLD_PATH,
            FakeFirewalld {
                calls: calls.clone(),
            },
        )
        .await
        .unwrap();
    connection.request_name(FIREWALLD).await.unwrap();
    (connection, calls)
}

/// The real feed on two private buses, and everything it publishes.
struct Running {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    task: JoinHandle<()>,
}

impl Running {
    async fn start(
        system: &PrivateBus,
        session: &PrivateBus,
        root: &FakeRoot,
        wanted: bool,
    ) -> Self {
        assert_private(system);
        assert_private(session);
        let (published_sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: tokio::sync::broadcast::channel(64).1,
        };
        let feed = ticker::feed_on(system.connect().await, session.connect().await, root.root());
        let running = Self {
            published,
            demand,
            _config: config,
            task: tokio::spawn(feed.run(context)),
        };
        if wanted {
            running.want();
        }
        running
    }

    fn want(&self) {
        self.demand
            .send_replace(BTreeSet::from([topics::TICKER.to_owned()]));
    }

    /// The first snapshot that satisfies `wanted`.
    async fn snapshot_where(&mut self, wanted: impl Fn(&TickerSnapshot) -> bool) -> TickerSnapshot {
        timeout(BUS_WAIT, async {
            loop {
                let (topic, data) = self.published.recv().await.expect("the feed stopped");
                assert_eq!(topic, topics::TICKER);
                let snapshot: TickerSnapshot = serde_json::from_value(data).unwrap();
                if wanted(&snapshot) {
                    return snapshot;
                }
            }
        })
        .await
        .expect("no matching snapshot in time")
    }

    /// Everything published so far, last last.
    fn drain(&mut self) -> Vec<TickerSnapshot> {
        let mut all = Vec::new();
        while let Ok((_, data)) = self.published.try_recv() {
            all.push(serde_json::from_value(data).unwrap());
        }
        all
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn failed_units_sum_system_and_user() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let system_manager = Systemd::start(&system, SYSTEM_FAILED).await;
    let user_manager = Systemd::start(&session, USER_FAILED).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;

    let snapshot = running.snapshot_where(|s| s.failed_units.is_some()).await;

    assert_eq!(snapshot.failed_units, Some(SYSTEM_FAILED + USER_FAILED));
    assert_eq!(system_manager.calls.count_of("Subscribe"), 1);
    assert_eq!(user_manager.calls.count_of("Subscribe"), 1);
    system_manager.calls.assert_none_could_start_the_service();
    user_manager.calls.assert_none_could_start_the_service();
}

#[tokio::test]
async fn failed_units_refresh_after_a_job_ends() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let system_manager = Systemd::start(&system, SYSTEM_FAILED).await;
    let _user_manager = Systemd::start(&session, USER_FAILED).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;
    running.snapshot_where(|s| s.failed_units == Some(3)).await;

    system_manager.failed.store(4, Ordering::SeqCst);
    system_manager.job_removed().await;

    running.snapshot_where(|s| s.failed_units == Some(6)).await;
}

#[tokio::test]
async fn bursts_of_job_signals_coalesce() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let system_manager = Systemd::start(&system, SYSTEM_FAILED).await;
    let _user_manager = Systemd::start(&session, USER_FAILED).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;
    running.snapshot_where(|s| s.failed_units.is_some()).await;

    for _ in 0..BURST_SIGNALS {
        system_manager.job_removed().await;
        sleep(BURST_GAP).await;
    }
    sleep(COALESCE_SETTLE).await;

    assert!(system_manager.calls.count_of("NFailedUnits") <= 2);
}

#[tokio::test]
async fn firewalld_absent_reads_false() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;

    running.snapshot_where(|s| s.firewalld == Some(false)).await;
}

#[tokio::test]
async fn firewalld_running_reads_true() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let (_firewalld, calls) = start_firewalld(&system).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;

    running.snapshot_where(|s| s.firewalld == Some(true)).await;
    calls.assert_none_could_start_the_service();
}

#[tokio::test]
async fn updates_are_counted_from_the_cached_list() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let packagekit = PackageKit::start(&system, PENDING_UPDATES, false).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;

    running
        .snapshot_where(|s| s.updates == Some(PENDING_UPDATES))
        .await;

    assert_eq!(packagekit.calls.count_of("RefreshCache"), 0);
}

#[tokio::test]
async fn updates_are_recounted_when_packagekit_reports_a_change() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let packagekit = PackageKit::start(&system, PENDING_UPDATES, false).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;
    running
        .snapshot_where(|s| s.updates == Some(PENDING_UPDATES))
        .await;

    packagekit.updates_changed().await;

    timeout(BUS_WAIT, async {
        while packagekit.calls.count_of("GetUpdates") < 2 {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("PackageKit was not asked again");
}

#[tokio::test]
async fn packagekit_error_leaves_updates_unknown() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let packagekit = PackageKit::start(&system, 0, true).await;
    let _system_manager = Systemd::start(&system, SYSTEM_FAILED).await;
    let _user_manager = Systemd::start(&session, USER_FAILED).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;

    let snapshot = running.snapshot_where(|s| s.failed_units.is_some()).await;
    sleep(QUIET).await;

    assert_eq!(packagekit.calls.count_of("CreateTransaction"), 1);
    assert_eq!(snapshot.updates, None);
    assert!(running.drain().iter().all(|s| s.updates.is_none()));
}

#[tokio::test]
async fn nvidia_driver_comes_from_the_module_version() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let root = FakeRoot::new();
    root.write(NVIDIA_VERSION_FILE, &format!("{NVIDIA_VERSION}\n"));
    let mut running = Running::start(&system, &session, &root, true).await;

    running
        .snapshot_where(|s| s.nvidia_driver.as_deref() == Some(NVIDIA_VERSION))
        .await;
}

#[tokio::test]
async fn missing_nvidia_module_leaves_the_driver_unknown() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let _system_manager = Systemd::start(&system, SYSTEM_FAILED).await;
    let _user_manager = Systemd::start(&session, USER_FAILED).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), true).await;

    let snapshot = running.snapshot_where(|s| s.failed_units.is_some()).await;
    sleep(QUIET).await;

    assert_eq!(snapshot.nvidia_driver, None);
    assert!(running.drain().iter().all(|s| s.nvidia_driver.is_none()));
}

#[tokio::test]
async fn ticker_idles_while_unwanted() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let system_manager = Systemd::start(&system, SYSTEM_FAILED).await;
    let user_manager = Systemd::start(&session, USER_FAILED).await;
    let packagekit = PackageKit::start(&system, PENDING_UPDATES, false).await;
    let (_firewalld, firewalld_calls) = start_firewalld(&system).await;
    let mut running = Running::start(&system, &session, &FakeRoot::new(), false).await;

    sleep(QUIET).await;
    assert_eq!(system_manager.calls.count(), 0);
    assert_eq!(user_manager.calls.count(), 0);
    assert_eq!(packagekit.calls.count(), 0);
    assert_eq!(firewalld_calls.count(), 0);
    assert!(running.drain().is_empty());

    running.want();
    running.snapshot_where(|s| s.failed_units.is_some()).await;
}

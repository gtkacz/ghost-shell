mod common;

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus};
use ghost_proto::{LauncherEntries, LauncherEntry, ResolvedConfig, topics};
use ghostd::feeds::launcher_entries;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use zbus::Connection;
use zbus::Variant;

const PATH: &str = "/com/canonical/unity/launcherentry/1";
const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";
const DOLPHIN_URI: &str = "application://org.kde.dolphin.desktop";
const DOLPHIN: &str = "org.kde.dolphin";
const QUIET_FOR: Duration = Duration::from_millis(400);

type Properties = HashMap<&'static str, Variant<'static>>;

struct RunningLauncher {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _task: JoinHandle<()>,
}

impl RunningLauncher {
    fn start(bus: Connection) -> Self {
        let (sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: tokio::sync::broadcast::channel(64).1,
        };
        let task = tokio::spawn(launcher_entries::feed_on(bus).run(context));
        Self {
            published,
            demand,
            _config: config,
            _task: task,
        }
    }

    /// Wants the topic, and returns once the feed has published its empty start.
    async fn want(&mut self) {
        self.demand
            .send(BTreeSet::from([topics::LAUNCHER_ENTRIES.to_owned()]))
            .unwrap();
        assert_eq!(self.next().await, LauncherEntries::default());
    }

    async fn next(&mut self) -> LauncherEntries {
        let (topic, data) = timeout(BUS_WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .unwrap();
        assert_eq!(topic, topics::LAUNCHER_ENTRIES);
        serde_json::from_value(data).unwrap()
    }

    async fn expect_quiet(&mut self) {
        assert!(
            timeout(QUIET_FOR, self.published.recv()).await.is_err(),
            "something was published"
        );
    }
}

async fn update(app: &Connection, uri: &str, properties: Properties) {
    app.emit_signal(None::<&str>, PATH, INTERFACE, "Update", &(uri, properties))
        .await
        .unwrap();
}

fn entry(count: Option<i64>, progress: Option<f64>, urgent: bool) -> LauncherEntry {
    LauncherEntry {
        count,
        progress,
        urgent,
    }
}

fn only(id: &str, entry: LauncherEntry) -> LauncherEntries {
    LauncherEntries {
        entries: [(id.to_owned(), entry)].into(),
    }
}

#[tokio::test]
async fn count_and_progress_are_published_by_desktop_id() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;

    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([
            ("count", Variant::I64(5)),
            ("count-visible", Variant::Bool(true)),
            ("progress", Variant::F64(0.25)),
            ("progress-visible", Variant::Bool(true)),
            ("urgent", Variant::Bool(true)),
        ]),
    )
    .await;

    assert_eq!(
        launcher.next().await,
        only(DOLPHIN, entry(Some(5), Some(0.25), true))
    );
}

#[tokio::test]
async fn progress_is_clamped_to_the_unit_range() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;

    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([
            ("progress", Variant::F64(7.0)),
            ("progress-visible", Variant::Bool(true)),
        ]),
    )
    .await;

    assert_eq!(
        launcher.next().await,
        only(DOLPHIN, entry(None, Some(1.0), false))
    );
}

#[tokio::test]
async fn count_hidden_unless_count_visible() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;

    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([("count", Variant::I64(9))]),
    )
    .await;

    assert_eq!(
        launcher.next().await,
        only(DOLPHIN, entry(None, None, false))
    );
}

#[tokio::test]
async fn later_update_merges_into_the_entry() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;

    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([
            ("progress", Variant::F64(0.5)),
            ("progress-visible", Variant::Bool(true)),
        ]),
    )
    .await;
    launcher.next().await;
    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([
            ("count", Variant::I64(2)),
            ("count-visible", Variant::Bool(true)),
        ]),
    )
    .await;

    assert_eq!(
        launcher.next().await,
        only(DOLPHIN, entry(Some(2), Some(0.5), false))
    );
}

#[tokio::test]
async fn an_update_that_changes_nothing_is_not_published() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;
    let urgent = || HashMap::from([("urgent", Variant::Bool(true))]);

    update(&app, DOLPHIN_URI, urgent()).await;
    launcher.next().await;
    update(&app, DOLPHIN_URI, urgent()).await;

    launcher.expect_quiet().await;
}

#[tokio::test]
async fn entries_of_a_vanished_sender_are_dropped() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;
    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([("urgent", Variant::Bool(true))]),
    )
    .await;
    launcher.next().await;

    drop(app);

    assert_eq!(launcher.next().await, LauncherEntries::default());
}

#[tokio::test]
async fn uri_without_application_scheme_is_ignored() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    launcher.want().await;
    let app = bus.connect().await;

    update(
        &app,
        "file:///usr/share/applications/org.kde.dolphin.desktop",
        HashMap::from([("urgent", Variant::Bool(true))]),
    )
    .await;

    launcher.expect_quiet().await;
}

#[tokio::test]
async fn feed_is_quiet_while_unwanted() {
    let bus = PrivateBus::start();
    let mut launcher = RunningLauncher::start(bus.connect().await);
    let app = bus.connect().await;

    update(
        &app,
        DOLPHIN_URI,
        HashMap::from([("urgent", Variant::Bool(true))]),
    )
    .await;
    launcher.expect_quiet().await;

    launcher.want().await;
}

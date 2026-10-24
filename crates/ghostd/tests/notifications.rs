mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus, next};
use common::notify_client::{Hints, NotifyClient};
use ghost_proto::{
    NotificationServer, NotificationsSnapshot, ResolvedConfig, SessionSnapshot, WindowInfo,
    WindowsSnapshot, topics,
};
use ghostd::feeds::notifications::server::{DISMISSED, NAME};
use ghostd::feeds::notifications::{self, Buses, Paths};
use ghostd::server::{Call, FeedContext, Publisher, TopicUpdate};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use zbus::fdo::{self, RequestNameFlags};
use zbus::proxy::SignalStream;
use zbus::{Array, Connection, Structure, Variant};

const QUIET: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(20);
const CLOCK_STEP_MS: u64 = 2_000;
const BURST: usize = 100;

#[derive(Clone, Copy)]
struct Options {
    marker: bool,
    popup_timeout_s: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            marker: true,
            popup_timeout_s: 6,
        }
    }
}

fn paths_in(dir: &TempDir) -> Paths {
    Paths {
        marker: dir.path().join("state/notifications-server"),
        history: dir.path().join("state/notifications.json"),
        images: dir.path().join("run/notify"),
        boot_id: dir.path().join("state/boot_id"),
    }
}

/// Calls `check` until it holds, so a test reads the feed's side effects without a fixed sleep.
async fn eventually(mut check: impl FnMut() -> bool) {
    timeout(BUS_WAIT, async {
        while !check() {
            sleep(POLL).await;
        }
    })
    .await
    .expect("the condition never held");
}

fn stored(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// The real feed on a private bus standing in for the session bus, and everything it publishes.
struct Running {
    bus: PrivateBus,
    dir: TempDir,
    options: Options,
    initial: NotificationsSnapshot,
    published: mpsc::UnboundedReceiver<(String, Value)>,
    topics: broadcast::Sender<TopicUpdate>,
    calls: broadcast::Sender<Call>,
    ready: Arc<AtomicUsize>,
    notify: NotifyClient,
    task: JoinHandle<()>,
    _demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _client: Connection,
    daemon: Option<Connection>,
}

impl Running {
    async fn start() -> Self {
        Self::start_with(Options::default()).await
    }

    async fn start_with(options: Options) -> Self {
        Self::boot(PrivateBus::start(), tempfile::tempdir().unwrap(), options).await
    }

    async fn boot(bus: PrivateBus, dir: TempDir, options: Options) -> Self {
        let buses = Buses {
            bus: bus.connect().await,
            helper: bus.connect().await,
        };
        Self::boot_with(Some(buses), bus, dir, options).await
    }

    async fn boot_with(
        buses: Option<Buses>,
        bus: PrivateBus,
        dir: TempDir,
        options: Options,
    ) -> Self {
        let paths = paths_in(&dir);
        fs::create_dir_all(paths.marker.parent().unwrap()).unwrap();
        if options.marker {
            fs::write(&paths.marker, "").unwrap();
        }
        let client = bus.connect().await;
        let notify = NotifyClient::new(&client).await;
        let (published_sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) =
            watch::channel(BTreeSet::from([topics::NOTIFICATIONS.to_owned()]));
        let mut resolved = ResolvedConfig::default();
        resolved.notifications.popup_timeout_s = options.popup_timeout_s;
        let (config, config_receiver) = watch::channel(resolved);
        let (calls, calls_receiver) = broadcast::channel(64);
        let (topics_sender, topics_receiver) = broadcast::channel(64);
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: calls_receiver,
            topics: topics_receiver,
        };
        let ready = Arc::new(AtomicUsize::new(0));
        let ready_count = Arc::clone(&ready);
        let clock = Arc::new(AtomicU64::new(CLOCK_STEP_MS));
        let daemon = buses.as_ref().map(|buses| buses.bus.clone());
        let feed = notifications::feed_on(
            buses,
            paths,
            watch::channel(None).0,
            move || clock.fetch_add(CLOCK_STEP_MS, Ordering::SeqCst),
            move || {
                ready_count.fetch_add(1, Ordering::SeqCst);
            },
        );
        let task = tokio::spawn(feed.run(context));
        let mut running = Self {
            bus,
            dir,
            options,
            initial: NotificationsSnapshot::default(),
            published,
            topics: topics_sender,
            calls,
            ready,
            notify,
            task,
            _demand: demand,
            _config: config,
            _client: client,
            daemon,
        };
        running.initial = running.next_snapshot().await;
        running
    }

    async fn next_snapshot(&mut self) -> NotificationsSnapshot {
        let (topic, data) = timeout(BUS_WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .unwrap();
        assert_eq!(topic, topics::NOTIFICATIONS);
        serde_json::from_value(data).unwrap()
    }

    async fn nothing_published(&mut self) {
        sleep(QUIET).await;
        let extra = self.published.try_recv();
        assert!(extra.is_err(), "unexpected publication: {extra:?}");
    }

    fn paths(&self) -> Paths {
        paths_in(&self.dir)
    }

    async fn send(&self, summary: &str) -> u32 {
        self.notify
            .notify("app", summary, "", &[], &Hints::new())
            .await
            .unwrap()
    }

    /// Sends one and returns its id with the snapshot it produced.
    async fn send_and_see(&mut self, summary: &str) -> (u32, NotificationsSnapshot) {
        let id = self.send(summary).await;
        (id, self.next_snapshot().await)
    }

    fn call(&self, method: &str, args: Value) {
        self.calls
            .send(Call {
                method: method.to_owned(),
                args,
            })
            .unwrap();
    }

    fn update(&self, topic: &str, value: Value) {
        self.topics
            .send(TopicUpdate {
                topic: Arc::from(topic),
                value: Arc::new(value),
            })
            .unwrap();
    }

    async fn signals(&self, name: &'static str) -> SignalStream<'static> {
        self.notify.proxy.receive_signal(name).await.unwrap()
    }

    /// Stops the feed, waits for the name to be free, and starts a new one on the same files.
    async fn restart(self) -> Self {
        let Self {
            bus,
            dir,
            options,
            task,
            daemon,
            ..
        } = self;
        task.abort();
        if let Some(daemon) = daemon {
            // The server task keeps a clone of the connection, so the abort alone leaves the name held.
            daemon.close().await.unwrap();
        }
        let _ = task.await;
        let watcher = bus.connect().await;
        let proxy = fdo::DBusProxy::new(&watcher).await.unwrap();
        let name = NAME.try_into().unwrap();
        timeout(BUS_WAIT, async {
            while proxy.name_has_owner(Clone::clone(&name)).await.unwrap() {
                sleep(POLL).await;
            }
        })
        .await
        .expect("the stopped feed kept the name");
        Self::boot(bus, dir, options).await
    }
}

fn dnd_windows(fullscreen: bool) -> Value {
    serde_json::to_value(WindowsSnapshot {
        windows: vec![WindowInfo {
            active: true,
            fullscreen,
            ..WindowInfo::default()
        }],
    })
    .unwrap()
}

async fn holder(bus: &PrivateBus) -> Connection {
    let connection = bus.connect().await;
    connection
        .request_name_with_flags(NAME, RequestNameFlags::DO_NOT_QUEUE)
        .await
        .unwrap();
    connection
}

#[tokio::test]
async fn notify_publishes_the_record_unread() {
    let mut feed = Running::start().await;
    assert_eq!(feed.initial.server, NotificationServer::GhostShell);
    let (id, snapshot) = feed.send_and_see("hello").await;
    assert_eq!(snapshot.items.len(), 1);
    let record = &snapshot.items[0];
    assert_eq!(
        (record.id, record.summary.as_str(), record.read),
        (id, "hello", false)
    );
    assert_eq!((snapshot.unread, snapshot.unread_crit), (1, 0));
    assert_eq!(snapshot.suppressed, "");
}

#[tokio::test]
async fn a_burst_of_notifies_publishes_once_per_record() {
    let mut feed = Running::start().await;
    for n in 0..BURST {
        feed.send(&format!("n{n}")).await;
    }
    for _ in 0..BURST {
        feed.next_snapshot().await;
    }
    feed.nothing_published().await;
}

#[tokio::test]
async fn invoke_emits_action_invoked_and_closes_unless_resident() {
    let mut feed = Running::start().await;
    let mut invoked = feed.signals("ActionInvoked").await;
    let mut closed = feed.signals("NotificationClosed").await;
    let actions = ["default", "Open", "reply", "Reply"];
    let id = feed
        .notify
        .notify("app", "plain", "", &actions, &Hints::new())
        .await
        .unwrap();
    feed.next_snapshot().await;

    feed.call("notifications.invoke", json!({"id": id, "key": "reply"}));
    let message = next(&mut invoked).await.unwrap();
    assert_eq!(
        message.body().deserialize::<(u32, String)>().unwrap(),
        (id, "reply".to_owned())
    );
    let message = next(&mut closed).await.unwrap();
    assert_eq!(
        message.body().deserialize::<(u32, u32)>().unwrap(),
        (id, DISMISSED)
    );
    assert!(feed.next_snapshot().await.items.is_empty());

    let mut hints = Hints::new();
    hints.insert("resident", Variant::from(true));
    let resident = feed
        .notify
        .notify("app", "stays", "", &actions, &hints)
        .await
        .unwrap();
    feed.next_snapshot().await;
    feed.call(
        "notifications.invoke",
        json!({"id": resident, "key": "default"}),
    );
    let message = next(&mut invoked).await.unwrap();
    assert_eq!(
        message.body().deserialize::<(u32, String)>().unwrap(),
        (resident, "default".to_owned())
    );
    feed.nothing_published().await;
    assert!(timeout(QUIET, next(&mut closed)).await.is_err());
}

#[tokio::test]
async fn invoke_on_unknown_id_is_ignored() {
    let mut feed = Running::start().await;
    let mut invoked = feed.signals("ActionInvoked").await;
    feed.call("notifications.invoke", json!({"id": 99, "key": "default"}));
    feed.call("notifications.invoke", json!({"id": "x"}));
    feed.call("notifications.dismiss", json!({"id": 99}));
    assert!(timeout(QUIET, next(&mut invoked)).await.is_err());
    feed.nothing_published().await;
    feed.send_and_see("still alive").await;
}

#[tokio::test]
async fn invoke_with_unoffered_key_is_ignored() {
    let mut feed = Running::start().await;
    let mut invoked = feed.signals("ActionInvoked").await;
    let id = feed
        .notify
        .notify("app", "no default", "", &["reply", "Reply"], &Hints::new())
        .await
        .unwrap();
    feed.next_snapshot().await;
    feed.call("notifications.invoke", json!({"id": id, "key": "default"}));
    feed.call("notifications.invoke", json!({"id": id, "key": "other"}));
    assert!(timeout(QUIET, next(&mut invoked)).await.is_err());
    feed.nothing_published().await;
}

#[tokio::test]
async fn dismiss_clear_read_and_dnd_change_the_topic() {
    let mut feed = Running::start().await;
    let mut closed = feed.signals("NotificationClosed").await;
    let (first, _) = feed.send_and_see("a").await;
    let (second, _) = feed.send_and_see("b").await;
    let (third, snapshot) = feed.send_and_see("c").await;
    assert_eq!(snapshot.unread, 3);

    feed.call("notifications.read", json!({"ids": [first]}));
    assert_eq!(feed.next_snapshot().await.unread, 2);
    feed.call("notifications.read", json!({"all": true}));
    assert_eq!(feed.next_snapshot().await.unread, 0);

    feed.call("notifications.dnd", json!({"on": true}));
    let snapshot = feed.next_snapshot().await;
    assert!(snapshot.dnd);
    assert_eq!(snapshot.suppressed, "dnd");

    feed.call("notifications.dismiss", json!({"id": second}));
    let message = next(&mut closed).await.unwrap();
    assert_eq!(
        message.body().deserialize::<(u32, u32)>().unwrap(),
        (second, DISMISSED)
    );
    let ids: Vec<u32> = feed
        .next_snapshot()
        .await
        .items
        .iter()
        .map(|item| item.id)
        .collect();
    assert_eq!(ids, [third, first]);

    feed.call("notifications.clear", json!({}));
    assert!(feed.next_snapshot().await.items.is_empty());
    let mut cleared = Vec::new();
    for _ in 0..2 {
        let message = next(&mut closed).await.unwrap();
        cleared.push(message.body().deserialize::<(u32, u32)>().unwrap());
    }
    cleared.sort_unstable();
    assert_eq!(cleared, [(first, DISMISSED), (third, DISMISSED)]);
}

#[tokio::test]
async fn history_survives_a_restart() {
    let mut feed = Running::start().await;
    let (first, _) = feed.send_and_see("a").await;
    let (second, _) = feed.send_and_see("b").await;
    feed.call("notifications.dnd", json!({"on": true}));
    feed.next_snapshot().await;
    let history = feed.paths().history;
    eventually(|| stored(&history).is_some_and(|file| file["dnd"] == true)).await;
    let before = stored(&history).unwrap();
    assert_eq!(before["items"].as_array().unwrap().len(), 2);

    let mut feed = feed.restart().await;
    let ids: Vec<u32> = feed.initial.items.iter().map(|item| item.id).collect();
    assert_eq!(ids, [second, first]);
    assert!(feed.initial.dnd);
    let (third, _) = feed.send_and_see("c").await;
    assert_eq!(third, second + 1);
}

#[tokio::test]
async fn transient_record_passes_through_items_and_the_file() {
    let mut feed = Running::start_with(Options {
        popup_timeout_s: 1,
        ..Options::default()
    })
    .await;
    let mut hints = Hints::new();
    hints.insert("transient", Variant::from(true));
    let transient = feed
        .notify
        .notify("app", "passing", "", &[], &hints)
        .await
        .unwrap();
    let snapshot = feed.next_snapshot().await;
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].id, transient);

    let (kept, snapshot) = feed.send_and_see("kept").await;
    let ids: Vec<u32> = snapshot.items.iter().map(|item| item.id).collect();
    assert_eq!(
        ids,
        [transient, kept],
        "the passing record sits in front only while it passes"
    );

    let snapshot = feed.next_snapshot().await;
    let ids: Vec<u32> = snapshot.items.iter().map(|item| item.id).collect();
    assert_eq!(ids, [kept]);

    let history = feed.paths().history;
    eventually(|| stored(&history).is_some()).await;
    let file = stored(&history).unwrap();
    let ids: Vec<u64> = file["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, [u64::from(kept)]);
}

#[tokio::test]
async fn fullscreen_active_window_holds_new_records() {
    let mut feed = Running::start().await;
    feed.update(topics::WINDOWS, dnd_windows(true));
    assert_eq!(feed.next_snapshot().await.suppressed, "fullscreen");
    let (_, snapshot) = feed.send_and_see("quiet").await;
    assert!(snapshot.items[0].held);
    assert_eq!(snapshot.suppressed, "fullscreen");

    feed.update(topics::WINDOWS, dnd_windows(false));
    assert_eq!(feed.next_snapshot().await.suppressed, "");
    let (_, snapshot) = feed.send_and_see("loud").await;
    assert!(!snapshot.items[0].held);
}

#[tokio::test]
async fn locked_session_holds_new_records() {
    let mut feed = Running::start().await;
    let locked = |locked| {
        serde_json::to_value(SessionSnapshot {
            locked,
            ..SessionSnapshot::default()
        })
        .unwrap()
    };
    feed.update(topics::SESSION, locked(true));
    assert_eq!(feed.next_snapshot().await.suppressed, "locked");
    let (_, snapshot) = feed.send_and_see("quiet").await;
    assert!(snapshot.items[0].held);

    feed.update(topics::SESSION, locked(false));
    assert_eq!(feed.next_snapshot().await.suppressed, "");
}

#[tokio::test]
async fn an_inhibitor_holds_new_records() {
    let mut feed = Running::start().await;
    let cookie: u32 = feed
        .notify
        .proxy
        .call("Inhibit", &("app", "reason", Hints::new()))
        .await
        .unwrap();
    let snapshot = feed.next_snapshot().await;
    assert!((snapshot.inhibited, snapshot.suppressed.as_str()) == (true, "inhibited"));
    let (_, snapshot) = feed.send_and_see("quiet").await;
    assert!(snapshot.items[0].held);

    feed.notify
        .proxy
        .call::<_, _, ()>("UnInhibit", &(cookie,))
        .await
        .unwrap();
    assert!(!feed.next_snapshot().await.inhibited);
}

#[tokio::test]
async fn without_the_marker_server_reads_plasma_or_none() {
    let bus = PrivateBus::start();
    let plasma = holder(&bus).await;
    let dir = tempfile::tempdir().unwrap();
    let options = Options {
        marker: false,
        ..Options::default()
    };
    let mut feed = Running::boot(bus, dir, options).await;
    assert_eq!(feed.initial.server, NotificationServer::Plasma);

    plasma.release_name(NAME).await.unwrap();
    assert_eq!(feed.next_snapshot().await.server, NotificationServer::None);

    let again = holder(&feed.bus).await;
    assert_eq!(
        feed.next_snapshot().await.server,
        NotificationServer::Plasma
    );
    drop(again);
}

#[tokio::test]
async fn without_the_marker_the_name_is_never_requested() {
    let mut feed = Running::start_with(Options {
        marker: false,
        ..Options::default()
    })
    .await;
    assert_eq!(feed.initial.server, NotificationServer::None);
    let watcher = feed.bus.connect().await;
    let proxy = fdo::DBusProxy::new(&watcher).await.unwrap();
    assert!(
        !proxy
            .name_has_owner(NAME.try_into().unwrap())
            .await
            .unwrap()
    );
    feed.nothing_published().await;
}

#[tokio::test]
async fn with_the_marker_the_name_is_owned_and_ready_sent() {
    let feed = Running::start().await;
    assert_eq!(feed.initial.server, NotificationServer::GhostShell);
    let watcher = feed.bus.connect().await;
    let proxy = fdo::DBusProxy::new(&watcher).await.unwrap();
    assert!(
        proxy
            .name_has_owner(NAME.try_into().unwrap())
            .await
            .unwrap()
    );
    eventually(|| feed.ready.load(Ordering::SeqCst) == 1).await;
    assert_eq!(feed.send("works").await, 1);
}

#[tokio::test]
async fn queued_behind_plasma_then_inherits_the_name() {
    let bus = PrivateBus::start();
    let plasma = holder(&bus).await;
    let dir = tempfile::tempdir().unwrap();
    let mut feed = Running::boot(bus, dir, Options::default()).await;
    assert_eq!(feed.initial.server, NotificationServer::Plasma);
    eventually(|| feed.ready.load(Ordering::SeqCst) == 1).await;

    drop(plasma);
    assert_eq!(
        feed.next_snapshot().await.server,
        NotificationServer::GhostShell
    );
}

#[tokio::test]
async fn ready_is_sent_without_a_session_bus() {
    let bus = PrivateBus::start();
    let mut feed =
        Running::boot_with(None, bus, tempfile::tempdir().unwrap(), Options::default()).await;
    assert_eq!(feed.initial.server, NotificationServer::None);
    eventually(|| feed.ready.load(Ordering::SeqCst) == 1).await;
    feed.call("notifications.dnd", json!({"on": true}));
    assert!(feed.next_snapshot().await.dnd);
    assert_eq!(feed.ready.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn ready_is_sent_without_the_marker() {
    let feed = Running::start_with(Options {
        marker: false,
        ..Options::default()
    })
    .await;
    eventually(|| feed.ready.load(Ordering::SeqCst) == 1).await;
    sleep(QUIET).await;
    assert_eq!(feed.ready.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn images_are_pruned_with_the_history() {
    let mut feed = Running::start().await;
    let images = feed.paths().images;
    fs::create_dir_all(&images).unwrap();
    let stray = images.join("99.png");
    fs::write(&stray, b"x").unwrap();
    feed.send_and_see("pruner").await;
    eventually(|| !stray.exists()).await;
}

#[tokio::test]
async fn image_data_becomes_a_png_after_the_reply() {
    let mut feed = Running::start().await;
    let pixel = Variant::Structure(
        Structure::builder()
            .add_field(1i32)
            .add_field(1i32)
            .add_field(3i32)
            .add_field(false)
            .add_field(8i32)
            .add_field(3i32)
            .add_field(Array::from(vec![1u8, 2, 3]))
            .build()
            .unwrap(),
    );
    let mut hints = Hints::new();
    hints.insert("image-data", pixel);
    let id = feed
        .notify
        .notify("app", "picture", "", &[], &hints)
        .await
        .unwrap();
    let snapshot = loop {
        let snapshot = feed.next_snapshot().await;
        if !snapshot.items[0].image.is_empty() {
            break snapshot;
        }
    };
    let expected = feed.paths().images.join(format!("{id}.png"));
    assert_eq!(snapshot.items[0].image, expected.to_str().unwrap());
    assert!(expected.exists());
}

mod common;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus};
use common::notify_client::{Hints, NotifyClient};
use ghost_proto::{
    AlertLevel, DiskSnapshot, Drive, DriveState, ResolvedConfig, WeatherAlert, WeatherAlerts,
    topics,
};
use ghostd::feeds::notifications::server::NAME;
use ghostd::feeds::notifications::{self, Buses, Paths};
use ghostd::server::{Call, FeedContext, Publisher, TopicUpdate};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use zbus::{Connection, OwnedVariant, interface};

const CLOCK_STEP_MS: u64 = 2_000;
const FIRST_ID: u32 = 10;
const CRITICAL: u8 = 2;
const NORMAL: u8 = 1;
const QUIET: Duration = Duration::from_millis(300);

#[derive(Debug, PartialEq)]
struct Seen {
    app: String,
    replaces_id: u32,
    icon: String,
    summary: String,
    body: String,
    urgency: u8,
    level: Option<String>,
}

/// Takes the place of Plasma's notification server and records what it is sent.
struct FakeServer {
    seen: mpsc::UnboundedSender<Seen>,
    next_id: u32,
}

#[interface(name = "org.freedesktop.Notifications")]
impl FakeServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        _timeout: i32,
    ) -> u32 {
        let urgency = hints
            .get("urgency")
            .and_then(|value| u8::try_from(value).ok())
            .expect("every call carries an urgency");
        let level = hints
            .get("x-ghost-shell-level")
            .and_then(|value| String::try_from(value.try_clone().ok()?).ok());
        self.seen
            .send(Seen {
                app: app_name,
                replaces_id,
                icon: app_icon,
                summary,
                body,
                urgency,
                level,
            })
            .unwrap();
        if replaces_id != 0 {
            return replaces_id;
        }
        self.next_id += 1;
        self.next_id
    }
}

async fn fake_server(bus: &PrivateBus) -> (Connection, mpsc::UnboundedReceiver<Seen>) {
    let (seen, receiver) = mpsc::unbounded_channel();
    let connection = bus
        .builder()
        .name(NAME)
        .serve_at(
            "/org/freedesktop/Notifications",
            FakeServer {
                seen,
                next_id: FIRST_ID - 1,
            },
        )
        .build()
        .await
        .unwrap();
    (connection, receiver)
}

fn paths_in(dir: &Path) -> Paths {
    Paths {
        marker: dir.join("state/notifications-server"),
        history: dir.join("state/notifications.json"),
        images: dir.join("run/notify"),
        boot_id: dir.join("state/boot_id"),
    }
}

/// The real feed on a private bus standing in for the session bus.
struct Feed {
    topics: broadcast::Sender<TopicUpdate>,
    calls: broadcast::Sender<Call>,
    signals: watch::Receiver<Option<u32>>,
    task: JoinHandle<()>,
    _demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
}

impl Feed {
    /// Starts it over the state in `dir` and returns once it has published its first snapshot.
    async fn start(bus: &PrivateBus, dir: &Path, boot_id: &str, marker: bool) -> Self {
        let paths = paths_in(dir);
        fs::create_dir_all(paths.marker.parent().unwrap()).unwrap();
        fs::write(&paths.boot_id, boot_id).unwrap();
        if marker {
            fs::write(&paths.marker, "").unwrap();
        }
        let buses = Buses {
            bus: bus.connect().await,
            helper: bus.connect().await,
        };
        let (published_sender, mut published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let (calls, calls_receiver) = broadcast::channel(64);
        let (topics_sender, topics_receiver) = broadcast::channel(64);
        let (signals_sender, signals) = watch::channel(None);
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                let _ = published_sender.send((topic.to_owned(), data));
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: calls_receiver,
            topics: topics_receiver,
        };
        let clock = Arc::new(AtomicU64::new(CLOCK_STEP_MS));
        let feed = notifications::feed_on(
            Some(buses),
            paths,
            signals_sender,
            move || clock.fetch_add(CLOCK_STEP_MS, Ordering::SeqCst),
            || {},
        );
        let task = tokio::spawn(feed.run(context));
        let first: Option<(String, Value)> = timeout(BUS_WAIT, published.recv()).await.unwrap();
        assert_eq!(first.unwrap().0, topics::NOTIFICATIONS);
        Self {
            topics: topics_sender,
            calls,
            signals,
            task,
            _demand: demand,
            _config: config,
        }
    }

    fn update(&self, topic: &str, value: Value) {
        self.topics
            .send(TopicUpdate {
                topic: Arc::from(topic),
                value: Arc::new(value),
            })
            .unwrap();
    }

    fn weather(&self, alerts: Vec<WeatherAlert>) {
        let alerts = WeatherAlerts {
            source: "INMET".to_owned(),
            updated_s: 0,
            alerts,
        };
        self.update(
            topics::WEATHER_ALERTS,
            serde_json::to_value(alerts).unwrap(),
        );
    }

    fn disks(&self, drives: Vec<Drive>) {
        self.update(
            topics::DISKS,
            serde_json::to_value(DiskSnapshot { drives }).unwrap(),
        );
    }

    fn session(&self, locked: bool) {
        self.update(topics::SESSION, json!({ "locked": locked }));
    }

    async fn signals_become(&mut self, wanted: Option<u32>) {
        timeout(BUS_WAIT, async {
            while *self.signals.borrow_and_update() != wanted {
                self.signals.changed().await.unwrap();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("signals never became {wanted:?}"));
    }

    fn stop(self) {
        self.task.abort();
    }
}

async fn next_seen(seen: &mut mpsc::UnboundedReceiver<Seen>) -> Seen {
    timeout(BUS_WAIT, seen.recv())
        .await
        .expect("no notification was sent in time")
        .unwrap()
}

fn alert(id: &str, title: &str, level: AlertLevel, risks: &[&str]) -> WeatherAlert {
    WeatherAlert {
        id: id.to_owned(),
        title: title.to_owned(),
        severity: "YELLOW".to_owned(),
        original_title: None,
        original_severity: None,
        auto_translated: false,
        level,
        start_s: 0,
        end_s: 0,
        risks: risks.iter().map(|risk| (*risk).to_owned()).collect(),
    }
}

fn drive(id: &str, model: &str, state: DriveState) -> Drive {
    Drive {
        id: id.to_owned(),
        model: model.to_owned(),
        state,
        ..Drive::default()
    }
}

#[tokio::test]
async fn new_inmet_alert_is_notified_once_with_its_level() {
    let bus = PrivateBus::start();
    let dir = TempDir::new().unwrap();
    let (_server, mut seen) = fake_server(&bus).await;
    let feed = Feed::start(&bus, dir.path(), "boot", false).await;
    let storm = alert("a1", "STORM", AlertLevel::Warn, &["WIND", "HAIL"]);

    feed.weather(vec![storm.clone()]);

    assert_eq!(
        next_seen(&mut seen).await,
        Seen {
            app: "WEATHER".to_owned(),
            replaces_id: 0,
            icon: "weather-storm".to_owned(),
            summary: "STORM · YELLOW".to_owned(),
            body: "WIND · HAIL".to_owned(),
            urgency: NORMAL,
            level: Some("warn".to_owned()),
        }
    );
    // The alert again, then a critical one: only the critical one is new.
    feed.weather(vec![
        storm,
        alert("a2", "FLOOD", AlertLevel::Crit, &["RIVER"]),
    ]);
    let critical = next_seen(&mut seen).await;
    assert_eq!(
        (critical.summary.as_str(), critical.urgency, critical.level),
        ("FLOOD · YELLOW", CRITICAL, None)
    );
}

#[tokio::test]
async fn updated_alert_replaces_its_notification() {
    let bus = PrivateBus::start();
    let dir = TempDir::new().unwrap();
    let (_server, mut seen) = fake_server(&bus).await;
    let feed = Feed::start(&bus, dir.path(), "boot", false).await;
    feed.weather(vec![alert("a1", "STORM", AlertLevel::Warn, &["WIND"])]);
    next_seen(&mut seen).await;

    // Sent at once, before the first send's answer need have reached the feed.
    feed.weather(vec![alert(
        "a1",
        "STORM",
        AlertLevel::Crit,
        &["WIND", "HAIL"],
    )]);

    let update = next_seen(&mut seen).await;
    assert_eq!(
        (update.replaces_id, update.body.as_str(), update.urgency),
        (FIRST_ID, "WIND · HAIL", CRITICAL)
    );
    // An alert that did not change is not sent again.
    feed.weather(vec![
        alert("a1", "STORM", AlertLevel::Crit, &["WIND", "HAIL"]),
        alert("a2", "FLOOD", AlertLevel::Warn, &[]),
    ]);
    let next = next_seen(&mut seen).await;
    assert_eq!(
        (next.replaces_id, next.summary.as_str()),
        (0, "FLOOD · YELLOW")
    );
}

#[tokio::test]
async fn an_alert_is_not_raised_again_after_a_restart_until_it_has_gone() {
    let bus = PrivateBus::start();
    let dir = TempDir::new().unwrap();
    let (_server, mut seen) = fake_server(&bus).await;
    let feed = Feed::start(&bus, dir.path(), "boot", false).await;
    feed.weather(vec![alert("a1", "STORM", AlertLevel::Warn, &[])]);
    next_seen(&mut seen).await;
    // The history is saved a moment after the change.
    let history = paths_in(dir.path()).history;
    timeout(BUS_WAIT, async {
        while !fs::read_to_string(&history).is_ok_and(|text| text.contains("inmet:a1")) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the raised alert was never saved");
    feed.stop();

    let feed = Feed::start(&bus, dir.path(), "boot", false).await;
    feed.weather(vec![alert("a1", "STORM", AlertLevel::Warn, &[])]);
    feed.weather(vec![alert("a2", "FLOOD", AlertLevel::Warn, &[])]);
    assert_eq!(next_seen(&mut seen).await.summary, "FLOOD · YELLOW");
    // a1 is gone from the latest list, so it is new again if it comes back.
    feed.weather(vec![alert("a1", "STORM", AlertLevel::Warn, &[])]);
    assert_eq!(next_seen(&mut seen).await.summary, "STORM · YELLOW");
}

#[tokio::test]
async fn dead_drive_is_notified_once_per_start() {
    let bus = PrivateBus::start();
    let dir = TempDir::new().unwrap();
    let (_server, mut seen) = fake_server(&bus).await;
    let feed = Feed::start(&bus, dir.path(), "boot-a", false).await;
    let live = drive("live#1", "Fine", DriveState::Live);
    let dead = drive("nvme#S1", "Samsung SSD 990", DriveState::Dead);
    let other = drive("sata#S2", "WD Blue", DriveState::Dead);

    feed.disks(vec![live.clone(), dead.clone()]);

    assert_eq!(
        next_seen(&mut seen).await,
        Seen {
            app: "STORAGE".to_owned(),
            replaces_id: 0,
            icon: String::new(),
            summary: "SAMSUNG SSD 990 · CONTROLLER STATE DEAD".to_owned(),
            body: String::new(),
            urgency: CRITICAL,
            level: None,
        }
    );
    // The same drive again, then another one: only the other one is new.
    feed.disks(vec![live, dead.clone(), other.clone()]);
    assert_eq!(
        next_seen(&mut seen).await.summary,
        "WD BLUE · CONTROLLER STATE DEAD"
    );
    let history = paths_in(dir.path()).history;
    timeout(BUS_WAIT, async {
        while !fs::read_to_string(&history).is_ok_and(|text| text.contains("sata#S2:boot-a")) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the raised drives were never saved");
    feed.stop();

    // A restart in the same boot stays quiet; a new boot raises the drive again.
    let feed = Feed::start(&bus, dir.path(), "boot-a", false).await;
    feed.disks(vec![dead.clone(), other.clone()]);
    sleep(QUIET).await;
    assert!(
        seen.try_recv().is_err(),
        "a drive was raised again in its boot"
    );
    feed.stop();
    let feed = Feed::start(&bus, dir.path(), "boot-b", false).await;
    feed.disks(vec![dead]);
    assert_eq!(
        next_seen(&mut seen).await.summary,
        "SAMSUNG SSD 990 · CONTROLLER STATE DEAD"
    );
}

#[tokio::test]
async fn signals_counts_unread_arrivals_since_lock() {
    let bus = PrivateBus::start();
    let dir = TempDir::new().unwrap();
    let mut feed = Feed::start(&bus, dir.path(), "boot", true).await;
    let client = bus.connect().await;
    let notify = NotifyClient::new(&client).await;
    let send = async |summary: &str| notify.notify("app", summary, "", &[], &Hints::new()).await;
    feed.signals_become(None).await;
    send("before the lock").await.unwrap();

    feed.session(true);
    feed.signals_become(Some(0)).await;
    send("first").await.unwrap();
    let second = send("second").await.unwrap();
    feed.signals_become(Some(2)).await;
    feed.calls
        .send(Call {
            method: "notifications.read".to_owned(),
            args: json!({ "ids": [second] }),
        })
        .unwrap();
    feed.signals_become(Some(1)).await;

    feed.session(false);
    feed.signals_become(None).await;
}

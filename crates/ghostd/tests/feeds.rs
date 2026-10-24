mod common;

use std::collections::BTreeSet;
use std::future::pending;
use std::time::Duration;

use common::FakeRoot;
use common::server::*;
use ghost_proto::{HostSnapshot, ResolvedConfig, topics};
use ghostd::fake;
use ghostd::feeds::{self, host};
use ghostd::server::{Feed, FeedContext, FeedEnd, Publisher, ServeError};
use serde_json::{Value, json};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep, timeout, timeout_at};

const WATCHED_TICKS: u64 = 6;
const NEW_BUILTINS: [&str; 19] = [
    topics::HOST,
    topics::DESKTOP,
    topics::WEATHER,
    topics::SKY,
    topics::SPECTRUM,
    topics::PROCS,
    topics::PROCS_TREE,
    topics::NET_LINKS,
    topics::NET,
    topics::DISKS_HEALTH,
    topics::BATTERIES,
    topics::UPLINK,
    topics::SESSION,
    topics::WINDOWS,
    topics::LAUNCHER_ENTRIES,
    topics::TICKER,
    topics::RECENT,
    topics::AGENTS_USAGE,
    topics::AGENTS_SESSIONS,
];
const BOOT_TIME_S: u64 = 1_792_500_000;
const RETURNING_FEED: &str = "returning";
const PANICKING_FEED: &str = "panicking";

fn ticks(count: u64) -> Duration {
    Duration::from_millis(u64::from(TICK_MS) * count)
}

fn subscribe(topics: &[&str]) -> Value {
    json!({"t": "subscribe", "topics": topics})
}

/// The topic of every message `client` receives within `window`, `""` for one without a topic.
async fn topics_within(client: &mut Client, window: Duration) -> Vec<String> {
    let deadline = Instant::now() + window;
    let mut seen = Vec::new();
    while let Ok(line) = timeout_at(deadline, client.lines.next_line()).await {
        let line = line
            .expect("socket read failed")
            .expect("server closed the connection");
        let message: Value = serde_json::from_str(&line).unwrap();
        seen.push(message["topic"].as_str().unwrap_or_default().to_owned());
    }
    seen
}

fn topic_set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|&name| name.to_owned()).collect()
}

/// Forwards every demand it sees, starting with the one in place when it starts.
fn forwarding_feed(demands: mpsc::UnboundedSender<BTreeSet<String>>) -> Feed {
    Feed::new("forwarding", move |mut context: FeedContext| async move {
        loop {
            let current = context.demand.borrow_and_update().clone();
            if demands.send(current).is_err() || context.demand.changed().await.is_err() {
                pending::<()>().await;
            }
        }
    })
}

async fn expect_demand(demands: &mut mpsc::UnboundedReceiver<BTreeSet<String>>, names: &[&str]) {
    let expected = topic_set(names);
    // Changes that land between two polls of the feed reach it as one, so only the last counts.
    timeout(WAIT, async {
        while demands.recv().await.expect("the feed stopped") != expected {}
    })
    .await
    .unwrap_or_else(|_| panic!("the demand never became {expected:?}"));
}

fn sample_host() -> HostSnapshot {
    HostSnapshot {
        hostname: "box".to_owned(),
        kernel: "7.2.11".to_owned(),
        boot_time_s: BOOT_TIME_S,
    }
}

/// Publishes `host` once, as soon as some client wants it.
fn host_when_wanted() -> Feed {
    Feed::new("host when wanted", |mut context: FeedContext| async move {
        feeds::until_wanted(&mut context.demand, topics::HOST).await;
        context.publisher.publish(topics::HOST, &sample_host());
        pending::<()>().await;
    })
}

fn returning_feed() -> Feed {
    Feed::new(RETURNING_FEED, |_| async {})
}

fn panicking_feed() -> Feed {
    Feed::new(PANICKING_FEED, |_| async { panic!("the feed hit a bug") })
}

/// Runs `feed` on its own and returns the first value it publishes.
async fn first_publish(feed: Feed) -> (String, Value) {
    let (published, mut values) = mpsc::unbounded_channel();
    let (_demand, demand) = watch::channel(BTreeSet::new());
    let (_config, config) = watch::channel(ResolvedConfig::default());
    let context = FeedContext {
        publisher: Publisher::new(move |topic, data| {
            published.send((topic.to_owned(), data)).unwrap();
        }),
        demand,
        config,
        calls: tokio::sync::broadcast::channel(64).1,
    };
    let _running = tokio::spawn(feed.run(context));
    timeout(WAIT, values.recv())
        .await
        .expect("the feed published nothing in time")
        .unwrap()
}

#[tokio::test]
async fn unsubscribed_builtins_are_not_published() {
    let daemon = Daemon::start().await;
    let mut ui = daemon.ui().await;

    ui.send(subscribe(&[topics::MEM])).await;
    assert_eq!(ui.recv().await["t"], "history");
    let seen = topics_within(&mut ui, ticks(WATCHED_TICKS)).await;

    assert!(
        seen.iter().filter(|&topic| topic == topics::MEM).count() >= 2,
        "got {seen:?}"
    );
    assert!(
        seen.iter().all(|topic| topic == topics::MEM),
        "got {seen:?}"
    );
    // Any cpu or gpu sample would have left a series behind for a later subscriber.
    let mut late = daemon.ui().await;
    late.send(subscribe(&[topics::CPU, topics::GPU])).await;
    let history = late.recv().await;
    assert_eq!(history["t"], "history");
    assert_eq!(history["series"], json!({}));
}

#[tokio::test]
async fn demand_is_the_union_of_ui_clients() {
    let (forwarded, mut demands) = mpsc::unbounded_channel();
    let daemon = Daemon::start_with_feeds(vec![forwarding_feed(forwarded)]).await;

    let mut first = daemon.ui().await;
    first.send(subscribe(&[topics::CPU])).await;
    let mut second = daemon.ui().await;
    second.send(subscribe(&[topics::GPU, topics::HOST])).await;
    expect_demand(&mut demands, &[topics::CPU, topics::GPU, topics::HOST]).await;

    drop(second);
    expect_demand(&mut demands, &[topics::CPU]).await;
}

#[tokio::test]
async fn feed_output_reaches_subscribers_and_late_joiners() {
    let daemon = Daemon::start_with_feeds(vec![host_when_wanted()]).await;

    let mut first = daemon.ui().await;
    first.send(subscribe(&[topics::HOST])).await;
    let live = first.recv_data(topics::HOST).await;
    assert_eq!(live["seq"], 1);
    assert_eq!(live["data"], serde_json::to_value(sample_host()).unwrap());

    let mut late = daemon.ui().await;
    late.send(subscribe(&[topics::HOST])).await;
    let cached = late.recv().await;
    assert_eq!(cached["t"], "data");
    assert_eq!(cached["topic"], topics::HOST);
    assert_eq!(cached["data"], live["data"]);
}

#[tokio::test]
async fn providers_cannot_claim_new_builtin_names() {
    let daemon = Daemon::start().await;

    for name in NEW_BUILTINS {
        let mut thief = daemon.provider("thief", &[name]).await;
        thief
            .expect_error(&format!("topic {name:?} is built in"))
            .await;
    }

    let mut witness = daemon.subscribed_ui(&[topics::WEATHER_ALERTS]).await;
    let mut inmet = daemon.provider("inmet", &[topics::WEATHER_ALERTS]).await;
    inmet
        .send(json!({"t": "publish", "topic": topics::WEATHER_ALERTS, "data": {"n": 1}}))
        .await;
    assert_eq!(
        witness.recv_data(topics::WEATHER_ALERTS).await["data"]["n"],
        1
    );
}

#[tokio::test]
async fn host_feed_reads_the_fake_root() {
    let fake = FakeRoot::new();
    fake.write("proc/sys/kernel/hostname", "box\n");
    fake.write("proc/sys/kernel/osrelease", "7.2.11\n");
    fake.write(
        "proc/stat",
        &format!("cpu 1 2 3 4 0 0 0 0 0 0\nintr 0\nctxt 0\nbtime {BOOT_TIME_S}\nprocesses 1\n"),
    );

    let (topic, data) = first_publish(host::feed(fake.root())).await;

    assert_eq!(topic, topics::HOST);
    assert_eq!(
        serde_json::from_value::<HostSnapshot>(data).unwrap(),
        sample_host()
    );
}

#[tokio::test]
async fn host_feed_reports_what_it_cannot_read_as_empty() {
    let fake = FakeRoot::new();

    let (_, data) = first_publish(host::feed(fake.root())).await;

    assert_eq!(
        serde_json::from_value::<HostSnapshot>(data).unwrap(),
        HostSnapshot::default()
    );
}

#[tokio::test]
async fn serving_stops_with_an_error_when_a_feed_returns() {
    let mut daemon = Daemon::start_with_feeds(vec![returning_feed()]).await;

    let stopped = timeout(WAIT, &mut daemon.serve).await;

    let error = stopped
        .expect("the daemon kept serving after a feed returned")
        .unwrap();
    assert!(
        matches!(
            error,
            ServeError::Feed {
                name: RETURNING_FEED,
                end: FeedEnd::Returned
            }
        ),
        "{error}"
    );
}

#[tokio::test]
async fn serving_stops_with_an_error_when_a_feed_panics() {
    let mut daemon = Daemon::start_with_feeds(vec![panicking_feed()]).await;

    let stopped = timeout(WAIT, &mut daemon.serve).await;

    let error = stopped
        .expect("the daemon kept serving after a feed panicked")
        .unwrap();
    let ServeError::Feed {
        name,
        end: FeedEnd::Failed(failure),
    } = &error
    else {
        panic!("expected a failed feed, got {error}");
    };
    assert_eq!(*name, PANICKING_FEED);
    assert!(failure.is_panic(), "{error}");
    assert!(error.to_string().contains("the feed hit a bug"), "{error}");
}

#[tokio::test]
async fn the_fake_spectrum_publishes_only_while_wanted() {
    let (published, mut frames) = mpsc::unbounded_channel();
    let publisher = Publisher::new(move |topic, data| {
        if topic == topics::SPECTRUM {
            published.send(data).unwrap();
        }
    });
    let (demand_tx, demand) = watch::channel(BTreeSet::new());
    let (_config, config) = watch::channel(ResolvedConfig::default());
    let _running: Vec<_> = fake::feeds()
        .into_iter()
        .map(|feed| {
            tokio::spawn(feed.run(FeedContext {
                publisher: publisher.clone(),
                demand: demand.clone(),
                config: config.clone(),
                calls: tokio::sync::broadcast::channel(64).1,
            }))
        })
        .collect();

    sleep(ticks(WATCHED_TICKS)).await;
    assert!(frames.is_empty(), "published while nobody wanted it");

    demand_tx.send_replace(topic_set(&[topics::SPECTRUM]));
    let first = timeout(WAIT, frames.recv()).await.unwrap().unwrap();
    let second = timeout(WAIT, frames.recv()).await.unwrap().unwrap();
    assert_eq!(first["bands"].as_array().unwrap().len(), 32);
    assert_ne!(first, second);

    demand_tx.send_replace(BTreeSet::new());
    sleep(ticks(1)).await;
    // A frame may already be on its way when the release lands, so only the last one counts.
    let mut last = None;
    while let Ok(frame) = frames.try_recv() {
        last = Some(frame);
    }
    assert_eq!(last.unwrap(), json!({"bands": vec![0; 32]}), "the release");
    sleep(ticks(WATCHED_TICKS)).await;
    assert!(frames.is_empty(), "kept publishing after it was released");
}

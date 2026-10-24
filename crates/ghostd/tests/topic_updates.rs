mod common;

use std::sync::Arc;
use std::time::Duration;

use common::server::{Daemon, WAIT};
use ghost_proto::topics;
use ghostd::feeds::next_topic;
use ghostd::server::{Feed, TopicUpdate};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::timeout;

const PUBLISHES_WITHOUT_READING: usize = 1_000;

/// A feed that forwards every topic update it sees.
fn watcher() -> (Feed, mpsc::UnboundedReceiver<(String, Value)>) {
    let (seen, updates) = mpsc::unbounded_channel();
    let feed = Feed::new("watcher", |mut context| async move {
        while let Ok(update) = context.topics.recv().await {
            let _ = seen.send((update.topic.to_string(), (*update.value).clone()));
        }
    });
    (feed, updates)
}

async fn update_of(updates: &mut mpsc::UnboundedReceiver<(String, Value)>, topic: &str) -> Value {
    timeout(WAIT, async {
        loop {
            let (name, value) = updates.recv().await.expect("the watcher ended");
            if name == topic {
                return value;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no update of {topic} reached the feed"))
}

#[tokio::test]
async fn every_published_value_reaches_feeds() {
    let (feed, mut updates) = watcher();
    let daemon = Daemon::start_with_feeds(vec![feed]).await;
    let mut provider = daemon.provider("inmet", &["x.y"]).await;

    provider
        .send(json!({"t": "publish", "topic": "x.y", "data": {"n": 1}}))
        .await;

    assert_eq!(update_of(&mut updates, "x.y").await, json!({"n": 1}));
}

#[tokio::test]
async fn sampled_topics_reach_feeds_too() {
    let (feed, mut updates) = watcher();
    let daemon = Daemon::start_with_feeds(vec![feed]).await;
    let _ui = daemon.subscribed_ui(&[]).await;

    let cpu = update_of(&mut updates, topics::CPU).await;

    assert!(cpu.is_object(), "{cpu}");
}

#[tokio::test]
async fn a_feed_that_never_reads_does_not_block_publishing() {
    let (done, finished) = oneshot::channel();
    let feed = Feed::new("deaf", |context| async move {
        for n in 0..PUBLISHES_WITHOUT_READING {
            context.publisher.publish("x.y", &n);
        }
        let _ = done.send(());
        std::future::pending::<()>().await;
    });
    let _daemon = Daemon::start_with_feeds(vec![feed]).await;

    timeout(Duration::from_secs(1), finished)
        .await
        .expect("publishing waited for a feed that does not read")
        .unwrap();
}

#[tokio::test]
async fn next_topic_skips_other_topics_and_lag() {
    let (sender, mut receiver) = broadcast::channel(2);
    let update = |topic: &str, value: u32| TopicUpdate {
        topic: Arc::from(topic),
        value: Arc::new(json!(value)),
    };
    // Six updates into room for two: the first four are lost, leaving `other` and the last `x`.
    for (topic, value) in [
        ("x", 1),
        ("x", 2),
        ("x", 3),
        ("x", 4),
        ("other", 5),
        ("x", 6),
    ] {
        sender.send(update(topic, value)).unwrap();
    }

    let value = next_topic(&mut receiver, "x").await;

    assert_eq!(*value, json!(6));
}

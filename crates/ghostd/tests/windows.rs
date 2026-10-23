mod common;

use std::time::{Duration, Instant};

use common::bus::PrivateBus;
use ghost_proto::{WindowInfo, WindowsSnapshot, topics};
use ghostd::feeds::windows::{Bridge, Timings};
use ghostd::server::{Call, Publisher};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use zbus::Connection;

const PATH: &str = "/org/ghostshell/Windows";
const REPORTS: &str = "org.ghostshell.Windows1";
const COMMANDS: &str = "org.ghostshell.WindowCommands1";
const WAIT: Duration = Duration::from_secs(5);
const NEXT_TIMEOUT: Duration = Duration::from_millis(100);
const STALE_AFTER: Duration = Duration::from_millis(50);
const QUIET_WINDOW: Duration = Duration::from_millis(300);
const FORWARDING_TIME: Duration = Duration::from_millis(20);
const QUEUE_CAP: usize = 32;
const FLOODED_CALLS: usize = 40;

struct Bridged {
    _bus: PrivateBus,
    _service: Connection,
    _forwarder: JoinHandle<std::convert::Infallible>,
    client: Connection,
    destination: String,
    calls: broadcast::Sender<Call>,
    published: mpsc::UnboundedReceiver<(String, Value)>,
}

impl Bridged {
    async fn start(timings: Timings) -> Self {
        let bus = PrivateBus::start();
        let (sender, published) = mpsc::unbounded_channel();
        let bridge = Bridge::new(
            Publisher::new(move |topic, data| {
                sender.send((topic.to_owned(), data)).unwrap();
            }),
            timings,
        );
        let (calls, receiver) = broadcast::channel(64);
        let service = bridge.serve(bus.builder()).build().await.unwrap();
        let destination = service.unique_name().unwrap().to_string();
        let forwarder = tokio::spawn(bridge.forward(receiver));
        let client = bus.connect().await;
        Self {
            _bus: bus,
            _service: service,
            _forwarder: forwarder,
            client,
            destination,
            calls,
            published,
        }
    }

    async fn report(&self, json: &str) {
        self.client
            .call_method(
                Some(self.destination.as_str()),
                PATH,
                Some(REPORTS),
                "Report",
                &(json,),
            )
            .await
            .unwrap();
    }

    async fn next(&self) -> String {
        self.client
            .call_method(
                Some(self.destination.as_str()),
                PATH,
                Some(COMMANDS),
                "Next",
                &(),
            )
            .await
            .unwrap()
            .body()
            .deserialize()
            .unwrap()
    }

    fn call(&self, method: &str, args: Value) {
        self.calls
            .send(Call {
                method: method.to_owned(),
                args,
            })
            .unwrap();
    }

    async fn published(&mut self) -> WindowsSnapshot {
        let (topic, data) = timeout(WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .unwrap();
        assert_eq!(topic, topics::WINDOWS);
        serde_json::from_value(data).unwrap()
    }
}

fn timings() -> Timings {
    Timings {
        next_timeout: NEXT_TIMEOUT,
        stale_after: STALE_AFTER,
    }
}

fn window(id: &str) -> Value {
    json!({
        "id": id, "app_id": "org.kde.konsole", "resource_class": "konsole",
        "caption": "Terminal", "pid": 1, "active": false, "minimized": false,
        "fullscreen": false, "demands_attention": false,
    })
}

#[tokio::test]
async fn report_publishes_windows_in_the_reported_order() {
    let mut bridged = Bridged::start(timings()).await;

    bridged
        .report(&json!([window("b"), window("a")]).to_string())
        .await;

    let ids: Vec<String> = bridged
        .published()
        .await
        .windows
        .into_iter()
        .map(|WindowInfo { id, .. }| id)
        .collect();
    assert_eq!(ids, ["b", "a"]);
}

#[tokio::test]
async fn malformed_report_keeps_the_last_list() {
    let mut bridged = Bridged::start(timings()).await;
    bridged.report(&json!([window("a")]).to_string()).await;
    bridged.published().await;

    bridged.report("{").await;

    assert!(
        timeout(QUIET_WINDOW, bridged.published.recv())
            .await
            .is_err(),
        "a malformed report published something"
    );
}

#[tokio::test]
async fn next_returns_a_queued_command_at_once() {
    let bridged = Bridged::start(timings()).await;
    bridged.call("windows.activate", json!({"id": "a"}));
    sleep(FORWARDING_TIME).await;

    assert_eq!(bridged.next().await, r#"{"action":"activate","id":"a"}"#);
}

#[tokio::test]
async fn next_returns_a_command_queued_while_it_waits() {
    let bridged = Bridged::start(timings()).await;
    let waiting = bridged.next();

    let (command, ()) = tokio::join!(waiting, async {
        sleep(FORWARDING_TIME).await;
        bridged.call("windows.minimize", json!({"id": "a"}));
    });

    assert_eq!(command, r#"{"action":"minimize","id":"a"}"#);
}

#[tokio::test]
async fn next_returns_empty_after_the_timeout() {
    let bridged = Bridged::start(timings()).await;
    let started = Instant::now();

    assert_eq!(bridged.next().await, "");

    assert!(started.elapsed() >= NEXT_TIMEOUT);
}

#[tokio::test]
async fn stale_command_is_dropped() {
    let bridged = Bridged::start(timings()).await;
    bridged.call("windows.close", json!({"id": "a"}));
    sleep(STALE_AFTER + FORWARDING_TIME).await;

    assert_eq!(bridged.next().await, "");
}

#[tokio::test]
async fn queue_keeps_the_newest_32() {
    let bridged = Bridged::start(Timings {
        stale_after: Duration::from_secs(60),
        ..timings()
    })
    .await;
    for index in 0..FLOODED_CALLS {
        bridged.call("windows.close", json!({"id": index.to_string()}));
    }
    sleep(QUIET_WINDOW).await;

    let mut ids = Vec::new();
    loop {
        let command = bridged.next().await;
        if command.is_empty() {
            break;
        }
        let command: Value = serde_json::from_str(&command).unwrap();
        ids.push(command["id"].as_str().unwrap().to_owned());
    }

    let expected: Vec<String> = (FLOODED_CALLS - QUEUE_CAP..FLOODED_CALLS)
        .map(|index| index.to_string())
        .collect();
    assert_eq!(ids, expected);
}

#[tokio::test]
async fn call_without_an_id_is_refused() {
    let bridged = Bridged::start(timings()).await;
    bridged.call("windows.close", json!({}));
    bridged.call("windows.close", json!({"id": ""}));
    bridged.call("windows.close", json!({"id": 7}));
    bridged.call("windows.activate", json!({"id": "a"}));
    sleep(FORWARDING_TIME).await;

    assert_eq!(bridged.next().await, r#"{"action":"activate","id":"a"}"#);
}

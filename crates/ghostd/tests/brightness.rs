mod common;

use std::collections::BTreeSet;
use std::time::Duration;

use common::brightness::{
    FakePlasma, MONITOR, MONITOR_LABEL, MONITOR_MAX, NightLightState, PANEL, PANEL_LABEL,
    PANEL_MAX, Request, TOGGLE_ACTION,
};
use common::bus::{BUS_WAIT, PrivateBus};
use ghost_proto::{Brightness, DisplayBrightness, NightLight, ResolvedConfig, calls, topics};
use ghostd::feeds::brightness;
use ghostd::server::{Call, FeedContext, Publisher};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};

const QUIET: Duration = Duration::from_millis(300);
const NO_FLAGS: u32 = 0;
const MONITOR_START: i32 = MONITOR_MAX;
const PANEL_START: i32 = 100;

/// The real feed on a private bus standing in for the session bus, and everything it publishes.
struct Running {
    plasma: FakePlasma,
    published: mpsc::UnboundedReceiver<(String, Value)>,
    calls: broadcast::Sender<Call>,
    demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _bus: PrivateBus,
    _task: JoinHandle<()>,
}

impl Running {
    async fn start() -> Self {
        Self::start_with(NIGHT_LIGHT_RUNNING, true).await
    }

    async fn start_with(night_light: NightLightState, wanted: bool) -> Self {
        let bus = PrivateBus::start();
        let plasma = FakePlasma::start_with(&bus, night_light).await;
        let (published_sender, published) = mpsc::unbounded_channel();
        let topics = if wanted {
            BTreeSet::from([topics::BRIGHTNESS.to_owned()])
        } else {
            BTreeSet::new()
        };
        let (demand, demand_receiver) = watch::channel(topics);
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let (calls, calls_receiver) = broadcast::channel(64);
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: calls_receiver,
        };
        let feed = brightness::feed_on(bus.connect().await);
        Self {
            plasma,
            published,
            calls,
            demand,
            _config: config,
            _bus: bus,
            _task: tokio::spawn(feed.run(context)),
        }
    }

    async fn next_snapshot(&mut self) -> Brightness {
        let (topic, data) = timeout(BUS_WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .expect("the feed stopped");
        assert_eq!(topic, topics::BRIGHTNESS);
        serde_json::from_value(data).unwrap()
    }

    async fn snapshot_where(&mut self, wanted: impl Fn(&Brightness) -> bool) -> Brightness {
        loop {
            let snapshot = self.next_snapshot().await;
            if wanted(&snapshot) {
                return snapshot;
            }
        }
    }

    fn call(&self, method: &str, args: Value) {
        self.calls
            .send(Call {
                method: method.to_owned(),
                args,
            })
            .unwrap();
    }

    /// Waits until the fake has seen `count` requests, which the feed handles in arrival order.
    async fn requests(&self, count: usize) -> Vec<Request> {
        timeout(BUS_WAIT, async {
            loop {
                let requests = self.plasma.log.requests();
                if requests.len() >= count {
                    return requests;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the fake never saw the requests")
    }

    /// Everything the fake saw once the feed had time to act on what it was asked.
    async fn requests_after_quiet(&self) -> Vec<Request> {
        sleep(QUIET).await;
        self.plasma.log.requests()
    }
}

const NIGHT_LIGHT_RUNNING: NightLightState = NightLightState {
    available: true,
    enabled: true,
    running: true,
};
const NIGHT_LIGHT_OFF: NightLightState = NightLightState {
    available: true,
    enabled: false,
    running: false,
};
const NIGHT_LIGHT_UNAVAILABLE: NightLightState = NightLightState {
    available: false,
    enabled: false,
    running: false,
};

fn set(display: &str, value: Value) -> Value {
    json!({"display": display, "value": value})
}

fn set_request(display: &str, value: i32) -> Request {
    Request::SetBrightness {
        display: display.to_owned(),
        value,
        flags: NO_FLAGS,
    }
}

#[tokio::test]
async fn publishes_the_displays_and_night_light_once_wanted() {
    let mut running = Running::start().await;
    assert_eq!(
        running.next_snapshot().await,
        Brightness {
            displays: vec![
                DisplayBrightness {
                    id: MONITOR.to_owned(),
                    label: MONITOR_LABEL.to_owned(),
                    brightness: MONITOR_START,
                    max: MONITOR_MAX,
                },
                DisplayBrightness {
                    id: PANEL.to_owned(),
                    label: PANEL_LABEL.to_owned(),
                    brightness: PANEL_START,
                    max: PANEL_MAX,
                },
            ],
            night_light: NightLight {
                available: true,
                enabled: true,
                running: true,
            },
        }
    );
}

#[tokio::test]
async fn publishes_nothing_until_wanted_and_again_each_time_it_is() {
    let mut running = Running::start_with(NIGHT_LIGHT_RUNNING, false).await;
    sleep(QUIET).await;
    assert!(running.published.try_recv().is_err());
    let wanted = BTreeSet::from([topics::BRIGHTNESS.to_owned()]);
    running.demand.send(wanted).unwrap();
    let first = running.next_snapshot().await;
    running.demand.send(BTreeSet::new()).unwrap();
    sleep(QUIET).await;
    running.plasma.set_display_brightness(MONITOR, 1234).await;
    sleep(QUIET).await;
    assert!(
        running.published.try_recv().is_err(),
        "a change while unwanted must not be read"
    );
    let wanted = BTreeSet::from([topics::BRIGHTNESS.to_owned()]);
    running.demand.send(wanted).unwrap();
    let second = running.next_snapshot().await;
    assert_eq!(first.displays[0].brightness, MONITOR_START);
    assert_eq!(second.displays[0].brightness, 1234);
}

#[tokio::test]
async fn follows_a_brightness_change_without_polling() {
    let mut running = Running::start().await;
    running.next_snapshot().await;
    running.plasma.set_display_brightness(PANEL, 42).await;
    let snapshot = running
        .snapshot_where(|snapshot| snapshot.displays[1].brightness == 42)
        .await;
    assert_eq!(snapshot.displays[0].brightness, MONITOR_START);
}

#[tokio::test]
async fn follows_night_light() {
    let mut running = Running::start().await;
    running.next_snapshot().await;
    running.plasma.set_night_light(NIGHT_LIGHT_OFF).await;
    let snapshot = running
        .snapshot_where(|snapshot| !snapshot.night_light.enabled)
        .await;
    assert_eq!(
        snapshot.night_light,
        NightLight {
            available: true,
            enabled: false,
            running: false,
        }
    );
}

#[tokio::test]
async fn no_displays_once_powerdevil_leaves_the_bus() {
    let mut running = Running::start().await;
    running.next_snapshot().await;
    running.plasma.release_screen_brightness().await;
    let snapshot = running
        .snapshot_where(|snapshot| snapshot.displays.is_empty())
        .await;
    assert!(snapshot.night_light.running);
}

#[tokio::test]
async fn sets_a_display_and_clamps_the_value_to_its_range() {
    let running = Running::start().await;
    running.call(calls::BRIGHTNESS_SET, set(MONITOR, json!(5000)));
    running.call(calls::BRIGHTNESS_SET, set(MONITOR, json!(99_999)));
    running.call(calls::BRIGHTNESS_SET, set(MONITOR, json!(0)));
    running.call(calls::BRIGHTNESS_SET, set(MONITOR, json!(-40)));
    running.call(calls::BRIGHTNESS_SET, set(PANEL, json!(77.6)));
    assert_eq!(
        running.requests(5).await,
        [
            set_request(MONITOR, 5000),
            set_request(MONITOR, MONITOR_MAX),
            set_request(MONITOR, 1),
            set_request(MONITOR, 1),
            set_request(PANEL, 78),
        ]
    );
}

#[tokio::test]
async fn refuses_a_set_it_cannot_trust() {
    let running = Running::start().await;
    for display in [
        json!("display9"),
        json!("../NightLight"),
        json!(""),
        json!(null),
        json!(7),
    ] {
        running.call(
            calls::BRIGHTNESS_SET,
            json!({"display": display, "value": 100}),
        );
    }
    for value in [json!("100"), json!(null), json!(true), json!([1])] {
        running.call(calls::BRIGHTNESS_SET, set(MONITOR, value));
    }
    running.call(calls::BRIGHTNESS_SET, json!({"display": MONITOR}));
    running.call(calls::BRIGHTNESS_SET, json!({"value": 5}));
    running.call(calls::BRIGHTNESS_SET, json!({}));
    running.call(calls::BRIGHTNESS_SET, set(MONITOR, json!(300)));
    assert_eq!(running.requests(1).await, [set_request(MONITOR, 300)]);
    assert_eq!(running.requests_after_quiet().await.len(), 1);
}

#[tokio::test]
async fn sets_a_display_while_the_topic_is_not_wanted() {
    let running = Running::start_with(NIGHT_LIGHT_RUNNING, false).await;
    running.call(calls::BRIGHTNESS_SET, set(MONITOR, json!(900)));
    assert_eq!(running.requests(1).await, [set_request(MONITOR, 900)]);
}

#[tokio::test]
async fn toggles_night_light_through_kwins_own_action() {
    let running = Running::start().await;
    running.call(calls::NIGHTLIGHT_TOGGLE, json!({}));
    assert_eq!(
        running.requests(1).await,
        [Request::InvokeShortcut(TOGGLE_ACTION.to_owned())]
    );
}

#[tokio::test]
async fn leaves_a_night_light_that_is_off_or_missing_alone() {
    for night_light in [NIGHT_LIGHT_OFF, NIGHT_LIGHT_UNAVAILABLE] {
        let running = Running::start_with(night_light, true).await;
        running.call(calls::NIGHTLIGHT_TOGGLE, json!({}));
        assert!(running.requests_after_quiet().await.is_empty());
    }
}

#[tokio::test]
async fn survives_a_plasma_that_is_not_on_the_bus() {
    let bus = PrivateBus::start();
    let (published_sender, mut published) = mpsc::unbounded_channel();
    let (_demand, demand_receiver) =
        watch::channel(BTreeSet::from([topics::BRIGHTNESS.to_owned()]));
    let (_config, config_receiver) = watch::channel(ResolvedConfig::default());
    let (calls_sender, calls_receiver) = broadcast::channel(64);
    let context = FeedContext {
        publisher: Publisher::new(move |topic, data| {
            published_sender.send((topic.to_owned(), data)).unwrap();
        }),
        demand: demand_receiver,
        config: config_receiver,
        calls: calls_receiver,
    };
    let task = tokio::spawn(brightness::feed_on(bus.connect().await).run(context));
    let (_, data) = timeout(BUS_WAIT, published.recv()).await.unwrap().unwrap();
    assert_eq!(
        serde_json::from_value::<Brightness>(data).unwrap(),
        Brightness::default()
    );
    calls_sender
        .send(Call {
            method: calls::NIGHTLIGHT_TOGGLE.to_owned(),
            args: json!({}),
        })
        .unwrap();
    calls_sender
        .send(Call {
            method: calls::BRIGHTNESS_SET.to_owned(),
            args: set(MONITOR, json!(10)),
        })
        .unwrap();
    sleep(QUIET).await;
    assert!(!task.is_finished());
}

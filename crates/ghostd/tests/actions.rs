mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use common::actions::{Seen, SessionStubs, Stubs, assert_private};
use common::bus::PrivateBus;
use ghost_proto::{ResolvedConfig, calls};
use ghostd::feeds::actions;
use ghostd::server::{Call, FeedContext, Publisher};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

const NETWORK: &str = "org.kde.plasma.networkmanagement";
const CLOCK: &str = "org.kde.plasma.digitalclock";
const BRIGHTNESS: &str = "org.kde.plasma.brightness";
const MAP_FILE: &str = "plasma-popups";

/// The real feed on private buses only. `feed()`, which connects to the machine's own buses, is
/// never called here.
struct Running {
    _session_bus: PrivateBus,
    _system_bus: PrivateBus,
    stubs: Stubs,
    calls: broadcast::Sender<Call>,
    map: TempDir,
    _demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _task: JoinHandle<()>,
}

impl Running {
    async fn start(which: SessionStubs) -> Self {
        let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
        assert_private(&session_bus);
        assert_private(&system_bus);
        let stubs = Stubs::start(&session_bus, &system_bus, which).await;
        let map = TempDir::new().unwrap();
        let (calls, receiver) = broadcast::channel(64);
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(|_, _| panic!("the actions feed publishes nothing")),
            demand: demand_receiver,
            config: config_receiver,
            calls: receiver,
        };
        let feed = actions::feed_on(
            Some(session_bus.connect().await),
            Some(system_bus.connect().await),
            map.path().join(MAP_FILE),
        );
        Self {
            _session_bus: session_bus,
            _system_bus: system_bus,
            stubs,
            calls,
            map,
            _demand: demand,
            _config: config,
            _task: tokio::spawn(feed.run(context)),
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

    fn write_map(&self, contents: &str) {
        fs::write(self.map.path().join(MAP_FILE), contents).unwrap();
    }

    /// Everything the stubs saw once a final lock call has been handled, which the feed does in
    /// order, so a call it refused has had its chance to show up first.
    async fn seen_before_a_lock(&self) -> Vec<Seen> {
        self.call(calls::SESSION_LOCK, json!({}));
        self.stubs.log.wait_for(1).await
    }
}

fn seen(service: &'static str, path: &'static str, member: &'static str, arg: &str) -> Seen {
    Seen {
        service,
        path,
        member,
        arg: arg.to_owned(),
        no_auto_start: true,
    }
}

fn lock() -> Seen {
    seen("org.freedesktop.ScreenSaver", "/ScreenSaver", "Lock", "")
}

fn invoke(arg: &str) -> Seen {
    seen(
        "org.kde.kglobalaccel",
        "/component/plasmashell",
        "invokeShortcut",
        arg,
    )
}

#[tokio::test]
async fn lock_calls_the_screen_saver() {
    let running = Running::start(SessionStubs::ALL).await;
    running.call(calls::SESSION_LOCK, json!({}));
    assert_eq!(running.stubs.log.wait_for(1).await, [lock()]);
}

#[tokio::test]
async fn logout_reboot_shutdown_go_through_ksmserver() {
    let running = Running::start(SessionStubs::ALL).await;
    running.call(calls::SESSION_LOGOUT, json!({}));
    running.call(calls::SESSION_REBOOT, json!({}));
    running.call(calls::SESSION_SHUTDOWN, json!({}));
    let shutdown = |member| seen("org.kde.Shutdown", "/Shutdown", member, "");
    assert_eq!(
        running.stubs.log.wait_for(3).await,
        [
            shutdown("logout"),
            shutdown("logoutAndReboot"),
            shutdown("logoutAndShutdown"),
        ]
    );
}

#[tokio::test]
async fn suspend_asks_logind_without_interaction() {
    let running = Running::start(SessionStubs::ALL).await;
    running.call(calls::SESSION_SUSPEND, json!({}));
    assert_eq!(
        running.stubs.log.wait_for(1).await,
        [seen(
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "Suspend",
            "false"
        )]
    );
}

#[tokio::test]
async fn krunner_gets_the_trimmed_query() {
    let running = Running::start(SessionStubs::ALL).await;
    running.call(calls::KRUNNER_QUERY, json!({"text": "  fire "}));
    let mut expected = seen("org.kde.krunner", "/App", "query", "fire");
    // KRunner is D-Bus activated, so the bus may start it for the query.
    expected.no_auto_start = false;
    assert_eq!(running.stubs.log.wait_for(1).await, [expected]);
}

#[tokio::test]
async fn krunner_query_is_capped_at_256_characters() {
    let running = Running::start(SessionStubs::ALL).await;
    running.call(calls::KRUNNER_QUERY, json!({"text": "é".repeat(300)}));
    let seen = running.stubs.log.wait_for(1).await;
    assert_eq!(seen[0].arg, "é".repeat(256));
}

#[tokio::test]
async fn empty_krunner_query_is_refused() {
    let running = Running::start(SessionStubs::ALL).await;
    running.call(calls::KRUNNER_QUERY, json!({"text": "   "}));
    running.call(calls::KRUNNER_QUERY, json!({}));
    assert_eq!(running.seen_before_a_lock().await, [lock()]);
}

#[tokio::test]
async fn popup_for_a_known_applet_uses_the_spike_route() {
    let running = Running::start(SessionStubs::ALL).await;
    running.write_map(&format!("{NETWORK}\t57\n{CLOCK}\t58\n"));
    running.call(calls::PLASMA_POPUP, json!({"applet": CLOCK}));
    assert_eq!(
        running.stubs.log.wait_for(1).await,
        [invoke("activate widget 58")]
    );
}

#[tokio::test]
async fn popup_for_the_brightness_applet_uses_its_widget() {
    let running = Running::start(SessionStubs::ALL).await;
    running.write_map(&format!("{BRIGHTNESS}\t61\n"));
    running.call(calls::PLASMA_POPUP, json!({"applet": BRIGHTNESS}));
    assert_eq!(
        running.stubs.log.wait_for(1).await,
        [invoke("activate widget 61")]
    );
}
#[tokio::test]
async fn popup_map_is_read_again_on_every_call() {
    let running = Running::start(SessionStubs::ALL).await;
    running.write_map(&format!("{NETWORK}\t57\n"));
    running.call(calls::PLASMA_POPUP, json!({"applet": NETWORK}));
    running.stubs.log.wait_for(1).await;
    running.write_map(&format!("{NETWORK}\t91\n"));
    running.call(calls::PLASMA_POPUP, json!({"applet": NETWORK}));
    assert_eq!(
        running.stubs.log.wait_for(2).await[1],
        invoke("activate widget 91")
    );
}

#[tokio::test]
async fn popup_for_an_unknown_applet_is_refused() {
    let running = Running::start(SessionStubs::ALL).await;
    running.write_map("org.kde.plasma.battery\t5\n");
    running.call(
        calls::PLASMA_POPUP,
        json!({"applet": "org.kde.plasma.battery"}),
    );
    running.call(calls::PLASMA_POPUP, json!({}));
    assert_eq!(running.seen_before_a_lock().await, [lock()]);
}

#[tokio::test]
async fn popup_ignores_map_lines_that_are_not_an_applet_and_digits() {
    let running = Running::start(SessionStubs::ALL).await;
    running.write_map(&format!(
        "org.kde.plasma.battery\t5\n{NETWORK}\t\n{NETWORK}\t5x\n{NETWORK}\t-3\nnoise\n"
    ));
    running.call(calls::PLASMA_POPUP, json!({"applet": NETWORK}));
    assert_eq!(running.seen_before_a_lock().await, [lock()]);
}

#[tokio::test]
async fn popup_without_a_map_does_nothing_and_does_not_stop_the_feed() {
    let running = Running::start(SessionStubs::ALL).await;
    assert!(!Path::new(&running.map.path().join(MAP_FILE)).exists());
    running.call(calls::PLASMA_POPUP, json!({"applet": NETWORK}));
    assert_eq!(running.seen_before_a_lock().await, [lock()]);
}

#[tokio::test]
async fn missing_service_is_logged_not_fatal() {
    let running = Running::start(SessionStubs {
        screen_saver: false,
    })
    .await;
    running.call(calls::SESSION_LOCK, json!({}));
    running.call(calls::SESSION_SUSPEND, json!({}));
    let seen = running.stubs.log.wait_for(1).await;
    assert_eq!(seen[0].member, "Suspend");
}

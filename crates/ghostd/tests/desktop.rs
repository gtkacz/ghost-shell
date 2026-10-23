mod common;

use std::future::pending;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use common::bus::{PrivateBus, RunningDesktop, call_desktop, next};
use ghost_proto::{DesktopSnapshot, ViewAction, topics};
use ghostd::feeds::desktop::{DesktopService, DesktopState, ScriptReload};
use ghostd::feeds::shortcuts::Registration;
use ghostd::server::Publisher;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use zbus::proxy::CacheProperties;
use zbus::{Connection, Guid, connection, fdo, interface};

// The contract the KWin script and the smoke tests call.
const OBJECT_PATH: &str = "/org/ghostshell/Desktop";
const INTERFACE: &str = "org.ghostshell.Desktop1";
const WAIT: Duration = Duration::from_secs(5);
const QUIET_WINDOW: Duration = Duration::from_millis(600);
const KWIN: &str = "org.kde.KWin";
// KWin's scripting API, as introspected on a Plasma 6 session.
const SCRIPTING_PATH: &str = "/Scripting";
const SCRIPT: &str = "ghost-shell-desktop";

type Published = mpsc::UnboundedReceiver<(String, Value)>;

fn outputs(names: &[&str]) -> Vec<String> {
    names.iter().map(|&name| name.to_owned()).collect()
}

/// A desktop service served on one end of a peer-to-peer connection, a client on the other end,
/// and every value the service publishes.
async fn serve_peer_to_peer() -> (Connection, Connection, Published) {
    let (published, values) = mpsc::unbounded_channel();
    let desktop = DesktopService::new(Publisher::new(move |topic, data| {
        published.send((topic.to_owned(), data)).unwrap();
    }));
    let (service_end, client_end) = UnixStream::pair().unwrap();
    let (service, client) = tokio::try_join!(
        connection::Builder::unix_stream(service_end)
            .server(Guid::generate())
            .p2p()
            .serve_at(OBJECT_PATH, desktop)
            .build(),
        connection::Builder::unix_stream(client_end).p2p().build(),
    )
    .unwrap();
    (service, client, values)
}

#[test]
fn report_changes_only_when_values_change() {
    let mut state = DesktopState::default();

    assert!(
        !state.report(Vec::new(), false),
        "the defaults are no change"
    );
    assert!(state.report(outputs(&["DP-1"]), false));
    assert!(!state.report(outputs(&["DP-1"]), false));
    assert!(state.report(outputs(&["DP-1"]), true));
    assert!(state.report(Vec::new(), true));
    assert!(!state.report(Vec::new(), true));
}

#[test]
fn reports_ignore_order_repeats_and_blanks() {
    let mut state = DesktopState::default();
    assert!(state.report(outputs(&["HDMI-A-1", "DP-1"]), false));

    assert!(!state.report(outputs(&[" DP-1", "", "HDMI-A-1 ", "DP-1"]), false));
    assert_eq!(
        state.snapshot().covered_outputs,
        outputs(&["DP-1", "HDMI-A-1"])
    );
}

#[test]
fn actions_count_only_on_the_desktop() {
    let mut state = DesktopState::default();
    assert!(!state.activate(ViewAction::Monitor));
    assert_eq!(state.snapshot().action_seq, 0);

    state.report(Vec::new(), true);
    assert!(state.activate(ViewAction::Monitor));
    let snapshot = state.snapshot();
    assert_eq!(snapshot.action_seq, 1);
    assert_eq!(snapshot.action, Some(ViewAction::Monitor));

    state.report(outputs(&["DP-1"]), false);
    assert!(!state.activate(ViewAction::Swap));
    let snapshot = state.snapshot();
    assert_eq!(snapshot.action_seq, 1);
    assert_eq!(snapshot.action, Some(ViewAction::Monitor));
}

#[tokio::test]
async fn dbus_report_reaches_the_publisher() {
    let (_service, client, mut published) = serve_peer_to_peer().await;

    client
        .call_method(
            None::<&str>,
            OBJECT_PATH,
            Some(INTERFACE),
            "Report",
            &("DP-1,HDMI-A-1", false),
        )
        .await
        .unwrap();

    let (topic, data) = timeout(WAIT, published.recv())
        .await
        .expect("nothing was published in time")
        .unwrap();
    assert_eq!(topic, topics::DESKTOP);
    assert_eq!(
        serde_json::from_value::<DesktopSnapshot>(data).unwrap(),
        DesktopSnapshot {
            covered_outputs: outputs(&["DP-1", "HDMI-A-1"]),
            active: false,
            ..DesktopSnapshot::default()
        }
    );
}

#[tokio::test]
async fn dbus_activate_rejects_unknown_actions() {
    let (_service, client, _published) = serve_peer_to_peer().await;

    let error = client
        .call_method(
            None::<&str>,
            OBJECT_PATH,
            Some(INTERFACE),
            "Activate",
            &("nope",),
        )
        .await
        .unwrap_err();

    assert_eq!(
        fdo::Error::from(error),
        fdo::Error::InvalidArgs("unknown view action \"nope\"".to_owned())
    );
}

#[tokio::test]
async fn dbus_report_of_an_empty_string_covers_no_output() {
    let (_service, client, mut published) = serve_peer_to_peer().await;

    client
        .call_method(
            None::<&str>,
            OBJECT_PATH,
            Some(INTERFACE),
            "Report",
            &("", true),
        )
        .await
        .unwrap();

    let (_, data) = timeout(WAIT, published.recv())
        .await
        .expect("nothing was published in time")
        .unwrap();
    let snapshot: DesktopSnapshot = serde_json::from_value(data).unwrap();
    assert!(snapshot.covered_outputs.is_empty());
    assert!(snapshot.active);
}

#[tokio::test]
async fn the_desktop_resets_when_kwin_leaves_the_bus() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    let kwin = bus.builder().name(KWIN).build().await.unwrap();
    call_desktop(&kwin, "Report", &("DP-1", true)).await;
    desktop.next_snapshot().await;
    call_desktop(&kwin, "Activate", &("monitor",)).await;
    desktop.next_snapshot().await;

    kwin.close().await.unwrap();

    // The action count stays, since clients compare against it.
    assert_eq!(
        desktop.next_snapshot().await,
        DesktopSnapshot {
            covered_outputs: Vec::new(),
            active: false,
            action: Some(ViewAction::Monitor),
            action_seq: 1,
            ..DesktopSnapshot::default()
        }
    );
}

#[tokio::test]
async fn a_restarted_kwin_reports_after_the_reset() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    let kwin = bus.builder().name(KWIN).build().await.unwrap();
    call_desktop(&kwin, "Report", &("DP-1", true)).await;
    desktop.next_snapshot().await;

    kwin.close().await.unwrap();
    assert_eq!(desktop.next_snapshot().await, DesktopSnapshot::default());

    let restarted = bus.builder().name(KWIN).build().await.unwrap();
    call_desktop(&restarted, "Report", &("HDMI-A-1", false)).await;
    assert_eq!(
        desktop.next_snapshot().await,
        DesktopSnapshot {
            covered_outputs: outputs(&["HDMI-A-1"]),
            ..DesktopSnapshot::default()
        }
    );
}

#[tokio::test]
async fn kwin_replaced_in_place_is_not_a_departure() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    // zbus asks for names with ALLOW_REPLACEMENT | REPLACE_EXISTING | DO_NOT_QUEUE, so a second
    // owner takes the name over at once and the first leaves no queue behind.
    let kwin = bus.builder().name(KWIN).build().await.unwrap();
    call_desktop(&kwin, "Report", &("DP-1", true)).await;
    desktop.next_snapshot().await;
    let watcher = bus.connect().await;
    let mut owners = fdo::DBusProxy::builder(&watcher)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .unwrap()
        .receive_name_owner_changed_with_args(&[(0, KWIN)])
        .await
        .unwrap();

    let replacement = bus.builder().name(KWIN).build().await.unwrap();
    let change = timeout(WAIT, next(&mut owners))
        .await
        .expect("the name never changed hands")
        .unwrap();
    let args = change.args().unwrap();
    assert_eq!(
        args.old_owner().as_ref().map(|owner| owner.as_str()),
        kwin.unique_name().map(|owner| owner.as_str())
    );
    assert_eq!(
        args.new_owner().as_ref().map(|owner| owner.as_str()),
        replacement.unique_name().map(|owner| owner.as_str())
    );

    // A reset would make the desktop inactive, so Activate would be refused.
    call_desktop(&replacement, "Activate", &("monitor",)).await;
    assert_eq!(
        desktop.next_snapshot().await,
        DesktopSnapshot {
            covered_outputs: outputs(&["DP-1"]),
            active: true,
            action: Some(ViewAction::Monitor),
            action_seq: 1,
            ..DesktopSnapshot::default()
        }
    );
}

#[tokio::test]
async fn a_one_shot_reporter_leaving_does_not_reset_the_desktop() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    // As `busctl --user call … Report sb "" true` does: report, then disconnect.
    let reporter = bus.connect().await;
    call_desktop(&reporter, "Report", &("", true)).await;
    desktop.next_snapshot().await;
    reporter.close().await.unwrap();

    let caller = bus.connect().await;
    call_desktop(&caller, "Activate", &("monitor",)).await;

    assert_eq!(
        desktop.next_snapshot().await,
        DesktopSnapshot {
            covered_outputs: Vec::new(),
            active: true,
            action: Some(ViewAction::Monitor),
            action_seq: 1,
            ..DesktopSnapshot::default()
        }
    );
}

#[derive(Debug, PartialEq)]
enum Call {
    IsScriptLoaded(String),
    UnloadScript(String),
    Start,
}

#[derive(Debug, Clone, Copy)]
enum Unload {
    Succeeds,
    Fails,
    Hangs,
}

/// Records what it is asked for and answers `isScriptLoaded` with `loaded`, so that each test
/// chooses what KWin knows about the script and how unloading it goes.
struct FakeScripting {
    calls: mpsc::UnboundedSender<Call>,
    loaded: bool,
    unload: Unload,
}

#[interface(name = "org.kde.kwin.Scripting")]
impl FakeScripting {
    #[zbus(name = "isScriptLoaded")]
    fn is_script_loaded(&self, plugin_name: String) -> bool {
        self.calls.send(Call::IsScriptLoaded(plugin_name)).unwrap();
        self.loaded
    }

    #[zbus(name = "unloadScript")]
    async fn unload_script(&self, plugin_name: String) -> fdo::Result<bool> {
        self.calls.send(Call::UnloadScript(plugin_name)).unwrap();
        match self.unload {
            Unload::Succeeds => Ok(true),
            Unload::Fails => Err(fdo::Error::Failed("refused".to_owned())),
            Unload::Hangs => pending().await,
        }
    }

    #[zbus(name = "start")]
    fn start(&self) {
        self.calls.send(Call::Start).unwrap();
    }
}

/// A fake KWin that owns its bus name and serves `/Scripting`, and the calls it receives.
async fn start_kwin(
    bus: &PrivateBus,
    loaded: bool,
    unload: Unload,
) -> (Connection, mpsc::UnboundedReceiver<Call>) {
    let (calls_sender, calls) = mpsc::unbounded_channel();
    let kwin = bus
        .builder()
        .name(KWIN)
        .serve_at(
            SCRIPTING_PATH,
            FakeScripting {
                calls: calls_sender,
                loaded,
                unload,
            },
        )
        .build()
        .await
        .unwrap();
    (kwin, calls)
}

async fn next_calls(calls: &mut mpsc::UnboundedReceiver<Call>, count: usize) -> Vec<Call> {
    timeout(WAIT, async {
        let mut received = Vec::new();
        while received.len() < count {
            received.push(calls.recv().await.expect("the fake stopped"));
        }
        received
    })
    .await
    .unwrap_or_else(|_| panic!("fewer than {count} calls arrived in time"))
}

async fn assert_no_more_calls(calls: &mut mpsc::UnboundedReceiver<Call>) {
    if let Ok(call) = timeout(QUIET_WINDOW, calls.recv()).await {
        panic!("KWin was asked for {call:?} unexpectedly");
    }
}

fn reload_calls() -> Vec<Call> {
    vec![
        Call::IsScriptLoaded(SCRIPT.to_owned()),
        Call::UnloadScript(SCRIPT.to_owned()),
        Call::Start,
    ]
}

#[tokio::test]
async fn a_start_reloads_a_loaded_kwin_script() {
    let bus = PrivateBus::start();
    let (_kwin, mut calls) = start_kwin(&bus, true, Unload::Succeeds).await;
    let _desktop = RunningDesktop::start(&bus, Registration::default()).await;

    assert_eq!(next_calls(&mut calls, 3).await, reload_calls());
}

#[tokio::test]
async fn a_start_leaves_an_unloaded_kwin_script_alone() {
    let bus = PrivateBus::start();
    let (_kwin, mut calls) = start_kwin(&bus, false, Unload::Succeeds).await;
    let _desktop = RunningDesktop::start(&bus, Registration::default()).await;

    assert_eq!(
        next_calls(&mut calls, 1).await,
        [Call::IsScriptLoaded(SCRIPT.to_owned())]
    );
    assert_no_more_calls(&mut calls).await;
}

#[tokio::test]
async fn a_start_without_kwin_does_nothing() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;

    let reporter = bus.connect().await;
    call_desktop(&reporter, "Report", &("DP-1", false)).await;

    assert_eq!(
        desktop.next_snapshot().await,
        DesktopSnapshot {
            covered_outputs: outputs(&["DP-1"]),
            ..DesktopSnapshot::default()
        }
    );
}

#[tokio::test]
async fn a_failed_unload_still_starts() {
    let bus = PrivateBus::start();
    let (_kwin, mut calls) = start_kwin(&bus, true, Unload::Fails).await;
    let _desktop = RunningDesktop::start(&bus, Registration::default()).await;

    assert_eq!(next_calls(&mut calls, 3).await, reload_calls());
}

#[tokio::test]
async fn an_unload_that_never_answers_is_given_up_on_and_still_starts() {
    let bus = PrivateBus::start();
    let (_kwin, mut calls) = start_kwin(&bus, true, Unload::Hangs).await;
    let _desktop = RunningDesktop::start(&bus, Registration::default()).await;

    assert_eq!(next_calls(&mut calls, 3).await, reload_calls());
}

#[tokio::test]
async fn kwin_appearing_later_is_not_asked_to_reload() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;

    let (kwin, mut calls) = start_kwin(&bus, true, Unload::Succeeds).await;
    // As a script that loads with the new KWin does. Its report follows KWin's arrival on the bus,
    // so once it is published ghostd has seen the arrival too.
    call_desktop(&kwin, "Report", &("DP-1", true)).await;
    desktop.next_snapshot().await;

    assert_no_more_calls(&mut calls).await;
}

#[tokio::test]
async fn fake_mode_never_reloads() {
    let bus = PrivateBus::start();
    let (_kwin, mut calls) = start_kwin(&bus, true, Unload::Succeeds).await;
    let _desktop =
        RunningDesktop::start_with(&bus, Registration::default(), ScriptReload::Skip).await;

    assert_no_more_calls(&mut calls).await;
}

#[test]
fn task_presses_count_over_a_window() {
    let mut state = DesktopState::default();
    state.report(outputs(&["DP-1"]), false);

    assert!(state.press_task(4));
    assert!(state.press_task(4));
    let snapshot = state.snapshot();
    assert_eq!(snapshot.task, Some(4));
    assert_eq!(snapshot.task_seq, 2);
}

#[tokio::test]
async fn toggle_launcher_counts_even_over_a_window() {
    let bus = PrivateBus::start();
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    let caller = bus.connect().await;

    call_desktop(&caller, "ToggleLauncher", &()).await;
    call_desktop(&caller, "ToggleLauncher", &()).await;

    let snapshot = desktop.next_snapshot().await;
    assert!(!snapshot.active);
    assert_eq!(snapshot.launcher_seq, 1);
    assert_eq!(desktop.next_snapshot().await.launcher_seq, 2);
}

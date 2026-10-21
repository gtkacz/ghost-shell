mod common;

use std::os::unix::net::UnixStream;
use std::time::Duration;

use common::bus::{PrivateBus, RunningDesktop, call_desktop, next};
use ghost_proto::{DesktopSnapshot, ViewAction, topics};
use ghostd::feeds::desktop::{DesktopService, DesktopState};
use ghostd::feeds::shortcuts::Registration;
use ghostd::server::Publisher;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use zbus::proxy::CacheProperties;
use zbus::{Connection, Guid, connection, fdo};

// The contract the KWin script and the smoke tests call.
const OBJECT_PATH: &str = "/org/ghostshell/Desktop";
const INTERFACE: &str = "org.ghostshell.Desktop1";
const WAIT: Duration = Duration::from_secs(5);
const KWIN: &str = "org.kde.KWin";

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
        }
    );
}

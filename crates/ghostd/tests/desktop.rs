use std::os::unix::net::UnixStream;
use std::time::Duration;

use ghost_proto::{DesktopSnapshot, ViewAction, topics};
use ghostd::feeds::desktop::{DesktopService, DesktopState};
use ghostd::server::Publisher;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::timeout;
use zbus::{Connection, Guid, connection, fdo};

// The contract the KWin script (Task 4) and the smoke tests call.
const OBJECT_PATH: &str = "/org/ghostshell/Desktop";
const INTERFACE: &str = "org.ghostshell.Desktop1";
const WAIT: Duration = Duration::from_secs(5);

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

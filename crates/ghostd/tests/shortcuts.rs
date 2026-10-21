use std::os::unix::net::UnixStream;
use std::time::Duration;

use ghost_proto::{DesktopSnapshot, ResolvedConfig, ViewAction, topics};
use ghostd::feeds::desktop::DesktopService;
use ghostd::feeds::shortcuts;
use ghostd::server::Publisher;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;
use zbus::{Connection, Guid, connection, interface};

const KGLOBALACCEL_PATH: &str = "/kglobalaccel";
const COMPONENT_PATH: &str = "/component/ghost_shell";
const COMPONENT_INTERFACE: &str = "org.kde.kglobalaccel.Component";
const PRESSED_SIGNAL: &str = "globalShortcutPressed";
const COMPONENT: &str = "ghost-shell";
const WAIT: Duration = Duration::from_secs(5);
// SetPresent (2) | NoAutoloading (4), from kglobalaccel 6.31's kglobalaccel_p.h.
const SET_PRESENT_NO_AUTOLOADING: u32 = 6;
const META_M: i32 = 0x1000_004D;
const META_N: i32 = 0x1000_004E;
const META_SHIFT_M: i32 = 0x1200_004D;
const REGISTRATION_CALLS: usize = 4;
const NO_TIMESTAMP: i64 = 0;

#[derive(Debug, PartialEq)]
enum Call {
    Register(Vec<String>),
    SetKeys {
        action_id: Vec<String>,
        keys: Vec<Vec<i32>>,
        flags: u32,
    },
}

/// Records what it is asked for and grants every key.
struct FakeKGlobalAccel(mpsc::UnboundedSender<Call>);

#[interface(name = "org.kde.KGlobalAccel")]
impl FakeKGlobalAccel {
    #[zbus(name = "doRegister")]
    fn do_register(&self, action_id: Vec<String>) {
        self.0.send(Call::Register(action_id)).unwrap();
    }

    #[zbus(name = "setShortcutKeys")]
    fn set_shortcut_keys(
        &self,
        action_id: Vec<String>,
        keys: Vec<(Vec<i32>,)>,
        flags: u32,
    ) -> Vec<(Vec<i32>,)> {
        self.0
            .send(Call::SetKeys {
                action_id,
                keys: keys.iter().map(|(key,)| key.clone()).collect(),
                flags,
            })
            .unwrap();
        keys
    }
}

struct Harness {
    kglobalaccel: Connection,
    calls: mpsc::UnboundedReceiver<Call>,
    config: watch::Sender<ResolvedConfig>,
    desktop: DesktopService,
    published: mpsc::UnboundedReceiver<(String, Value)>,
}

/// Runs `shortcuts::run` against a fake KGlobalAccel on the other end of a peer-to-peer
/// connection.
async fn start() -> Harness {
    let (calls_sender, calls) = mpsc::unbounded_channel();
    let (kglobalaccel_end, ghostd_end) = UnixStream::pair().unwrap();
    let (kglobalaccel, ghostd) = tokio::try_join!(
        connection::Builder::unix_stream(kglobalaccel_end)
            .server(Guid::generate())
            .p2p()
            .serve_at(KGLOBALACCEL_PATH, FakeKGlobalAccel(calls_sender))
            .build(),
        connection::Builder::unix_stream(ghostd_end).p2p().build(),
    )
    .unwrap();
    let (published_sender, published) = mpsc::unbounded_channel();
    let desktop = DesktopService::new(Publisher::new(move |topic, data| {
        published_sender.send((topic.to_owned(), data)).unwrap();
    }));
    let (config, config_receiver) = watch::channel(ResolvedConfig::default());
    tokio::spawn(shortcuts::run(ghostd, config_receiver, desktop.clone()));
    Harness {
        kglobalaccel,
        calls,
        config,
        desktop,
        published,
    }
}

fn action_id(action: &str, friendly_name: &str) -> Vec<String> {
    [COMPONENT, action, COMPONENT, friendly_name]
        .map(str::to_owned)
        .to_vec()
}

/// The calls that register `monitor` and `swap` with one key each.
fn registration(monitor: i32, swap: i32) -> Vec<Call> {
    let monitor_id = action_id("monitor", "Toggle the monitor view");
    let swap_id = action_id("swap", "Next main view");
    vec![
        Call::Register(monitor_id.clone()),
        Call::SetKeys {
            action_id: monitor_id,
            keys: vec![vec![monitor, 0, 0, 0]],
            flags: SET_PRESENT_NO_AUTOLOADING,
        },
        Call::Register(swap_id.clone()),
        Call::SetKeys {
            action_id: swap_id,
            keys: vec![vec![swap, 0, 0, 0]],
            flags: SET_PRESENT_NO_AUTOLOADING,
        },
    ]
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

async fn next_snapshot(
    published: &mut mpsc::UnboundedReceiver<(String, Value)>,
) -> DesktopSnapshot {
    let (topic, data) = timeout(WAIT, published.recv())
        .await
        .expect("nothing was published in time")
        .unwrap();
    assert_eq!(topic, topics::DESKTOP);
    serde_json::from_value(data).unwrap()
}

impl Harness {
    /// Puts the desktop in focus, so that presses are taken.
    async fn focus_desktop(&mut self) {
        self.desktop.report("", true);
        assert!(next_snapshot(&mut self.published).await.active);
    }

    /// Presses `shortcut` and returns the snapshot that follows.
    async fn press(&mut self, shortcut: &str) -> DesktopSnapshot {
        self.kglobalaccel
            .emit_signal(
                None::<&str>,
                COMPONENT_PATH,
                COMPONENT_INTERFACE,
                PRESSED_SIGNAL,
                &(COMPONENT, shortcut, NO_TIMESTAMP),
            )
            .await
            .unwrap();
        next_snapshot(&mut self.published).await
    }
}

#[tokio::test]
async fn shortcuts_register_at_start_and_again_only_when_they_change() {
    let mut harness = start().await;
    assert_eq!(
        next_calls(&mut harness.calls, REGISTRATION_CALLS).await,
        registration(META_M, META_SHIFT_M)
    );
    harness.focus_desktop().await;

    harness
        .config
        .send_modify(|config| config.appearance.panel_fill = 40);
    // Once the press's view is published, the loop has also taken the change above, which the
    // config channel would otherwise merge into the next one.
    harness.press("monitor").await;
    harness
        .config
        .send_modify(|config| config.shortcuts.monitor = "Meta+N".to_owned());

    // Had the unrelated change registered again, these would still be the old keys.
    assert_eq!(
        next_calls(&mut harness.calls, REGISTRATION_CALLS).await,
        registration(META_N, META_SHIFT_M)
    );
}

#[tokio::test]
async fn a_pressed_shortcut_activates_its_view_on_the_desktop() {
    let mut harness = start().await;
    // Presses are listened for before registering, so none can slip in between.
    next_calls(&mut harness.calls, REGISTRATION_CALLS).await;
    harness.focus_desktop().await;

    let snapshot = harness.press("swap").await;

    assert_eq!(snapshot.action, Some(ViewAction::Swap));
    assert_eq!(snapshot.action_seq, 1);
}

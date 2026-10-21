mod common;

use common::bus::{BUS_WAIT, PrivateBus, RunningDesktop, call_desktop};
use ghost_proto::ViewAction;
use ghostd::feeds::shortcuts::Registration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use zbus::{Connection, fdo, interface};

const KGLOBALACCEL: &str = "org.kde.kglobalaccel";
const KGLOBALACCEL_PATH: &str = "/kglobalaccel";
const COMPONENT_PATH: &str = "/component/ghost_shell";
const COMPONENT_INTERFACE: &str = "org.kde.kglobalaccel.Component";
const PRESSED_SIGNAL: &str = "globalShortcutPressed";
const COMPONENT: &str = "ghost-shell";
// SetPresent (2) | NoAutoloading (4), from kglobalaccel 6.31's kglobalaccel_p.h.
const SET_PRESENT_NO_AUTOLOADING: u32 = 6;
// KGlobalAccel::MatchType::Equal.
const MATCH_EQUAL: i32 = 0;
const META_M: i32 = 0x1000_004D;
const META_N: i32 = 0x1000_004E;
const META_SHIFT_M: i32 = 0x1200_004D;
const CALLS_PER_REGISTRATION: usize = 6;
const NO_TIMESTAMP: i64 = 0;

type ShortcutInfo = (
    String,
    String,
    String,
    String,
    String,
    String,
    Vec<i32>,
    Vec<i32>,
);

#[derive(Debug, PartialEq)]
enum Call {
    Register(Vec<String>),
    SetKeys {
        action_id: Vec<String>,
        keys: Vec<Vec<i32>>,
        flags: u32,
    },
    HoldersOf {
        key: Vec<i32>,
        match_type: i32,
    },
    SetInactive(Vec<String>),
}

/// Records what it is asked for, grants every key and reports no other holder. It refuses to
/// release `monitor`, so that a failure there can be shown not to skip `swap`.
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

    #[zbus(name = "globalShortcutsByKey")]
    fn global_shortcuts_by_key(&self, key: (Vec<i32>,), match_type: (i32,)) -> Vec<ShortcutInfo> {
        self.0
            .send(Call::HoldersOf {
                key: key.0,
                match_type: match_type.0,
            })
            .unwrap();
        Vec::new()
    }

    #[zbus(name = "setInactive")]
    fn set_inactive(&self, action_id: Vec<String>) -> fdo::Result<()> {
        let refused = action_id[1] == "monitor";
        self.0.send(Call::SetInactive(action_id)).unwrap();
        if refused {
            return Err(fdo::Error::Failed("refused".to_owned()));
        }
        Ok(())
    }
}

/// A fake KGlobalAccel that owns its bus name, and the calls it receives.
async fn start_kglobalaccel(bus: &PrivateBus) -> (Connection, mpsc::UnboundedReceiver<Call>) {
    let (calls_sender, calls) = mpsc::unbounded_channel();
    let connection = bus
        .builder()
        .name(KGLOBALACCEL)
        .serve_at(KGLOBALACCEL_PATH, FakeKGlobalAccel(calls_sender))
        .build()
        .await
        .unwrap();
    (connection, calls)
}

fn action_id(action: &str, friendly_name: &str) -> Vec<String> {
    [COMPONENT, action, COMPONENT, friendly_name]
        .map(str::to_owned)
        .to_vec()
}

fn monitor_id() -> Vec<String> {
    action_id("monitor", "Toggle the monitor view")
}

fn swap_id() -> Vec<String> {
    action_id("swap", "Next main view")
}

fn sequence(key: i32) -> Vec<i32> {
    vec![key, 0, 0, 0]
}

/// The calls that register one action with one key, then look for other holders of the key.
fn registering(action_id: Vec<String>, key: i32) -> [Call; 3] {
    [
        Call::Register(action_id.clone()),
        Call::SetKeys {
            action_id,
            keys: vec![sequence(key)],
            flags: SET_PRESENT_NO_AUTOLOADING,
        },
        Call::HoldersOf {
            key: sequence(key),
            match_type: MATCH_EQUAL,
        },
    ]
}

fn registration(monitor: i32, swap: i32) -> Vec<Call> {
    registering(monitor_id(), monitor)
        .into_iter()
        .chain(registering(swap_id(), swap))
        .collect()
}

async fn next_calls(calls: &mut mpsc::UnboundedReceiver<Call>, count: usize) -> Vec<Call> {
    timeout(BUS_WAIT, async {
        let mut received = Vec::new();
        while received.len() < count {
            received.push(calls.recv().await.expect("the fake stopped"));
        }
        received
    })
    .await
    .unwrap_or_else(|_| panic!("fewer than {count} calls arrived in time"))
}

/// Waits until both actions got `monitor` and `swap` as their keys, whatever else arrives too.
async fn expect_keys_set(calls: &mut mpsc::UnboundedReceiver<Call>, monitor: i32, swap: i32) {
    let mut missing = vec![
        (monitor_id(), vec![sequence(monitor)]),
        (swap_id(), vec![sequence(swap)]),
    ];
    timeout(BUS_WAIT, async {
        while !missing.is_empty() {
            if let Call::SetKeys {
                action_id, keys, ..
            } = calls.recv().await.expect("the fake stopped")
            {
                missing.retain(|expected| *expected != (action_id.clone(), keys.clone()));
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the keys were not set in time"));
}

async fn press(kglobalaccel: &Connection, shortcut: &str) {
    kglobalaccel
        .emit_signal(
            None::<&str>,
            COMPONENT_PATH,
            COMPONENT_INTERFACE,
            PRESSED_SIGNAL,
            &(COMPONENT, shortcut, NO_TIMESTAMP),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn shortcuts_register_at_start_and_again_only_when_they_change() {
    let bus = PrivateBus::start();
    let (kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    assert_eq!(
        next_calls(&mut calls, CALLS_PER_REGISTRATION).await,
        registration(META_M, META_SHIFT_M)
    );
    let kwin = bus.connect().await;
    call_desktop(&kwin, "Report", &("", true)).await;
    assert!(desktop.next_snapshot().await.active);

    desktop
        .config
        .send_modify(|config| config.appearance.panel_fill = 40);
    // Once the press's view is published, the loop has also taken the change above, which the
    // config channel would otherwise merge into the next one.
    press(&kglobalaccel, "monitor").await;
    desktop.next_snapshot().await;
    desktop
        .config
        .send_modify(|config| config.shortcuts.monitor = "Meta+N".to_owned());

    // Had the unrelated change registered again, these would still be the old keys.
    assert_eq!(
        next_calls(&mut calls, CALLS_PER_REGISTRATION).await,
        registration(META_N, META_SHIFT_M)
    );
}

#[tokio::test]
async fn a_pressed_shortcut_activates_its_view_on_the_desktop() {
    let bus = PrivateBus::start();
    let (kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    // Presses are listened for before registering, so none can slip in between.
    next_calls(&mut calls, CALLS_PER_REGISTRATION).await;
    let kwin = bus.connect().await;
    call_desktop(&kwin, "Report", &("", true)).await;
    desktop.next_snapshot().await;

    press(&kglobalaccel, "swap").await;

    let snapshot = desktop.next_snapshot().await;
    assert_eq!(snapshot.action, Some(ViewAction::Swap));
    assert_eq!(snapshot.action_seq, 1);
}

#[tokio::test]
async fn a_kglobalaccel_that_appears_late_or_restarts_gets_the_shortcuts() {
    let bus = PrivateBus::start();
    let _desktop = RunningDesktop::start(&bus, Registration::default()).await;

    let (first, mut first_calls) = start_kglobalaccel(&bus).await;
    expect_keys_set(&mut first_calls, META_M, META_SHIFT_M).await;

    // KWin hosts KGlobalAccel, so a restarted KWin is a new owner that knows nothing of ghostd.
    first.close().await.unwrap();
    let (_second, mut second_calls) = start_kglobalaccel(&bus).await;
    expect_keys_set(&mut second_calls, META_M, META_SHIFT_M).await;
}

#[tokio::test]
async fn a_clean_stop_releases_only_shortcuts_this_daemon_registered() {
    let bus = PrivateBus::start();
    let (_kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let stopping = bus.connect().await;
    // Like a second ghostd that never got the bus name: it must leave the shortcuts alone.
    Registration::default().release_on(&stopping).await;

    let ours = Registration::default();
    let _desktop = RunningDesktop::start(&bus, ours.clone()).await;
    assert_eq!(
        next_calls(&mut calls, CALLS_PER_REGISTRATION).await,
        registration(META_M, META_SHIFT_M)
    );
    ours.release_on(&stopping).await;

    assert_eq!(
        next_calls(&mut calls, 2).await,
        vec![
            Call::SetInactive(monitor_id()),
            Call::SetInactive(swap_id())
        ]
    );
}

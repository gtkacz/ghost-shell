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
const META: i32 = 0x0100_0022;
const META_DIGIT_1: i32 = 0x1000_0031;
const META_F1: i32 = 0x1000_0000 | 0x0100_0030;
const DEFAULT_TASKS: i32 = 9;
const TASK_SLOTS: usize = 10;
// Four actions with three calls each, the default tasks with three each, then task-10 inactive.
const CALLS_PER_REGISTRATION: usize = 4 * 3 + 9 * 3 + 1;
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

fn launcher_id() -> Vec<String> {
    action_id("launcher", "Open the launcher")
}

fn notifications_id() -> Vec<String> {
    action_id("notifications", "Open the notification centre")
}

fn task_id(number: usize) -> Vec<String> {
    action_id(
        &format!("task-{number}"),
        &format!("Activate pinned app {number}"),
    )
}

/// The calls that register `keys` as the first pinned apps, then free the remaining slots.
fn task_registration(keys: &[i32]) -> Vec<Call> {
    let registered = keys
        .iter()
        .zip(1..)
        .flat_map(|(&key, number)| registering(task_id(number), key));
    let freed = (keys.len() + 1..=TASK_SLOTS).map(|number| Call::SetInactive(task_id(number)));
    registered.chain(freed).collect()
}

fn default_task_keys() -> Vec<i32> {
    (0..DEFAULT_TASKS)
        .map(|digit| META_DIGIT_1 + digit)
        .collect()
}

fn registration(monitor: i32, swap: i32) -> Vec<Call> {
    registering(monitor_id(), monitor)
        .into_iter()
        .chain(registering(swap_id(), swap))
        .chain(registering(launcher_id(), META))
        .chain(registering(notifications_id(), META_N))
        .chain(task_registration(&default_task_keys()))
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

#[tokio::test]
async fn release_covers_the_launcher_and_every_task_shortcut() {
    let bus = PrivateBus::start();
    let (_kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let stopping = bus.connect().await;
    let ours = Registration::default();
    let _desktop = RunningDesktop::start(&bus, ours.clone()).await;
    next_calls(&mut calls, CALLS_PER_REGISTRATION).await;
    ours.release_on(&stopping).await;

    let released: Vec<Call> = [monitor_id(), swap_id(), launcher_id(), notifications_id()]
        .into_iter()
        .chain((1..=TASK_SLOTS).map(task_id))
        .map(Call::SetInactive)
        .collect();
    assert_eq!(next_calls(&mut calls, released.len()).await, released);
}

#[tokio::test]
async fn launcher_shortcut_registers_bare_meta() {
    let bus = PrivateBus::start();
    let (kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    let registered = next_calls(&mut calls, CALLS_PER_REGISTRATION).await;
    assert!(registered.contains(&Call::SetKeys {
        action_id: launcher_id(),
        keys: vec![sequence(META)],
        flags: SET_PRESENT_NO_AUTOLOADING,
    }));

    press(&kglobalaccel, "launcher").await;

    assert_eq!(desktop.next_snapshot().await.launcher_seq, 1);
}

#[tokio::test]
async fn task_shortcut_publishes_its_index() {
    let bus = PrivateBus::start();
    let (kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    next_calls(&mut calls, CALLS_PER_REGISTRATION).await;

    // The desktop is not active, which a task press must not care about.
    press(&kglobalaccel, "task-3").await;

    let snapshot = desktop.next_snapshot().await;
    assert_eq!(snapshot.task, Some(2));
    assert_eq!(snapshot.task_seq, 1);
}

#[tokio::test]
async fn task_shortcuts_follow_config() {
    let bus = PrivateBus::start();
    let (_kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let desktop = RunningDesktop::start(&bus, Registration::default()).await;
    next_calls(&mut calls, CALLS_PER_REGISTRATION).await;

    desktop
        .config
        .send_modify(|config| config.shortcuts.tasks = vec!["Meta+F1".to_owned()]);

    let calls = next_calls(&mut calls, 4 * 3 + 3 + 9).await;
    assert_eq!(calls[12..], task_registration(&[META_F1]));
}

#[tokio::test]
async fn view_actions_still_need_the_desktop() {
    let bus = PrivateBus::start();
    let (kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    next_calls(&mut calls, CALLS_PER_REGISTRATION).await;

    press(&kglobalaccel, "monitor").await;
    press(&kglobalaccel, "launcher").await;

    // The launcher press is published, and had monitor counted it would be first.
    let snapshot = desktop.next_snapshot().await;
    assert_eq!((snapshot.action_seq, snapshot.launcher_seq), (0, 1));
}

#[tokio::test]
async fn notifications_shortcut_bumps_the_seq_whatever_has_focus() {
    let bus = PrivateBus::start();
    let (kglobalaccel, mut calls) = start_kglobalaccel(&bus).await;
    let mut desktop = RunningDesktop::start(&bus, Registration::default()).await;
    let registered = next_calls(&mut calls, CALLS_PER_REGISTRATION).await;
    assert!(registered.contains(&Call::SetKeys {
        action_id: notifications_id(),
        keys: vec![sequence(META_N)],
        flags: SET_PRESENT_NO_AUTOLOADING,
    }));

    press(&kglobalaccel, "notifications").await;
    press(&kglobalaccel, "notifications").await;

    let first = desktop.next_snapshot().await;
    assert!(!first.active);
    assert_eq!((first.notifications_seq, first.action_seq), (1, 0));
    assert_eq!(desktop.next_snapshot().await.notifications_seq, 2);
}

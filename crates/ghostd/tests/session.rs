mod common;

use std::collections::BTreeSet;
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus};
use common::session::{FakeLogin1, FakeScreenSaver, emit_active_changed_from};
use ghost_proto::{GreetingKind, ResolvedConfig, SessionSnapshot, topics};
use ghostd::feeds::session;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use zbus::Connection;

const SESSION_ID: &str = "2";
/// Long enough for a signal to cross a private bus and be handled, short enough to keep the test
/// quick.
const QUIET_FOR: Duration = Duration::from_millis(400);

/// The real feed on two private buses, one standing in for the system bus and one for the session
/// bus, and everything it publishes.
struct RunningSession {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    _demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _task: JoinHandle<()>,
}

impl RunningSession {
    async fn start(session_bus: &PrivateBus, system_bus: Option<&PrivateBus>) -> Self {
        let system = match system_bus {
            Some(bus) => Some(bus.connect().await),
            None => None,
        };
        Self::start_on(session_bus.connect().await, system)
    }

    fn start_on(session_bus: Connection, system: Option<Connection>) -> Self {
        let (published_sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
        };
        Self {
            published,
            _demand: demand,
            _config: config,
            _task: tokio::spawn(session::feed_on(session_bus, system).run(context)),
        }
    }

    async fn next_snapshot(&mut self) -> SessionSnapshot {
        let (topic, data) = timeout(BUS_WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .expect("the feed stopped");
        assert_eq!(topic, topics::SESSION);
        serde_json::from_value(data).unwrap()
    }

    async fn snapshot_where(
        &mut self,
        wanted: impl Fn(&SessionSnapshot) -> bool,
    ) -> SessionSnapshot {
        loop {
            let snapshot = self.next_snapshot().await;
            if wanted(&snapshot) {
                return snapshot;
            }
        }
    }

    async fn assert_quiet(&mut self) {
        if let Ok(published) = timeout(QUIET_FOR, self.published.recv()).await {
            panic!("expected nothing to be published, got {published:?}");
        }
    }
}

#[tokio::test]
async fn publishes_locked_state_read_at_start() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let saver = FakeScreenSaver::start(&session_bus, true).await;
    let login = FakeLogin1::start(&system_bus, SESSION_ID).await;

    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;

    assert_eq!(
        running.next_snapshot().await,
        SessionSnapshot {
            id: SESSION_ID.into(),
            locked: true,
            sleeping: false,
            greeting: None,
            greeting_seq: 0,
        }
    );
    saver.calls.assert_none_could_start_the_service();
    login.calls.assert_none_could_start_the_service();
}

#[tokio::test]
async fn active_changed_drives_unlock_greeting() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let saver = FakeScreenSaver::start(&session_bus, false).await;
    let _login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    assert!(!running.next_snapshot().await.locked);

    saver.set_active(true).await;
    let locked = running.snapshot_where(|snapshot| snapshot.locked).await;
    assert_eq!(locked.greeting_seq, 0);
    saver.set_active(false).await;
    let unlocked = running.next_snapshot().await;

    assert!(!unlocked.locked);
    assert_eq!(unlocked.greeting, Some(GreetingKind::Unlock));
    assert_eq!(unlocked.greeting_seq, 1);
}

#[tokio::test]
async fn prepare_for_sleep_marks_sleeping_and_wake() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let _saver = FakeScreenSaver::start(&session_bus, false).await;
    let login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    running.next_snapshot().await;

    login.prepare_for_sleep(true).await;
    let asleep = running.next_snapshot().await;
    login.prepare_for_sleep(false).await;
    let awake = running.next_snapshot().await;

    assert!(asleep.sleeping);
    assert_eq!(asleep.greeting_seq, 0);
    assert!(!awake.sleeping);
    assert_eq!(awake.greeting, Some(GreetingKind::Wake));
    assert_eq!(awake.greeting_seq, 1);
}

#[tokio::test]
async fn resume_while_locked_greets_once_at_the_unlock() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let saver = FakeScreenSaver::start(&session_bus, true).await;
    let login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    assert!(running.next_snapshot().await.locked);

    login.prepare_for_sleep(true).await;
    running.snapshot_where(|snapshot| snapshot.sleeping).await;
    login.prepare_for_sleep(false).await;
    let resumed = running.snapshot_where(|snapshot| !snapshot.sleeping).await;
    saver.set_active(false).await;
    let unlocked = running.next_snapshot().await;

    assert_eq!(resumed.greeting_seq, 0);
    assert_eq!(unlocked.greeting, Some(GreetingKind::Wake));
    assert_eq!(unlocked.greeting_seq, 1);
    running.assert_quiet().await;
}

#[tokio::test]
async fn a_new_logind_owner_ends_a_sleep_whose_resume_was_missed() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let _saver = FakeScreenSaver::start(&session_bus, false).await;
    let login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    running.next_snapshot().await;
    login.prepare_for_sleep(true).await;
    running.snapshot_where(|snapshot| snapshot.sleeping).await;

    login.release().await;
    let restarted = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let snapshot = running.snapshot_where(|snapshot| !snapshot.sleeping).await;

    assert_eq!(snapshot.greeting, Some(GreetingKind::Wake));
    restarted.calls.assert_none_could_start_the_service();
}

#[tokio::test]
async fn reads_session_id_from_user_display() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let _saver = FakeScreenSaver::start(&session_bus, false).await;
    let _login = FakeLogin1::start(&system_bus, "7").await;

    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;

    assert_eq!(running.next_snapshot().await.id, "7");
}

#[tokio::test]
async fn screensaver_appearing_later_is_read_in_full() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let saver = FakeScreenSaver::serve(&session_bus, true).await;
    let _login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    assert!(!running.next_snapshot().await.locked);

    saver.claim().await;
    let snapshot = running.snapshot_where(|snapshot| snapshot.locked).await;

    assert_eq!(snapshot.greeting_seq, 0);
    saver.calls.assert_none_could_start_the_service();
}

#[tokio::test]
async fn signals_from_a_non_owner_are_ignored() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let saver = FakeScreenSaver::start(&session_bus, false).await;
    let _login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    running.next_snapshot().await;
    saver.set_active(true).await;
    running.snapshot_where(|snapshot| snapshot.locked).await;
    let stranger = session_bus.connect().await;

    emit_active_changed_from(&stranger, false).await;

    running.assert_quiet().await;
    saver.set_active(false).await;
    assert_eq!(running.next_snapshot().await.greeting_seq, 1);
}

#[tokio::test]
async fn no_system_bus_still_tracks_the_lock() {
    let session_bus = PrivateBus::start();
    let saver = FakeScreenSaver::start(&session_bus, false).await;
    let mut running = RunningSession::start(&session_bus, None).await;
    assert_eq!(running.next_snapshot().await.id, "");

    saver.set_active(true).await;
    running.snapshot_where(|snapshot| snapshot.locked).await;
    saver.set_active(false).await;
    let snapshot = running.next_snapshot().await;

    assert_eq!(snapshot.greeting, Some(GreetingKind::Unlock));
    assert_eq!(snapshot.id, "");
}

#[tokio::test]
async fn losing_the_screensaver_unlocks_without_a_greeting() {
    let (session_bus, system_bus) = (PrivateBus::start(), PrivateBus::start());
    let saver = FakeScreenSaver::start(&session_bus, true).await;
    let _login = FakeLogin1::start(&system_bus, SESSION_ID).await;
    let mut running = RunningSession::start(&session_bus, Some(&system_bus)).await;
    assert!(running.next_snapshot().await.locked);

    saver.release().await;
    let snapshot = running.next_snapshot().await;

    assert!(!snapshot.locked);
    assert_eq!(snapshot.greeting, None);
    assert_eq!(snapshot.greeting_seq, 0);
}

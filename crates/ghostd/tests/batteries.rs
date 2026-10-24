mod common;

use std::collections::BTreeSet;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus};
use common::kdeconnect::{FakeKdeConnect, PHONE_BATTERY, PhoneBattery};
use common::upower::{DUALSENSE_ID, DUALSENSE_PCT, FakeUpower, MOUSE_ID, STATE_CHARGING};
use ghost_proto::{Batteries, Battery, BatteryKind, ResolvedConfig, topics};
use ghostd::feeds::batteries;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, yield_now};
use tokio::time::{Instant, sleep};

const NOW_S: u64 = 1_792_500_000;
const LATE_BY: Duration = Duration::from_secs(5);
const UNWANTED_FOR: Duration = Duration::from_secs(3 * 60);
const KDE_CONNECT_POLL: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);
const POLLS_WATCHED: i32 = 3;
const PHONE: &str = "kdeconnect:abc123";
const NO_BATTERY: PhoneBattery = PhoneBattery {
    has_battery: false,
    charge: -1,
    charging: false,
};
const OUT_OF_RANGE: PhoneBattery = PhoneBattery {
    has_battery: true,
    charge: 101,
    charging: false,
};
const CHARGE_ZERO: PhoneBattery = PhoneBattery {
    has_battery: true,
    charge: 0,
    charging: false,
};

/// Wall-clock seconds that follow the paused tokio clock from `NOW_S`, moved on or back by however
/// much the wall clock was jumped on its own.
struct FakeWallClock {
    start: Instant,
    jumped_s: AtomicI64,
}

impl FakeWallClock {
    fn now_s(&self) -> u64 {
        let followed_s = NOW_S + self.start.elapsed().as_secs();
        followed_s.wrapping_add_signed(self.jumped_s.load(Ordering::SeqCst))
    }

    fn set_back(&self, duration: Duration) {
        let seconds = i64::try_from(duration.as_secs()).unwrap();
        self.jumped_s.fetch_sub(seconds, Ordering::SeqCst);
    }
}

/// The real feed on two private buses, one standing in for the system bus and one for the
/// session bus, and everything it publishes.
struct RunningBatteries {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    clock: Arc<FakeWallClock>,
    _demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    _task: JoinHandle<()>,
}

impl RunningBatteries {
    async fn start(system: &PrivateBus, session: &PrivateBus) -> Self {
        Self::start_with(system, session, ResolvedConfig::default(), true).await
    }

    async fn start_with(
        system: &PrivateBus,
        session: &PrivateBus,
        config: ResolvedConfig,
        wanted: bool,
    ) -> Self {
        let (published_sender, published) = mpsc::unbounded_channel();
        let topics = if wanted {
            BTreeSet::from([topics::BATTERIES.to_owned()])
        } else {
            BTreeSet::new()
        };
        let (demand, demand_receiver) = watch::channel(topics);
        let (config, config_receiver) = watch::channel(config);
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
            calls: tokio::sync::broadcast::channel(64).1,
            topics: tokio::sync::broadcast::channel(1).1,
        };
        let clock = Arc::new(FakeWallClock {
            start: Instant::now(),
            jumped_s: AtomicI64::new(0),
        });
        let wall_clock = Arc::clone(&clock);
        let feed = batteries::feed_on(
            system.connect().await,
            session.connect().await,
            Arc::new(move || wall_clock.now_s()),
        );
        Self {
            published,
            clock,
            _demand: demand,
            _config: config,
            _task: tokio::spawn(feed.run(context)),
        }
    }

    /// Waits on the wall clock rather than tokio's: under a paused clock tokio jumps ahead
    /// whenever it would wait on a private bus, which runs in another process. Spinning keeps the
    /// clock still while the bus answers.
    async fn next_snapshot(&mut self) -> Batteries {
        let deadline = std::time::Instant::now() + BUS_WAIT;
        loop {
            match self.published.try_recv() {
                Ok((topic, data)) => {
                    assert_eq!(topic, topics::BATTERIES);
                    return serde_json::from_value(data).unwrap();
                }
                Err(TryRecvError::Empty) => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "nothing was published in time"
                    );
                    yield_now().await;
                }
                Err(TryRecvError::Disconnected) => panic!("the feed stopped"),
            }
        }
    }

    async fn snapshot_where(&mut self, wanted: impl Fn(&Batteries) -> bool) -> Batteries {
        loop {
            let snapshot = self.next_snapshot().await;
            if wanted(&snapshot) {
                return snapshot;
            }
        }
    }

    /// The last snapshot published so far.
    fn latest(&mut self) -> Option<Batteries> {
        let mut latest = None;
        while let Ok((_, data)) = self.published.try_recv() {
            latest = Some(serde_json::from_value(data).unwrap());
        }
        latest
    }
}

/// Awaits `future` without letting the paused clock jump ahead meanwhile, as `next_snapshot` does,
/// so a timer of the feed cannot fire while the bus answers.
async fn without_time_passing<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    loop {
        tokio::select! {
            biased;
            output = &mut future => return output,
            () = yield_now() => {}
        }
    }
}

fn has_devices(snapshot: &Batteries) -> bool {
    !snapshot.devices.is_empty()
}

fn device<'a>(snapshot: &'a Batteries, id: &str) -> Option<&'a Battery> {
    snapshot.devices.iter().find(|battery| battery.id == id)
}

fn phone_reads(pct: f32) -> impl Fn(&Batteries) -> bool {
    move |snapshot| device(snapshot, PHONE).is_some_and(|phone| phone.pct == Some(pct))
}

/// The first battery the feed reads from UPower, with KDE Connect absent.
async fn dualsense_with(config: ResolvedConfig) -> Battery {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let upower = FakeUpower::start(&system).await;
    let mut running = RunningBatteries::start_with(&system, &session, config, true).await;
    let snap = running.snapshot_where(has_devices).await;
    upower.calls.assert_none_could_start_the_service();
    snap.devices[0].clone()
}

/// The charge the feed reads for a phone whose battery plugin reports `battery`.
async fn phone_pct(battery: PhoneBattery) -> Option<f32> {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let _kdeconnect = FakeKdeConnect::start(&session, battery).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    let snap = running.snapshot_where(has_devices).await;
    device(&snap, PHONE).expect("the phone is listed").pct
}

#[tokio::test(start_paused = true)]
async fn a_dualsense_reads_as_a_controller() {
    let b = dualsense_with(ResolvedConfig::default()).await;

    assert_eq!(b.kind, BatteryKind::Controller);
    assert_eq!(b.label, "DUALSENSE WIRELESS CONTROLLER");
    assert_eq!(b.pct, Some(80.0));
    assert!(!b.charging);
}

#[tokio::test(start_paused = true)]
async fn the_display_device_and_power_supplies_are_skipped() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let _upower = FakeUpower::start(&system).await;
    let mut running = RunningBatteries::start(&system, &session).await;

    let snap = running.snapshot_where(has_devices).await;

    assert_eq!(
        snap.devices
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        [DUALSENSE_ID]
    );
}

#[tokio::test(start_paused = true)]
async fn a_paired_reachable_phone_reports_its_charge() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let kdeconnect = FakeKdeConnect::start(&session, PHONE_BATTERY).await;
    let mut running = RunningBatteries::start(&system, &session).await;

    let snap = running.snapshot_where(has_devices).await;

    let [phone] = snap.devices.as_slice() else {
        panic!("expected only the paired, reachable phone: {snap:?}");
    };
    assert_eq!(phone.pct, Some(15.0));
    assert_eq!(phone.id, "kdeconnect:abc123");
    assert_eq!(
        (phone.kind, phone.label.as_str(), phone.charging),
        (BatteryKind::Phone, "PHONE", true)
    );
    kdeconnect.calls.assert_none_could_start_the_service();
}

#[tokio::test(start_paused = true)]
async fn no_refresh_is_ever_requested_from_the_phone() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let kdeconnect = FakeKdeConnect::start(&session, PHONE_BATTERY).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    running.snapshot_where(phone_reads(15.0)).await;

    for poll in 1..=POLLS_WATCHED {
        let charge = PHONE_BATTERY.charge - poll;
        kdeconnect.set_phone_charge(charge).await;
        sleep(KDE_CONNECT_POLL).await;
        running.snapshot_where(phone_reads(charge as f32)).await;
    }

    assert_eq!(kdeconnect.method_calls_except(&["devices"]), 0);
    assert_eq!(
        kdeconnect.calls.count_of("devices"),
        usize::try_from(POLLS_WATCHED + 1).unwrap()
    );
    kdeconnect.calls.assert_none_could_start_the_service();
}

#[tokio::test(start_paused = true)]
async fn upower_appearing_later_is_read_in_full() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let upower = FakeUpower::serve(&system).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    let before = running.next_snapshot().await;

    sleep(LATE_BY).await;
    upower.claim().await;

    let after = running.snapshot_where(has_devices).await;
    assert!(before.devices.is_empty());
    assert_eq!(after.devices.len(), 1);
    assert_eq!(after.devices[0].pct, Some(80.0));
    upower.calls.assert_none_could_start_the_service();
}

#[tokio::test(start_paused = true)]
async fn upower_going_away_empties_its_devices() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let upower = FakeUpower::start(&system).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    running.snapshot_where(has_devices).await;

    upower.release().await;

    let after = running.snapshot_where(|snap| !has_devices(snap)).await;
    assert!(after.devices.is_empty());
}

#[tokio::test(start_paused = true)]
async fn kde_connect_appearing_later_is_read_without_waiting_for_the_poll() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let kdeconnect = FakeKdeConnect::serve(&session, PHONE_BATTERY).await;
    let started = Instant::now();
    let mut running = RunningBatteries::start(&system, &session).await;
    let before = running.next_snapshot().await;

    sleep(LATE_BY).await;
    without_time_passing(kdeconnect.claim()).await;

    let after = running.snapshot_where(has_devices).await;
    assert!(before.devices.is_empty());
    assert_eq!(device(&after, PHONE).unwrap().pct, Some(15.0));
    assert!(started.elapsed() < KDE_CONNECT_POLL);
}

#[tokio::test(start_paused = true)]
async fn a_device_reporting_a_new_charge_is_read_again() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let upower = FakeUpower::start(&system).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    running.snapshot_where(has_devices).await;

    upower.set_dualsense(55.0, STATE_CHARGING).await;

    let after = running
        .snapshot_where(|snap| snap.devices[0].pct == Some(55.0))
        .await;
    assert!(after.devices[0].charging);
}

#[tokio::test(start_paused = true)]
async fn a_device_added_later_is_shown() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let upower = FakeUpower::start(&system).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    running.snapshot_where(has_devices).await;

    upower.add_mouse(40.0).await;

    let after = running
        .snapshot_where(|snap| device(snap, MOUSE_ID).is_some())
        .await;
    let mouse = device(&after, MOUSE_ID).unwrap();
    assert_eq!(
        (mouse.kind, mouse.label.as_str(), mouse.pct),
        (BatteryKind::Mouse, "MX MASTER 3", Some(40.0))
    );
    assert_eq!(
        device(&after, DUALSENSE_ID).unwrap().pct,
        Some(DUALSENSE_PCT as f32)
    );
}

#[tokio::test(start_paused = true)]
async fn a_phone_that_never_reported_has_no_charge() {
    assert_eq!(phone_pct(NO_BATTERY).await, None);
    assert_eq!(phone_pct(OUT_OF_RANGE).await, None);
    assert_eq!(phone_pct(CHARGE_ZERO).await, Some(0.0));
}

#[tokio::test(start_paused = true)]
async fn the_phone_is_read_again_every_minute() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let kdeconnect = FakeKdeConnect::start(&session, PHONE_BATTERY).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    running.snapshot_where(phone_reads(15.0)).await;

    kdeconnect.set_phone_charge(14).await;
    sleep(KDE_CONNECT_POLL - Duration::from_secs(1)).await;
    assert!(running.latest().is_none(), "read before the minute was up");
    sleep(Duration::from_secs(1)).await;

    running.snapshot_where(phone_reads(14.0)).await;
}

#[tokio::test(start_paused = true)]
async fn a_wall_clock_set_back_does_not_hold_the_poll_back() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let kdeconnect = FakeKdeConnect::start(&session, PHONE_BATTERY).await;
    let mut running = RunningBatteries::start(&system, &session).await;
    running.snapshot_where(phone_reads(15.0)).await;

    running.clock.set_back(HOUR);
    kdeconnect.set_phone_charge(14).await;
    sleep(KDE_CONNECT_POLL).await;

    running.snapshot_where(phone_reads(14.0)).await;
}

#[tokio::test(start_paused = true)]
async fn a_config_label_replaces_the_model() {
    let mut config = ResolvedConfig::default();
    config
        .batteries
        .labels
        .insert(DUALSENSE_ID.to_owned(), "PAD".to_owned());

    let b = dualsense_with(config).await;

    assert_eq!(b.label, "PAD");
}

#[tokio::test(start_paused = true)]
async fn nothing_runs_while_unwanted() {
    let (system, session) = (PrivateBus::start(), PrivateBus::start());
    let upower = FakeUpower::start(&system).await;
    let kdeconnect = FakeKdeConnect::start(&session, PHONE_BATTERY).await;
    let mut running =
        RunningBatteries::start_with(&system, &session, ResolvedConfig::default(), false).await;

    sleep(UNWANTED_FOR).await;

    assert_eq!((upower.calls.count(), kdeconnect.calls.count()), (0, 0));
    assert!(running.latest().is_none());
}

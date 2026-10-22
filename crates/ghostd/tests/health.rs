mod common;

use std::collections::BTreeSet;
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use common::bus::{BUS_WAIT, PrivateBus};
use common::udisks::{
    ATA_SERIAL, FakeUdisks, NEVER_READ_SERIAL, NVME, NVME_SERIAL, Stuck, UdisksShape,
};
use ghost_proto::{DiskHealth, DriveHealth, ResolvedConfig, topics};
use ghostd::feeds::health;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, yield_now};
use tokio::time::{Instant, sleep};

const NOW_S: u64 = 1_792_632_000;
const SUSPENDED_FOR: Duration = Duration::from_secs(8 * 60 * 60);
const WALL_CLOCK_CHECK: Duration = Duration::from_secs(60);
const HEALTH_EVERY: Duration = Duration::from_secs(10 * 60);
const UDISKS_LATE_BY: Duration = Duration::from_secs(30);
const UNWANTED_FOR: Duration = Duration::from_secs(20 * 60);
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Wall-clock seconds that follow the paused tokio clock from `NOW_S`, moved on by however much
/// the machine was suspended.
struct FakeWallClock {
    start: Instant,
    suspended_s: AtomicU64,
}

impl FakeWallClock {
    fn now_s(&self) -> u64 {
        NOW_S + self.start.elapsed().as_secs() + self.suspended_s.load(Ordering::SeqCst)
    }

    /// The wall clock moves on by `duration` while the monotonic clock stands still.
    fn suspend_for(&self, duration: Duration) {
        self.suspended_s
            .fetch_add(duration.as_secs(), Ordering::SeqCst);
    }
}

/// The real feed on a private bus, and everything it publishes.
struct RunningHealth {
    published: mpsc::UnboundedReceiver<(String, Value)>,
    clock: Arc<FakeWallClock>,
    _demand: watch::Sender<BTreeSet<String>>,
    _config: watch::Sender<ResolvedConfig>,
    task: JoinHandle<()>,
}

impl RunningHealth {
    async fn start(bus: &PrivateBus) -> Self {
        Self::start_wanting(bus, BTreeSet::from([topics::DISKS_HEALTH.to_owned()])).await
    }

    async fn start_wanting(bus: &PrivateBus, wanted: BTreeSet<String>) -> Self {
        let (published_sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(wanted);
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
        };
        let clock = Arc::new(FakeWallClock {
            start: Instant::now(),
            suspended_s: AtomicU64::new(0),
        });
        let wall_clock = Arc::clone(&clock);
        let feed = health::feed_on(bus.connect().await, Arc::new(move || wall_clock.now_s()));
        Self {
            published,
            clock,
            _demand: demand,
            _config: config,
            task: tokio::spawn(feed.run(context)),
        }
    }

    /// Waits on the wall clock rather than tokio's: under a paused clock tokio jumps ahead
    /// whenever it would wait on the private bus, which runs in another process. Spinning keeps
    /// the clock still while the bus answers.
    async fn next_snapshot(&mut self) -> DiskHealth {
        let deadline = std::time::Instant::now() + BUS_WAIT;
        loop {
            match self.published.try_recv() {
                Ok((topic, data)) => {
                    assert_eq!(topic, topics::DISKS_HEALTH);
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

    async fn snapshot_where(&mut self, wanted: impl Fn(&DiskHealth) -> bool) -> DiskHealth {
        loop {
            let snapshot = self.next_snapshot().await;
            if wanted(&snapshot) {
                return snapshot;
            }
        }
    }

    fn nothing_published(&mut self) -> bool {
        matches!(self.published.try_recv(), Err(TryRecvError::Empty))
    }
}

/// Awaits `future` without letting the paused clock jump ahead meanwhile, as `next_snapshot` does,
/// so the feed's read timeout cannot fire while the bus answers.
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

/// Waits, with the paused clock held still, until the fake has received `count` calls of `member`:
/// a call still on its way through the bus must not see the feed's timeout fire before it lands.
async fn until_called(udisks: &FakeUdisks, member: &str, count: usize) {
    let deadline = std::time::Instant::now() + BUS_WAIT;
    while udisks.calls(member) < count {
        assert!(
            std::time::Instant::now() < deadline,
            "{member} was not called {count} times in time"
        );
        yield_now().await;
    }
}

fn drive<'a>(snapshot: &'a DiskHealth, serial: &str) -> &'a DriveHealth {
    snapshot
        .drives
        .iter()
        .find(|drive| drive.serial == serial)
        .unwrap_or_else(|| panic!("no drive {serial:?} in {snapshot:?}"))
}

/// The first snapshot from the feed over a fake UDisks2 of `shape`, with the fake kept for its
/// call counts.
async fn first_snapshot(bus: &PrivateBus, shape: UdisksShape) -> (DiskHealth, FakeUdisks) {
    let udisks = FakeUdisks::start(bus, shape).await;
    let mut running = RunningHealth::start(bus).await;
    let snapshot = running.next_snapshot().await;
    running.task.abort();
    (snapshot, udisks)
}

#[tokio::test(start_paused = true)]
async fn wear_and_hours_come_from_the_cache() {
    let bus = PrivateBus::start();
    let (snap, _udisks) = first_snapshot(&bus, UdisksShape::default()).await;

    assert_eq!(
        drive(&snap, NVME_SERIAL),
        &DriveHealth {
            serial: "2O412LA45EG7".into(),
            wear_pct: Some(3),
            power_on_hours: Some(12_345),
        }
    );
}

#[tokio::test(start_paused = true)]
async fn smart_update_is_never_called() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::start(&bus, UdisksShape::default()).await;
    let mut running = RunningHealth::start(&bus).await;
    running.next_snapshot().await;
    running.clock.suspend_for(HEALTH_EVERY);
    sleep(WALL_CLOCK_CHECK).await;
    running.next_snapshot().await;

    assert_eq!(udisks.calls("SmartUpdate"), 0);
    // The counter does see calls: the one method the feed may call, once per read.
    assert_eq!(udisks.calls("SmartGetAttributes"), 2);
}

#[tokio::test(start_paused = true)]
async fn ata_drives_are_read_from_properties_only() {
    let bus = PrivateBus::start();
    let (snap, udisks) = first_snapshot(&bus, UdisksShape::default()).await;

    assert_eq!(
        udisks.method_calls_on("org.freedesktop.UDisks2.Drive.Ata"),
        0
    );
    assert_eq!(udisks.method_calls_on(NVME), 1);
    assert_eq!(drive(&snap, "WD-BLUE").power_on_hours, Some(20_000));
    assert_eq!(drive(&snap, ATA_SERIAL).wear_pct, None);
}

#[tokio::test(start_paused = true)]
async fn a_drive_never_read_reports_none() {
    let bus = PrivateBus::start();
    let shape = UdisksShape {
        never_read_drive: true,
        ..UdisksShape::default()
    };
    let (snap, udisks) = first_snapshot(&bus, shape).await;

    let d = drive(&snap, NEVER_READ_SERIAL);
    assert_eq!((d.wear_pct, d.power_on_hours), (None, None));
    assert_eq!(drive(&snap, NVME_SERIAL).wear_pct, Some(3));
    assert_eq!(
        udisks.calls("SmartGetAttributes"),
        1,
        "a drive never read has no cache to ask"
    );
}

#[tokio::test(start_paused = true)]
async fn not_authorized_leaves_wear_unknown_and_keeps_hours() {
    let bus = PrivateBus::start();
    let shape = UdisksShape {
        attributes_denied: true,
        ..UdisksShape::default()
    };
    let (snap, udisks) = first_snapshot(&bus, shape).await;

    let d = drive(&snap, "2O412LA45EG7");
    assert_eq!((d.wear_pct, d.power_on_hours), (None, Some(12_345)));
    assert_eq!(snap.drives.len(), 2);
    assert_eq!(drive(&snap, ATA_SERIAL).power_on_hours, Some(20_000));
    assert_eq!(udisks.calls("SmartGetAttributes"), 1);
}

#[tokio::test(start_paused = true)]
async fn a_resume_after_the_cadence_refreshes_once() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::start(&bus, UdisksShape::default()).await;
    let mut running = RunningHealth::start(&bus).await;
    running.next_snapshot().await;
    assert_eq!(udisks.calls("GetManagedObjects"), 1);

    running.clock.suspend_for(SUSPENDED_FOR);
    sleep(WALL_CLOCK_CHECK).await;
    let refreshed = running.next_snapshot().await;

    assert_eq!(udisks.calls("GetManagedObjects"), 2);
    assert_eq!(refreshed.drives.len(), 2);
    sleep(HEALTH_EVERY / 2).await;
    assert_eq!(udisks.calls("GetManagedObjects"), 2);
    assert!(running.nothing_published());
}

#[tokio::test(start_paused = true)]
async fn udisks_appearing_later_is_read_in_full() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::serve(&bus, UdisksShape::default()).await;
    let mut running = RunningHealth::start(&bus).await;
    let before = running.next_snapshot().await;

    sleep(UDISKS_LATE_BY).await;
    without_time_passing(udisks.claim()).await;

    let after = running.snapshot_where(|snap| !snap.drives.is_empty()).await;
    assert!(before.drives.is_empty());
    assert_eq!(after.drives.len(), 2);
    assert_eq!(drive(&after, "2O412LA45EG7").wear_pct, Some(3));
}

#[tokio::test(start_paused = true)]
async fn udisks_leaving_publishes_no_drives_and_keeps_running() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::start(&bus, UdisksShape::default()).await;
    let mut running = RunningHealth::start(&bus).await;
    running.next_snapshot().await;

    without_time_passing(udisks.release()).await;

    let gone = running.snapshot_where(|snap| snap.drives.is_empty()).await;
    assert_eq!(gone, DiskHealth::default());
    without_time_passing(udisks.claim()).await;
    let back = running.snapshot_where(|snap| !snap.drives.is_empty()).await;
    assert_eq!(back.drives.len(), 2);
    assert!(!running.task.is_finished());
}

#[tokio::test(start_paused = true)]
async fn nothing_runs_while_unwanted() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::start(&bus, UdisksShape::default()).await;
    let mut running = RunningHealth::start_wanting(&bus, BTreeSet::new()).await;

    sleep(UNWANTED_FOR).await;

    assert_eq!(udisks.total_calls(), 0);
    assert!(running.nothing_published());
    assert!(!running.task.is_finished());
}

#[tokio::test(start_paused = true)]
async fn a_udisks_that_never_answers_publishes_nothing_and_is_read_again_when_it_restarts() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::start(&bus, UdisksShape::default()).await;
    udisks.stick(Stuck::ManagedObjects);
    let mut running = RunningHealth::start(&bus).await;

    until_called(&udisks, "GetManagedObjects", 1).await;
    sleep(READ_TIMEOUT).await;
    assert!(running.nothing_published());
    assert_eq!(udisks.calls("GetManagedObjects"), 1);

    udisks.stick(Stuck::Nothing);
    without_time_passing(udisks.release()).await;
    without_time_passing(udisks.claim()).await;
    let back = running.snapshot_where(|snap| !snap.drives.is_empty()).await;
    assert_eq!(drive(&back, NVME_SERIAL).wear_pct, Some(3));
    assert!(!running.task.is_finished());
}

#[tokio::test(start_paused = true)]
async fn a_stuck_drive_keeps_the_last_health_and_the_cadence() {
    let bus = PrivateBus::start();
    let udisks = FakeUdisks::start(&bus, UdisksShape::default()).await;
    let mut running = RunningHealth::start(&bus).await;
    running.next_snapshot().await;

    udisks.stick(Stuck::Attributes);
    running.clock.suspend_for(HEALTH_EVERY);
    sleep(WALL_CLOCK_CHECK).await;
    until_called(&udisks, "SmartGetAttributes", 2).await;
    sleep(READ_TIMEOUT).await;
    assert_eq!(udisks.calls("SmartGetAttributes"), 2);
    assert!(
        running.nothing_published(),
        "a read cut short must leave the last health standing"
    );

    udisks.stick(Stuck::Nothing);
    sleep(HEALTH_EVERY - WALL_CLOCK_CHECK).await;
    assert_eq!(
        udisks.calls("GetManagedObjects"),
        2,
        "not due before the cadence"
    );
    sleep(WALL_CLOCK_CHECK).await;
    let next = running.next_snapshot().await;
    assert_eq!(udisks.calls("GetManagedObjects"), 3);
    assert_eq!(drive(&next, NVME_SERIAL).wear_pct, Some(3));
}

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use ghost_proto::{Location, ResolvedConfig, SkySnapshot, topics};
use ghostd::feeds::sky::feed_with_clock;
use ghostd::server::{FeedContext, Publisher};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, sleep, timeout, timeout_at};

/// 19:13:20Z on 2026-10-21, the instant the weather feed's tests start at.
const START_S: u64 = 1_792_610_000;
const SAO_PAULO: (f64, f64) = (-23.55, -46.63);
const RIO: (f64, f64) = (-22.91, -43.17);
const SECOND: Duration = Duration::from_secs(1);
const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(3600);
const DAY: Duration = Duration::from_secs(24 * 3600);
const SYNODIC_MONTH_DAYS: f64 = 29.530_588_853;
const SECONDS_PER_DAY: f64 = 86_400.0;
const SYNODIC_MONTH_S: f64 = SYNODIC_MONTH_DAYS * SECONDS_PER_DAY;
/// Rio lies 3.5 degrees of longitude east of São Paulo, which is 14 minutes of sun time, and its
/// latitude moves the sunrise a little more.
const RIO_SUNRISE_LEAD_S: Range<i64> = 5 * 60..30 * 60;
const PHASE_TOLERANCE: f32 = 0.001;
/// Two days on, the sun rises within a couple of minutes of the same time.
const SUNRISE_DRIFT_TOLERANCE_S: u64 = 5 * 60;

/// Wall-clock seconds that follow the paused tokio clock from `START_S`, moved on by however much
/// the wall clock was jumped on its own.
struct FakeWallClock {
    start: Instant,
    jumped_s: AtomicU64,
}

impl FakeWallClock {
    fn now_s(&self) -> u64 {
        START_S + self.start.elapsed().as_secs() + self.jumped_s.load(Ordering::SeqCst)
    }

    /// A suspend: the wall clock moves on by `duration` while the monotonic clock stands still.
    fn suspend_for(&self, duration: Duration) {
        self.jumped_s
            .fetch_add(duration.as_secs(), Ordering::SeqCst);
    }
}

fn config_at(place: Option<(f64, f64)>) -> ResolvedConfig {
    ResolvedConfig {
        location: Location {
            lat: place.map(|(lat, _)| lat),
            lon: place.map(|(_, lon)| lon),
            ..Location::default()
        },
        ..ResolvedConfig::default()
    }
}

/// The sky feed on its own, with its demand, config and output in the test's hands.
struct Running {
    clock: Arc<FakeWallClock>,
    demand: watch::Sender<BTreeSet<String>>,
    config: watch::Sender<ResolvedConfig>,
    published: mpsc::UnboundedReceiver<SkySnapshot>,
}

impl Running {
    fn start(place: Option<(f64, f64)>) -> Self {
        Self::start_wanting(place, &[])
    }

    fn start_wanting(place: Option<(f64, f64)>, wanted: &[&str]) -> Self {
        let clock = Arc::new(FakeWallClock {
            start: Instant::now(),
            jumped_s: AtomicU64::new(0),
        });
        let (demand, demand_rx) =
            watch::channel(wanted.iter().map(|&topic| topic.to_owned()).collect());
        let (config, config_rx) = watch::channel(config_at(place));
        let (sent, published) = mpsc::unbounded_channel();
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                assert_eq!(topic, topics::SKY);
                sent.send(serde_json::from_value(data).unwrap()).unwrap();
            }),
            demand: demand_rx,
            config: config_rx,
            calls: tokio::sync::broadcast::channel(64).1,
            topics: tokio::sync::broadcast::channel(1).1,
        };
        let wall_clock = Arc::clone(&clock);
        let sky = feed_with_clock(Arc::new(move || wall_clock.now_s()));
        tokio::spawn(sky.run(context));
        Self {
            clock,
            demand,
            config,
            published,
        }
    }

    fn want(&self, topics: &[&str]) {
        self.demand
            .send_replace(topics.iter().map(|&topic| topic.to_owned()).collect());
    }

    fn move_to(&self, place: Option<(f64, f64)>) {
        self.config.send_replace(config_at(place));
    }

    async fn next(&mut self) -> SkySnapshot {
        // The paused clock would otherwise run on forever behind a feed that never publishes.
        timeout(DAY, self.published.recv())
            .await
            .expect("nothing was published for a day")
            .expect("the feed stopped")
    }

    async fn expect_nothing_before(&mut self, deadline: Instant) {
        if let Ok(published) = timeout_at(deadline, self.published.recv()).await {
            panic!("published {published:?} before the deadline");
        }
    }
}

fn phase_gained_in(duration: Duration) -> f32 {
    (duration.as_secs_f64() / SYNODIC_MONTH_S) as f32
}

#[tokio::test(start_paused = true)]
async fn recomputes_every_minute_only_while_wanted() {
    let mut sky = Running::start(Some(SAO_PAULO));
    let at_start = sky.next().await;
    assert!(at_start.available);

    sky.expect_nothing_before(Instant::now() + HOUR).await;

    let wanted_at = Instant::now();
    sky.want(&[topics::SKY]);
    let first = sky.next().await;
    assert_eq!(wanted_at.elapsed(), Duration::ZERO);
    let second = sky.next().await;
    assert_eq!(wanted_at.elapsed(), MINUTE);
    let third = sky.next().await;
    assert_eq!(wanted_at.elapsed(), 2 * MINUTE);

    assert!(first.available && first.sunrise_s.is_some() && first.sunset_s.is_some());
    assert!(first.sunrise_s < first.sunset_s);
    assert!(second.moon_phase > first.moon_phase && third.moon_phase > second.moon_phase);

    sky.want(&[]);
    sky.expect_nothing_before(Instant::now() + HOUR).await;
}

#[tokio::test(start_paused = true)]
async fn another_topic_coming_and_going_leaves_the_cadence_alone() {
    let mut sky = Running::start(Some(SAO_PAULO));
    sky.next().await;
    let wanted_at = Instant::now();
    sky.want(&[topics::SKY]);
    sky.next().await;

    sleep(30 * SECOND).await;
    sky.want(&[topics::SKY, topics::CPU]);
    sleep(10 * SECOND).await;
    sky.want(&[topics::SKY]);
    sky.next().await;

    assert_eq!(wanted_at.elapsed(), MINUTE);
}

#[tokio::test(start_paused = true)]
async fn no_location_is_unavailable_without_computing() {
    let mut sky = Running::start(None);
    sky.want(&[topics::SKY]);

    assert_eq!(sky.next().await, SkySnapshot::default());
    sky.expect_nothing_before(Instant::now() + HOUR).await;
}

#[tokio::test(start_paused = true)]
async fn a_location_change_recomputes_at_once_wanted_or_not() {
    let mut sky = Running::start(Some(SAO_PAULO));
    let in_sao_paulo = sky.next().await;

    let moved_at = Instant::now();
    sky.move_to(Some(RIO));
    let in_rio = sky.next().await;
    assert_eq!(moved_at.elapsed(), Duration::ZERO);
    let earlier_s = in_sao_paulo.sunrise_s.unwrap() as i64 - in_rio.sunrise_s.unwrap() as i64;
    assert!(
        RIO_SUNRISE_LEAD_S.contains(&earlier_s),
        "Rio's sunrise is {earlier_s} s earlier"
    );

    sky.want(&[topics::SKY]);
    sky.next().await;
    sky.move_to(None);
    assert_eq!(sky.next().await, SkySnapshot::default());
    sky.expect_nothing_before(Instant::now() + HOUR).await;
}

#[tokio::test(start_paused = true)]
async fn after_a_suspend_the_next_wake_reads_the_wall_clock() {
    let mut sky = Running::start(Some(SAO_PAULO));
    sky.next().await;
    let wanted_at = Instant::now();
    sky.want(&[topics::SKY]);
    let before = sky.next().await;

    sky.clock.suspend_for(2 * DAY);
    let after = sky.next().await;

    let noticed_after = wanted_at.elapsed();
    assert!(noticed_after <= MINUTE + SECOND, "{noticed_after:?}");
    let moon_moved = after.moon_phase - before.moon_phase;
    let expected = phase_gained_in(2 * DAY);
    assert!(
        (moon_moved - expected).abs() < PHASE_TOLERANCE,
        "the phase moved {moon_moved}, not {expected}"
    );
    // The date was read again too, so the sunrise is the one two days on.
    let rise_shift_s = after.sunrise_s.unwrap().abs_diff(before.sunrise_s.unwrap());
    assert!(
        rise_shift_s.abs_diff(2 * DAY.as_secs()) < SUNRISE_DRIFT_TOLERANCE_S,
        "the sunrise moved {rise_shift_s} s"
    );
}

#[tokio::test(start_paused = true)]
async fn a_place_set_while_wanted_is_published_once_then_every_minute() {
    let mut sky = Running::start_wanting(None, &[topics::SKY]);
    assert_eq!(sky.next().await, SkySnapshot::default());
    sky.expect_nothing_before(Instant::now() + 5 * MINUTE).await;

    let placed_at = Instant::now();
    sky.move_to(Some(SAO_PAULO));
    assert!(sky.next().await.available);
    assert_eq!(placed_at.elapsed(), Duration::ZERO);
    sky.next().await;

    assert_eq!(placed_at.elapsed(), MINUTE);
}

#[tokio::test(start_paused = true)]
async fn a_feed_that_starts_wanted_publishes_once_then_every_minute() {
    let started_at = Instant::now();
    let mut sky = Running::start_wanting(Some(SAO_PAULO), &[topics::SKY]);
    assert!(sky.next().await.available);
    assert_eq!(started_at.elapsed(), Duration::ZERO);

    sky.next().await;

    assert_eq!(started_at.elapsed(), MINUTE);
}
